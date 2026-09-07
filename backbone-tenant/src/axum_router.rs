//! The tenant-routing middleware (ADR-0027; features `axum` + a runtime with a database).
//!
//! Sits at the FRONT of the request path: resolve the request's tenant from the host (via
//! [`HostResolver`](crate::resolve::HostResolver)), get-or-build that tenant's runtime from the
//! [`TenantRegistry`](crate::TenantRegistry), and pass the request on with the runtime attached.
//! Everything downstream — auth scopes, module handlers — runs against *this tenant's* runtime,
//! which under database-per-tenant is the whole isolation story: a request can only ever touch the
//! database its host resolved to.
//!
//! Two things land in the request extensions:
//!
//! - `Arc<F::Runtime>` — the tenant's runtime, for handlers/composers that need the whole thing;
//! - `sqlx::PgPool` — when the runtime [`ProvidesDatabase`], its tenant-dedicated pool. This is
//!   the extension the auth middlewares look for: `backbone_auth::company::company_auth` today,
//!   and its org-scope twin, open their request fence scope on exactly this pool. The router
//!   handing over the pool is what makes the fence per-tenant for free.
//!
//! Failing closed: a host that names no tenant gets 404 before any handler runs, and a tenant
//! whose runtime cannot be built (database down, not yet provisioned) gets 503 — never a fallback
//! to some default tenant.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use crate::resolve::{HostResolver, ResolveError};
use crate::{TenantId, TenantRegistry, TenantRuntimeFactory};

/// The header a development profile may send instead of a tenant subdomain. Honored only by a
/// resolver explicitly built with `allow_header_override(true)` — production ignores (and flags)
/// it. Constant here so composers and tests agree on the name.
pub const TENANT_OVERRIDE_HEADER: &str = "x-tenant-slug";

/// A tenant runtime that carries a database pool. Blanket-implemented for a bare `PgPool` (the
/// common small composition); composite runtimes — `(PgPool, Router, …)` — implement it so the
/// router can hand their pool to the auth layers.
pub trait ProvidesDatabase {
    /// The tenant-dedicated pool, if this runtime has one.
    fn database(&self) -> Option<&sqlx::PgPool>;
}

impl ProvidesDatabase for sqlx::PgPool {
    fn database(&self) -> Option<&sqlx::PgPool> {
        Some(self)
    }
}

/// Middleware state: how to resolve a host, and where built runtimes come from.
pub struct TenantRouter<F: TenantRuntimeFactory> {
    /// Host → tenant resolution (subdomain + custom-domain map + gated dev override).
    resolver: HostResolver,
    /// Build-once cache of tenant runtimes.
    registry: Arc<TenantRegistry<F>>,
}

impl<F: TenantRuntimeFactory> Clone for TenantRouter<F> {
    fn clone(&self) -> Self {
        Self { resolver: self.resolver.clone(), registry: Arc::clone(&self.registry) }
    }
}

impl<F: TenantRuntimeFactory> TenantRouter<F> {
    /// Route hosts resolved by `resolver` to runtimes from `registry`.
    pub fn new(resolver: HostResolver, registry: TenantRegistry<F>) -> Self {
        Self { resolver, registry: Arc::new(registry) }
    }

    /// Route over a registry handle the caller also holds — the control-plane shape, where the
    /// same registry is shared with whatever evicts tenants on deprovision.
    pub fn with_registry(resolver: HostResolver, registry: Arc<TenantRegistry<F>>) -> Self {
        Self { resolver, registry }
    }
}

fn error_response(status: StatusCode, error: &str, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error, "message": message }))).into_response()
}

/// Middleware: resolve the tenant, get-or-build its runtime, attach it (and its pool) to the
/// request. Mount outermost, via `axum::middleware::from_fn_with_state(router, tenant_route)`.
///
/// Handlers and inner layers read the tenant's runtime from the extensions — `PgPool` directly,
/// the full runtime as `Arc<YourRuntime>`.
pub async fn tenant_route<F, R>(
    State(router): State<TenantRouter<F>>,
    mut req: Request,
    next: Next,
) -> Response
where
    F: TenantRuntimeFactory<Runtime = R> + 'static,
    R: ProvidesDatabase + Send + Sync + 'static,
{
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let override_header = req
        .headers()
        .get(TENANT_OVERRIDE_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);

    // Resolve before anything else touches a database: a host that names no tenant must not get
    // one picked for it.
    let tenant: TenantId = match router.resolver.resolve(&host, override_header.as_deref()).await {
        Ok(t) => t,
        // A dev header against a strict resolver is a configuration signal worth surfacing;
        // host-shape problems are a flat 404 — they name no tenant, and say nothing more.
        Err(ResolveError::OverrideDisabled) => {
            return error_response(
                StatusCode::FORBIDDEN,
                "tenant_not_routed",
                &ResolveError::OverrideDisabled.to_string(),
            )
        }
        Err(e @ (ResolveError::ApexHost { .. }
        | ResolveError::UnknownHost { .. }
        | ResolveError::BadSlug { .. })) => {
            return error_response(StatusCode::NOT_FOUND, "unknown_tenant", &e.to_string())
        }
    };

    let runtime = match router.registry.get_or_build(&tenant).await {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!(target: "backbone_tenant::router", tenant = %tenant.as_str(), error = %e, "tenant runtime build failed");
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "tenant_unavailable",
                "the tenant's database is unavailable or not yet provisioned",
            );
        }
    };

    // Hand the tenant's pool to the auth layers so their request fence scope opens on THIS
    // tenant's connection — the composition point between routing and the RLS fence.
    if let Some(pool) = runtime.database() {
        req.extensions_mut().insert(pool.clone());
    }
    req.extensions_mut().insert(runtime);

    next.run(req).await
}

//! Permission checks on routes: the one layer generic routes, generated transitions and
//! hand-written verbs share.
//!
//! A route requires a permission, written `<resource>:<action>`. [`guarded`] wraps a route so a
//! request without it is refused before the route's extractors or handler run:
//!
//! - no `AuthContext` on the request: `401`, violation `authentication_required`;
//! - an `AuthContext` that lacks the permission: `403`, violation `permission_required` with
//!   `params.permission` naming it ([`permission_refusal`]).
//!
//! The check is [`AuthContext::require`](backbone_auth::middleware::AuthContext::require), which
//! reads `AuthContext.permissions`. With no grant resolver installed that is whatever the host's own
//! authentication put there; installing `backbone_auth::resolve_grants` makes it the identity
//! module's data. Nothing is allowed by default in either case.
//!
//! A gate that has already admitted a request on a narrower rule (a self-service lane) attaches
//! [`PermissionWaiver`](backbone_auth::PermissionWaiver) to the request, and the layer lets it pass.
//!
//! ```rust,ignore
//! use axum::routing::post;
//! use backbone_core::http::guarded;
//!
//! // in `ticket_handler.ext.rs`: a custom verb, wrapped so `schema doctor` finds it guarded
//! let route = guarded(post(escalate), "ticket:escalate");
//! ```

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::MethodRouter,
    Json,
};
use backbone_auth::middleware::AuthContext;
use backbone_auth::{PermissionRequired, PermissionWaiver, PERMISSION_REQUIRED};

use crate::http::ApiResponse;
use crate::violation::Violation;

/// What one route needs: a permission, and the legacy spellings it also accepts for one release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needs {
    /// The canonical `<resource>:<action>`.
    pub permission: String,
    /// Legacy spellings that also grant it (the module-wide `<module>:<action>`).
    pub alternates: Vec<String>,
}

/// The `403` a missing permission answers with: the violation `permission_required`, and
/// `params.permission` naming what the route requires.
pub fn permission_refusal<T>(refused: &PermissionRequired) -> (StatusCode, Json<ApiResponse<T>>) {
    let message = refused.to_string();
    let violation = Violation::new("", PERMISSION_REQUIRED, message.clone())
        .with_param("permission", refused.permission.clone());
    (
        StatusCode::FORBIDDEN,
        Json(ApiResponse::rejected(message, vec![violation])),
    )
}

/// The `401` a request with no authenticated caller answers with.
pub fn unauthenticated_refusal<T>() -> (StatusCode, Json<ApiResponse<T>>) {
    let message = "Authentication required";
    let violation = Violation::new("", "authentication_required", message);
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiResponse::rejected(message, vec![violation])),
    )
}

/// Check `needs` against the request, answering the refusal or `None` to proceed.
fn refusal(req: &Request, needs: &[Needs]) -> Option<Response> {
    if req.extensions().get::<PermissionWaiver>().is_some() {
        return None;
    }
    let Some(ctx) = req.extensions().get::<AuthContext>() else {
        return Some(unauthenticated_refusal::<()>().into_response());
    };
    for need in needs {
        let alternates: Vec<&str> = need.alternates.iter().map(String::as_str).collect();
        if let Err(refused) = ctx.require_with(&need.permission, &alternates) {
            tracing::debug!(target: "backbone::auth", user = %ctx.user_id, permission = %refused.permission, "permission refused");
            return Some(permission_refusal::<()>(&refused).into_response());
        }
    }
    None
}

async fn enforce(State(needs): State<Arc<Vec<Needs>>>, req: Request, next: Next) -> Response {
    match refusal(&req, &needs) {
        Some(refused) => refused,
        None => next.run(req).await,
    }
}

/// Wrap `route` so a caller must hold `permission` (a canonical `<resource>:<action>`).
pub fn guarded<S>(route: MethodRouter<S>, permission: impl Into<String>) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    guarded_needs(
        route,
        vec![Needs {
            permission: permission.into(),
            alternates: Vec::new(),
        }],
    )
}

/// Wrap `route` so a caller must hold every one of `permissions` (a route that does two things, as
/// an upsert creates and updates).
pub fn guarded_all<S>(route: MethodRouter<S>, permissions: &[&str]) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    guarded_needs(
        route,
        permissions
            .iter()
            .map(|p| Needs {
                permission: (*p).to_string(),
                alternates: Vec::new(),
            })
            .collect(),
    )
}

/// [`guarded`] with each permission's accepted legacy spellings, which is what a generated route
/// builds from [`CrudPermissions`].
pub fn guarded_needs<S>(route: MethodRouter<S>, needs: Vec<Needs>) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    route.route_layer(from_fn_with_state(Arc::new(needs), enforce))
}

/// Which permission of an entity a generic route needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gate {
    /// List, get, count, aggregate, history, trash views.
    Read,
    /// Create, bulk create.
    Create,
    /// Put, patch, bulk update, bulk preview.
    Update,
    /// Soft delete, bulk soft delete.
    Delete,
    /// Restore, restore bulk, restore all.
    Restore,
    /// Hard delete from the trash, bulk hard delete, empty the trash.
    Purge,
    /// Upsert: creates or updates, so it needs both.
    CreateUpdate,
}

impl Gate {
    /// The action half of the permission this gate names by default.
    pub fn action(&self) -> &'static str {
        match self {
            Gate::Read => "read",
            Gate::Create => "create",
            Gate::Update => "update",
            Gate::Delete => "delete",
            Gate::Restore => "restore",
            Gate::Purge => "purge",
            Gate::CreateUpdate => "create",
        }
    }
}

/// The permissions of one entity's generic routes: `<entity>:read` for reads, and so on, with any a
/// schema's `requires:` renames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudPermissions {
    entity: String,
    module: Option<String>,
    overrides: Vec<(Gate, String)>,
    public: Vec<Gate>,
}

impl CrudPermissions {
    /// The permissions of `entity` (its snake_case name, `payroll_entry`).
    pub fn new(entity: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            module: None,
            overrides: Vec::new(),
            public: Vec::new(),
        }
    }

    /// Also accept the module-wide spelling (`<module>:<action>`, legacy) for the default
    /// permissions, so an existing grant on a whole module keeps working for one release.
    pub fn in_module(mut self, module: impl Into<String>) -> Self {
        self.module = Some(module.into());
        self
    }

    /// Require `permission` for `gate` instead of `<entity>:<action>`.
    pub fn requiring(mut self, gate: Gate, permission: impl Into<String>) -> Self {
        self.overrides.retain(|(g, _)| *g != gate);
        self.overrides.push((gate, permission.into()));
        self
    }

    /// Declare the routes of `gate` public: they require no permission at all, and no
    /// `AuthContext`, so an unauthenticated request is served where the service mounted the routes
    /// outside its authentication layer (see the generated `public_routes()`). The schema's
    /// `public:` declares it per entity; the tenant or company fence still applies, since it lives
    /// in the database connection and not in this layer.
    pub fn public(mut self, gate: Gate) -> Self {
        let gate = if gate == Gate::CreateUpdate {
            Gate::Create
        } else {
            gate
        };
        if !self.public.contains(&gate) {
            self.public.push(gate);
        }
        self
    }

    /// Whether the routes of `gate` are public. An upsert is public only when both the create and
    /// the update it performs are.
    pub fn is_public(&self, gate: Gate) -> bool {
        match gate {
            Gate::CreateUpdate => {
                self.public.contains(&Gate::Create) && self.public.contains(&Gate::Update)
            }
            g => self.public.contains(&g),
        }
    }

    /// The canonical permission `gate` requires.
    pub fn permission(&self, gate: Gate) -> String {
        let gate = if gate == Gate::CreateUpdate {
            Gate::Create
        } else {
            gate
        };
        self.overrides
            .iter()
            .find(|(g, _)| *g == gate)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| format!("{}:{}", self.entity, gate.action()))
    }

    fn one(&self, gate: Gate) -> Needs {
        let overridden = self.overrides.iter().any(|(g, _)| *g == gate);
        let alternates = match (&self.module, overridden) {
            (Some(module), false) => vec![format!("{module}:{}", gate.action())],
            _ => Vec::new(),
        };
        Needs {
            permission: self.permission(gate),
            alternates,
        }
    }

    /// What a route of `gate` needs: nothing for a public gate, and for an upsert only the half
    /// that is not public.
    pub fn needs(&self, gate: Gate) -> Vec<Needs> {
        match gate {
            Gate::CreateUpdate => [Gate::Create, Gate::Update]
                .into_iter()
                .filter(|g| !self.public.contains(g))
                .map(|g| self.one(g))
                .collect(),
            g if self.public.contains(&g) => Vec::new(),
            g => vec![self.one(g)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest, routing::get};
    use tower::ServiceExt;

    fn app(route: MethodRouter<()>) -> axum::Router {
        axum::Router::new().route("/", route)
    }

    async fn call(
        app: axum::Router,
        ctx: Option<AuthContext>,
        waiver: bool,
    ) -> (StatusCode, serde_json::Value) {
        let mut req = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();
        if let Some(c) = ctx {
            req.extensions_mut().insert(c);
        }
        if waiver {
            req.extensions_mut().insert(PermissionWaiver);
        }
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    fn holding(perms: &[&str]) -> AuthContext {
        AuthContext {
            user_id: "u".into(),
            roles: vec![],
            permissions: perms.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn a_holder_passes_and_a_caller_without_it_is_403_with_the_permission_named() {
        let route = || guarded(get(|| async { "ok" }), "ticket:read");
        let (status, _) = call(app(route()), Some(holding(&["ticket:read"])), false).await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = call(app(route()), Some(holding(&[])), false).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "no permission, no resolver: fail closed"
        );
        assert_eq!(body["violations"][0]["code"], "permission_required");
        assert_eq!(body["violations"][0]["params"]["permission"], "ticket:read");
        assert_eq!(body["success"], false);

        let (status, body) = call(app(route()), Some(holding(&["ticket:update"])), false).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["violations"][0]["params"]["permission"], "ticket:read");
    }

    #[tokio::test]
    async fn a_request_with_no_auth_context_is_401() {
        let (status, body) = call(
            app(guarded(get(|| async { "ok" }), "ticket:read")),
            None,
            false,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["violations"][0]["code"], "authentication_required");
    }

    #[tokio::test]
    async fn wildcards_grant_and_the_waiver_admits_a_request_a_narrower_rule_already_admitted() {
        let route = || guarded(get(|| async { "ok" }), "ticket:read");
        for held in ["ticket:*", "*:read", "*:*"] {
            let (status, _) = call(app(route()), Some(holding(&[held])), false).await;
            assert_eq!(status, StatusCode::OK, "{held}");
        }
        let (status, _) = call(app(route()), Some(holding(&[])), true).await;
        assert_eq!(status, StatusCode::OK, "waived");
        let (status, _) = call(app(route()), None, true).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a waiver stands for the whole check"
        );
    }

    #[tokio::test]
    async fn the_check_runs_before_the_handler() {
        async fn must_not_run() -> &'static str {
            panic!("the handler must not run for a refused caller")
        }
        let route = guarded(get(must_not_run), "ticket:read");
        let (status, _) = call(app(route), Some(holding(&[])), false).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn guarded_all_needs_every_permission() {
        let route = || guarded_all(get(|| async { "ok" }), &["ticket:create", "ticket:update"]);
        let (status, body) = call(app(route()), Some(holding(&["ticket:create"])), false).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            body["violations"][0]["params"]["permission"],
            "ticket:update"
        );
        let (status, _) = call(
            app(route()),
            Some(holding(&["ticket:create", "ticket:update"])),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[test]
    fn crud_permissions_default_to_entity_colon_action_and_honour_a_rename() {
        let p = CrudPermissions::new("payroll_entry").requiring(Gate::Read, "payroll:view");
        assert_eq!(p.permission(Gate::Read), "payroll:view");
        assert_eq!(p.permission(Gate::Purge), "payroll_entry:purge");
        assert_eq!(p.permission(Gate::CreateUpdate), "payroll_entry:create");
        assert_eq!(
            p.needs(Gate::CreateUpdate)
                .iter()
                .map(|n| n.permission.as_str())
                .collect::<Vec<_>>(),
            vec!["payroll_entry:create", "payroll_entry:update"]
        );
        let p = CrudPermissions::new("salary_slip").in_module("payroll");
        assert_eq!(
            p.needs(Gate::Read)[0].alternates,
            vec!["payroll:read".to_string()]
        );
        let p = p.requiring(Gate::Read, "payroll:view");
        assert!(
            p.needs(Gate::Read)[0].alternates.is_empty(),
            "a renamed permission has no legacy alternates"
        );
    }

    #[test]
    fn a_public_gate_needs_nothing_and_an_upsert_needs_only_its_non_public_half() {
        let p = CrudPermissions::new("label").public(Gate::Read);
        assert!(p.is_public(Gate::Read) && !p.is_public(Gate::Create));
        assert!(p.needs(Gate::Read).is_empty());
        assert_eq!(p.needs(Gate::Create)[0].permission, "label:create");

        let p = CrudPermissions::new("label").public(Gate::Create);
        assert!(!p.is_public(Gate::CreateUpdate), "an upsert also updates");
        assert_eq!(
            p.needs(Gate::CreateUpdate)
                .iter()
                .map(|n| n.permission.as_str())
                .collect::<Vec<_>>(),
            vec!["label:update"]
        );
        let p = p.public(Gate::Update);
        assert!(p.is_public(Gate::CreateUpdate));
        assert!(p.needs(Gate::CreateUpdate).is_empty());
    }
}

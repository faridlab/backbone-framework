//! The request layer that makes grants data (feature `axum`).
//!
//! [`resolve_grants`] reads who the caller is off the session context the host's guard inserted
//! (`OrgContext` from [`org_auth`](crate::org::org_auth), or `CompanyContext` from
//! [`company_auth`](crate::company::company_auth)), asks the installed [`GrantSource`] once, and
//! publishes the result as the request's [`AuthContext`] (and as [`Grants`], so a profile route can
//! publish the same list). From there every route's `ctx.require(..)` reads data the identity
//! module's tables hold, not whatever a token carried.
//!
//! Installing it is the opt-in. A service that does not install it keeps whatever `AuthContext` its
//! own authentication inserts, and routes check exactly that.
//!
//! # Wiring
//!
//! Mount it inside the session guard, so the context it reads exists and the request's connection
//! and tenant pool are bound:
//!
//! ```rust,ignore
//! use axum::middleware::from_fn_with_state;
//! use backbone_auth::{resolve_grants, GrantResolver, IdentityGrantSource};
//!
//! let resolver = GrantResolver::new(IdentityGrantSource::new(boot_pool.clone()));
//! let router = module_routes
//!     .route_layer(from_fn_with_state(resolver, resolve_grants))   // innermost: reads the session
//!     .route_layer(from_fn_with_state(org_verifier, org_auth))     // proves who, binds the scope
//!     .layer(from_fn_with_state(tenant_router, tenant_route));     // picks the tenant's pool
//! ```
//!
//! Fail-closed throughout: a source that cannot answer is `503`, a session naming no user of the
//! tenant gets an `AuthContext` that holds nothing, and a request with no session gets no
//! `AuthContext` at all, so routes answer `401`.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use uuid::Uuid;

use crate::grants::{GrantSource, Grants};
use crate::middleware::AuthContext;

/// The installed [`GrantSource`], the state of [`resolve_grants`].
#[derive(Clone)]
pub struct GrantResolver {
    source: Arc<dyn GrantSource>,
}

impl GrantResolver {
    /// Install `source`.
    pub fn new(source: impl GrantSource + 'static) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    /// Install a source already shared.
    pub fn from_arc(source: Arc<dyn GrantSource>) -> Self {
        Self { source }
    }
}

impl std::fmt::Debug for GrantResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrantResolver").finish_non_exhaustive()
    }
}

fn unavailable(message: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "success": false,
            "error": message,
            "violations": [{ "path": "", "code": "grants_unavailable", "message": message }],
        })),
    )
        .into_response()
}

/// Fill the request's `AuthContext` from the installed source, once per request. See the module
/// docs for where to mount it.
pub async fn resolve_grants(
    State(resolver): State<GrantResolver>,
    mut req: Request,
    next: Next,
) -> Response {
    // Once per request: a layer mounted twice, or a nested router, must not read the store again.
    if req.extensions().get::<Grants>().is_some() {
        return next.run(req).await;
    }
    let (user_id, unit) = if let Some(org) = req.extensions().get::<crate::org::OrgContext>() {
        (org.user_id.clone(), Some(org.acting_unit_id))
    } else if let Some(company) = req.extensions().get::<crate::company::CompanyContext>() {
        (company.user_id.clone(), Some(company.company_id))
    } else if let Some(ctx) = req.extensions().get::<AuthContext>() {
        (ctx.user_id.clone(), None)
    } else {
        // No session: nothing to resolve, and routes refuse a request with no AuthContext.
        return next.run(req).await;
    };
    let existing_roles = req
        .extensions()
        .get::<AuthContext>()
        .map(|c| c.roles.clone())
        .unwrap_or_default();

    let Ok(user) = Uuid::parse_str(&user_id) else {
        tracing::warn!(target: "backbone::auth", "the session does not name a user of this tenant; it holds no permissions");
        req.extensions_mut().insert(AuthContext {
            user_id,
            roles: existing_roles,
            permissions: Vec::new(),
        });
        req.extensions_mut().insert(Grants::default());
        return next.run(req).await;
    };
    match resolver
        .source
        .load_for_request(user, unit, req.extensions())
        .await
    {
        Ok(grants) => {
            let mut roles = existing_roles;
            for r in &grants.roles {
                if !roles.contains(r) {
                    roles.push(r.clone());
                }
            }
            req.extensions_mut().insert(AuthContext {
                user_id,
                roles,
                permissions: grants.effective(),
            });
            req.extensions_mut().insert(grants);
            next.run(req).await
        }
        Err(e) => {
            tracing::error!(target: "backbone::auth", error = %e, "grant lookup failed; the request is refused");
            unavailable("your permissions could not be verified")
        }
    }
}

#[cfg(feature = "identity-grants")]
pub use identity::IdentityGrantSource;

#[cfg(feature = "identity-grants")]
mod identity {
    use async_trait::async_trait;
    use backbone_orm::org_scope;
    use sqlx::{PgPool, Row};
    use uuid::Uuid;

    use crate::grants::{GrantError, GrantSource, Grants};

    /// Grants read from the identity module's tables, in one query, on the request's bound
    /// connection when there is one.
    ///
    /// A permission is held through any of four grants, all unioned:
    ///
    /// - a role assigned to the user (`public.user_roles`), through that role's permission rows;
    /// - the role on the user's membership of the acting org unit
    ///   (`sapiens.organization_users.role_id`), through the same rows;
    /// - a direct grant on the user (`public.user_permissions`);
    /// - a direct grant in the identity module (`sapiens.direct_permission_grants`).
    ///
    /// A grant that is revoked, expired, suspended or soft-deleted is not a grant, and a direct grant
    /// narrowed to one record (a `resource_id`, a scope or conditions) is not a grant over the whole
    /// collection either, so it is left out rather than widened.
    ///
    /// `ADMIN` and `SUPERUSER` carry authority by name (see [`Grants::effective`]); no permission
    /// rows are needed for them. [`trusting_roles`](Self::trusting_roles) names further roles a
    /// composition trusts with whole modules (`ACCOUNTANT` -> `accounting:*`).
    #[derive(Clone)]
    pub struct IdentityGrantSource {
        pool: PgPool,
        trusted: Vec<(String, Vec<String>)>,
    }

    impl IdentityGrantSource {
        /// A source over `pool`, the fallback when a request carries neither a bound connection
        /// nor a tenant pool of its own.
        pub fn new(pool: PgPool) -> Self {
            Self {
                pool,
                trusted: Vec::new(),
            }
        }

        /// Roles that hold a module whole: each `(role, modules)` adds `<module>:*` for a holder.
        pub fn trusting_roles(mut self, roles: Vec<(String, Vec<String>)>) -> Self {
            self.trusted = roles;
            self
        }

        async fn load_on(
            &self,
            pool: &PgPool,
            user: Uuid,
            unit: Option<Uuid>,
        ) -> Result<Grants, GrantError> {
            let rows = org_scope::fetch_all_rows_scoped(
                pool,
                sqlx::query(
                    r#"
                    WITH held_roles AS (
                        SELECT ur.role_id
                          FROM public.user_roles ur
                         WHERE ur.user_id = $1
                           AND (ur.metadata->>'deleted_at') IS NULL
                        UNION
                        SELECT ou.role_id
                          FROM sapiens.organization_users ou
                         WHERE ou.user_id = $1
                           AND ou.org_unit_id = $2
                           AND ou.role_id IS NOT NULL
                           AND ou.status = 'active'
                           AND (ou.metadata->>'deleted_at') IS NULL
                    )
                    SELECT r.name AS role, p.name AS permission
                      FROM held_roles h
                      JOIN public.roles r
                        ON r.id = h.role_id AND (r.metadata->>'deleted_at') IS NULL
                      LEFT JOIN public.role_permissions rp
                        ON rp.role_id = r.id AND (rp.metadata->>'deleted_at') IS NULL
                      LEFT JOIN public.permissions p
                        ON p.id = rp.permission_id AND (p.metadata->>'deleted_at') IS NULL
                    UNION ALL
                    SELECT NULL, p.name
                      FROM public.user_permissions up
                      JOIN public.permissions p
                        ON p.id = up.permission_id AND (p.metadata->>'deleted_at') IS NULL
                     WHERE up.user_id = $1
                       AND up.status = 'active'
                       AND up.revoked_at IS NULL
                       AND (up.expires_at IS NULL OR up.expires_at > now())
                       AND up.resource_id IS NULL
                       AND (up.metadata->>'deleted_at') IS NULL
                    UNION ALL
                    SELECT NULL, p.name
                      FROM sapiens.direct_permission_grants g
                      JOIN public.permissions p
                        ON p.id = g.permission_id AND (p.metadata->>'deleted_at') IS NULL
                     WHERE g.user_id = $1
                       AND g.status = 'active'
                       AND g.revoked_at IS NULL
                       AND (g.expires_at IS NULL OR g.expires_at > now())
                       AND (g.scope IS NULL OR g.scope = '{}'::jsonb)
                       AND (g.conditions IS NULL OR g.conditions = '{}'::jsonb)
                       AND (g.metadata->>'deleted_at') IS NULL
                    "#,
                )
                .bind(user)
                .bind(unit),
            )
            .await
            .map_err(|e| GrantError(e.to_string()))?;

            let mut grants = Grants::default();
            for row in rows {
                if let Some(role) = row
                    .try_get::<Option<String>, _>("role")
                    .map_err(|e| GrantError(e.to_string()))?
                {
                    grants.roles.insert(role);
                }
                if let Some(permission) = row
                    .try_get::<Option<String>, _>("permission")
                    .map_err(|e| GrantError(e.to_string()))?
                {
                    grants.permissions.insert(permission);
                }
            }
            for (role, modules) in &self.trusted {
                if grants.roles.contains(role) {
                    grants
                        .permissions
                        .extend(modules.iter().map(|m| format!("{m}:*")));
                }
            }
            Ok(grants)
        }
    }

    #[async_trait]
    impl GrantSource for IdentityGrantSource {
        async fn load(&self, user: Uuid, unit: Option<Uuid>) -> Result<Grants, GrantError> {
            self.load_on(&self.pool, user, unit).await
        }

        async fn load_for_request(
            &self,
            user: Uuid,
            unit: Option<Uuid>,
            extensions: &axum::http::Extensions,
        ) -> Result<Grants, GrantError> {
            // Under the tenant router the request carries the tenant's own pool; the boot pool
            // would answer from the wrong database for every other tenant.
            match extensions.get::<PgPool>() {
                Some(pool) => self.load_on(pool, user, unit).await,
                None => self.load_on(&self.pool, user, unit).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::{
        body::Body, http::Request as HttpRequest, middleware::from_fn_with_state, routing::get,
        Extension, Router,
    };
    use tower::ServiceExt;

    use crate::grants::GrantError;

    struct Fixed(Result<Grants, GrantError>);

    #[async_trait]
    impl GrantSource for Fixed {
        async fn load(&self, _user: Uuid, _unit: Option<Uuid>) -> Result<Grants, GrantError> {
            self.0.clone()
        }
    }

    async fn seen(ctx: Option<Extension<AuthContext>>) -> String {
        match ctx {
            Some(Extension(c)) => format!(
                "{}|{}|{}",
                c.user_id,
                c.roles.join(","),
                c.permissions.join(",")
            ),
            None => "none".into(),
        }
    }

    async fn run(source: Fixed, session: Option<AuthContext>) -> (StatusCode, String) {
        let router = Router::new()
            .route("/", get(seen))
            .route_layer(from_fn_with_state(
                GrantResolver::new(source),
                resolve_grants,
            ));
        let mut req = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();
        if let Some(s) = session {
            req.extensions_mut().insert(s);
        }
        let res = router.oneshot(req).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn the_resolved_grants_replace_what_the_session_carried() {
        let user = Uuid::new_v4();
        let session = AuthContext {
            user_id: user.to_string(),
            roles: vec!["jwt_role".into()],
            permissions: vec!["ticket:delete".into()],
        };
        let grants = Grants::new(["CLERK"], ["ticket:read"]);
        let (status, body) = run(Fixed(Ok(grants)), Some(session)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            format!("{user}|jwt_role,CLERK|ticket:read"),
            "the token's permissions are not the source"
        );
    }

    #[tokio::test]
    async fn an_authority_role_is_published_as_the_god_permission() {
        let user = Uuid::new_v4();
        let session = AuthContext::new(user.to_string());
        let (_, body) = run(
            Fixed(Ok(Grants::new(["ADMIN"], Vec::<String>::new()))),
            Some(session),
        )
        .await;
        assert!(body.ends_with("|*:*"), "{body}");
    }

    #[tokio::test]
    async fn a_source_that_cannot_answer_refuses_the_request() {
        let session = AuthContext::new(Uuid::new_v4().to_string());
        let (status, body) = run(Fixed(Err(GrantError("db down".into()))), Some(session)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("grants_unavailable"), "{body}");
        assert!(
            !body.contains("db down"),
            "the store's error is logged, not sent: {body}"
        );
    }

    #[tokio::test]
    async fn a_session_naming_no_uuid_holds_nothing() {
        let session = AuthContext {
            user_id: "not-a-uuid".into(),
            roles: vec![],
            permissions: vec!["*:*".into()],
        };
        let (status, body) = run(
            Fixed(Ok(Grants::new(["ADMIN"], Vec::<String>::new()))),
            Some(session),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body, "not-a-uuid||",
            "the carried god permission is dropped, and the source is not asked"
        );
    }

    #[tokio::test]
    async fn a_request_without_a_session_gets_no_context() {
        let (status, body) = run(
            Fixed(Ok(Grants::new(["ADMIN"], Vec::<String>::new()))),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "none");
    }
}

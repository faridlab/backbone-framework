//! Axum middleware for authorization
//!
//! Provides Axum middleware integration for authorization system.

use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::{debug, error};
use tower_layer::Layer;

use crate::service::*;
use crate::traits::*;
use crate::types::*;

/// Authorization state stored in request extensions
#[derive(Clone, Debug)]
pub struct AuthState {
    pub user_id: Option<String>,
}

/// Authorization middleware layer
#[derive(Clone)]
pub struct AuthorizationLayer {
    service: Arc<AuthorizationService>,
}

impl AuthorizationLayer {
    pub fn new(service: Arc<AuthorizationService>) -> Self {
        Self { service }
    }
}

impl<S> Layer<S> for AuthorizationLayer
where
    S: Clone + Send + 'static,
{
    type Service = AuthorizationMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthorizationMiddleware {
            inner,
            service: self.service.clone(),
        }
    }
}

/// Authorization middleware service
pub struct AuthorizationMiddleware<S> {
    inner: S,
    service: Arc<AuthorizationService>,
}

impl<S> Clone for AuthorizationMiddleware<S>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            service: self.service.clone(),
        }
    }
}

impl<S> tower::Service<Request> for AuthorizationMiddleware<S>
where
    S: tower::Service<Request> + Clone + Send + 'static,
    S::Response: IntoResponse,
    S::Error: Into<std::io::Error>,
    S::Future: Send,
{
    type Response = Response;
    type Error = std::io::Error;
    type Future = futures::future::BoxFuture<'static, Result<Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let service = self.service.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Get auth header
            let auth_header = req
                .headers()
                .get("authorization")
                .and_then(|h| h.to_str().ok())
                .and_then(|h| {
                    if h.starts_with("Bearer ") {
                        Some(h[7..].to_string())
                    } else {
                        None
                    }
                });

            // Skip authorization for public endpoints
            let path = req.uri().path();
            if is_public_endpoint(path) {
                debug!("Public endpoint '{}', skipping authorization", path);
                return inner
                    .call(req)
                    .await
                    .map(IntoResponse::into_response)
                    .map_err(Into::into);
            }

            // Get user_id from auth header
            let user_id = match auth_header {
                Some(uid) => uid,
                None => {
                    error!("No user_id found for protected endpoint '{}'", path);
                    return Ok(StatusCode::UNAUTHORIZED.into_response());
                }
            };

            // Create authorization request
            let auth_request = AuthorizationRequest {
                user: AuthUser {
                    user_id: user_id.clone(),
                    username: user_id.clone(),
                    roles: vec![],
                    permissions: vec![],
                    expires_at: None,
                },
                resource: to_resource_type(path),
                action: to_action(req.method()),
                resource_id: None,
            };

            // Check authorization
            match service.check_authorization(auth_request).await {
                Ok(response) if response.allowed => {
                    debug!("Authorization passed for user '{}': {:?}", user_id, response.checks);
                    inner
                        .call(req)
                        .await
                        .map(IntoResponse::into_response)
                        .map_err(Into::into)
                }
                Ok(_) => {
                    debug!("Authorization failed for user '{}'", user_id);
                    Ok(StatusCode::FORBIDDEN.into_response())
                }
                Err(e) => {
                    error!("Authorization error: {:?}", e);
                    Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
                }
            }
        })
    }
}

/// Convert HTTP method to action
fn to_action(method: &axum::http::Method) -> Action {
    match method {
        &axum::http::Method::GET => Action::Read,
        &axum::http::Method::POST => Action::Create,
        &axum::http::Method::PUT => Action::Update,
        &axum::http::Method::DELETE => Action::Delete,
        &axum::http::Method::PATCH => Action::Update,
        _ => Action::Read,
    }
}

/// Convert path to resource type
fn to_resource_type(path: &str) -> Resource {
    // Extract resource from path (e.g., /api/v1/users/123 -> users)
    let parts: Vec<&str> = path
        .trim_end_matches('/')
        .split('/')
        .collect();

    // Skip first empty part and api version
    let resource = parts.get(2).unwrap_or(&"unknown");
    match *resource {
        "users" => Resource::User,
        "roles" => Resource::Role,
        "permissions" => Resource::Permission,
        "settings" => Resource::Settings,
        _ => Resource::User,
    }
}

/// Check if path is a public endpoint (no authorization required)
fn is_public_endpoint(path: &str) -> bool {
    let public_paths = [
        "/health",
        "/api/v1/auth/login",
        "/api/v1/auth/register",
        "/api/v1/auth/forgot-password",
        "/docs",
        "/",
    ];

    public_paths
        .iter()
        .any(|p| if *p == "/" { path == "/" } else { path == *p || path.starts_with(p) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_public_endpoint() {
        assert!(is_public_endpoint("/health"));
        assert!(is_public_endpoint("/api/v1/auth/login"));
        assert!(is_public_endpoint("/api/v1/auth/register"));
        assert!(!is_public_endpoint("/api/v1/users"));
        assert!(!is_public_endpoint("/api/v1/roles"));
    }

    #[test]
    fn test_to_action() {
        assert!(matches!(to_action(&axum::http::Method::GET), Action::Read));
        assert!(matches!(to_action(&axum::http::Method::POST), Action::Create));
        assert!(matches!(to_action(&axum::http::Method::PUT), Action::Update));
        assert!(matches!(to_action(&axum::http::Method::DELETE), Action::Delete));
    }
}

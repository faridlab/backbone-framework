//! Axum-native health/readiness/liveness route handlers.
//!
//! Mounts the standard production probe endpoints onto an Axum router using
//! the shared `HealthChecker`. Replaces hand-rolled per-service health
//! routing with one consistent set of endpoints across the fleet.
//!
//! ## Endpoints
//!
//! - `GET /health` — overall status (always 200, body reflects state).
//! - `GET /health/detailed` — full component report (always 200).
//! - `GET /readyz` — Kubernetes readiness probe (200 healthy / 503 otherwise).
//! - `GET /livez` — Kubernetes liveness probe (always 200 if process running).
//!
//! ## Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use axum::Router;
//! use backbone_health::{HealthChecker, HealthConfig, routes::health_routes};
//!
//! # async fn build() -> Router {
//! let checker = Arc::new(HealthChecker::new(HealthConfig::default()));
//! Router::new().merge(health_routes(checker))
//! # }
//! ```

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};

use crate::{checker::HealthChecker, status::HealthStatus};

/// Build a router with the standard `/health`, `/health/detailed`, `/readyz`,
/// `/livez` endpoints, all backed by the supplied `HealthChecker`.
pub fn health_routes(checker: Arc<HealthChecker>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/detailed", get(health_detailed))
        .route("/readyz", get(readyz))
        .route("/livez", get(livez))
        .with_state(checker)
}

async fn health(State(checker): State<Arc<HealthChecker>>) -> impl IntoResponse {
    Json(checker.health_status().await)
}

async fn health_detailed(State(checker): State<Arc<HealthChecker>>) -> impl IntoResponse {
    Json(checker.health_report().await)
}

async fn readyz(State(checker): State<Arc<HealthChecker>>) -> impl IntoResponse {
    let report = checker.readiness().await;
    let code = match report.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded | HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };
    (code, Json(report))
}

async fn livez(State(checker): State<Arc<HealthChecker>>) -> impl IntoResponse {
    Json(checker.liveness(None).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{components::MockHealthCheck, HealthConfig};
    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;

    fn checker() -> Arc<HealthChecker> {
        Arc::new(HealthChecker::new(HealthConfig::default()))
    }

    #[tokio::test]
    async fn health_returns_200_with_status_body() {
        let app = health_routes(checker());
        let resp = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn detailed_returns_200_with_report_body() {
        let app = health_routes(checker());
        let resp = app
            .oneshot(
                Request::get("/health/detailed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_returns_200_when_healthy() {
        let app = health_routes(checker());
        let resp = app
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // No components registered → readiness is healthy by default.
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readyz_returns_503_when_component_unhealthy() {
        // Regression guard for the only behavioral branch in `readyz`:
        // a registered failing component must flip the HTTP status to 503
        // so Kubernetes (or any other readiness-probing orchestrator) takes
        // the pod out of rotation.
        let c = checker();
        c.add_component(
            "broken-db".to_string(),
            Box::new(MockHealthCheck::unhealthy("broken-db".to_string())),
        )
        .await
        .unwrap();
        // First check populates the cached statuses the readiness probe reads.
        c.run_single_check().await.ok();

        let app = health_routes(c);
        let resp = app
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn livez_returns_200() {
        let app = health_routes(checker());
        let resp = app
            .oneshot(Request::get("/livez").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

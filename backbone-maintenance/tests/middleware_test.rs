//! Integration tests for the maintenance-mode middleware.
//!
//! These exercise the public middleware contract (gate behavior, allow-path
//! prefix matching, 503 body shape, headers, admin token check) without
//! standing up any backing service.

use std::sync::{Arc, OnceLock};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Router,
};
use backbone_maintenance::{
    admin_toggle_handler, maintenance_middleware, status_handler, MaintenanceConfig,
    MaintenanceState, MaintenanceUpdate, MAINTENANCE_ADMIN_TOKEN_ENV,
};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tower::util::ServiceExt;

/// Serializes tests that mutate `MAINTENANCE_ADMIN_TOKEN` so parallel tokio
/// tests don't race on shared process env state. Using `tokio::sync::Mutex`
/// (not `std::sync::Mutex`) so the guard can be safely held across `await`
/// points without tripping `clippy::await_holding_lock`.
fn env_lock() -> &'static Arc<Mutex<()>> {
    static L: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    L.get_or_init(|| Arc::new(Mutex::new(())))
}

/// RAII guard that takes the env lock, mutates the `MAINTENANCE_ADMIN_TOKEN`
/// env var on entry, and unconditionally clears it on drop. The unconditional
/// cleanup is important: a panicking test would otherwise leak the env var
/// into the next test in the same binary.
struct AdminTokenGuard {
    _lock: OwnedMutexGuard<()>,
}

impl AdminTokenGuard {
    async fn set(value: &str) -> Self {
        let _lock = env_lock().clone().lock_owned().await;
        std::env::set_var(MAINTENANCE_ADMIN_TOKEN_ENV, value);
        Self { _lock }
    }

    async fn unset() -> Self {
        let _lock = env_lock().clone().lock_owned().await;
        std::env::remove_var(MAINTENANCE_ADMIN_TOKEN_ENV);
        Self { _lock }
    }
}

impl Drop for AdminTokenGuard {
    fn drop(&mut self) {
        std::env::remove_var(MAINTENANCE_ADMIN_TOKEN_ENV);
    }
}

fn cfg(enabled: bool) -> MaintenanceConfig {
    MaintenanceConfig {
        enabled,
        message: "down".to_string(),
        retry_after_seconds: 60,
        severity: "warning".to_string(),
        estimated_end_at: None,
        allow_paths: vec!["/health".to_string(), "/admin".to_string()],
    }
}

fn gated_app(state: Arc<MaintenanceState>) -> Router {
    Router::new()
        .route("/api/v1/anything", get(|| async { "ok" }))
        .route("/health", get(|| async { "healthy" }))
        .route("/health/detailed", get(|| async { "detailed" }))
        .route("/admin/users", get(|| async { "admin" }))
        .route_layer(axum::middleware::from_fn_with_state(
            state,
            maintenance_middleware,
        ))
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn passes_through_when_disabled() {
    let state = MaintenanceState::from_config(&cfg(false));
    let resp = gated_app(state)
        .oneshot(Request::get("/api/v1/anything").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn returns_503_with_full_contract_when_enabled() {
    let state = MaintenanceState::from_config(&cfg(true));
    let resp = gated_app(state)
        .oneshot(Request::get("/api/v1/anything").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.headers().get("retry-after").unwrap(), "60");
    assert_eq!(
        resp.headers().get("x-service-status").unwrap(),
        "maintenance"
    );

    let body = body_json(resp).await;
    assert_eq!(body["status"], "maintenance");
    assert_eq!(body["message"], "down");
    assert_eq!(body["retry_after"], 60);
    assert_eq!(body["severity"], "warning");
    assert!(body["started_at"].is_string());
    assert!(body["estimated_end_at"].is_null());
}

#[tokio::test]
async fn allow_paths_bypass_gate_with_prefix_match() {
    let state = MaintenanceState::from_config(&cfg(true));
    let app = gated_app(state);

    let resp = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(Request::get("/health/detailed").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .oneshot(Request::get("/admin/users").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn allow_path_does_not_match_unrelated_paths() {
    let state = MaintenanceState::from_config(&cfg(true));
    let resp = gated_app(state)
        .oneshot(Request::get("/api/v1/anything").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn status_endpoint_returns_current_state() {
    let state = MaintenanceState::from_config(&cfg(true));
    let app = Router::new()
        .route("/maintenance/status", get(status_handler))
        .with_state(state);

    let resp = app
        .oneshot(
            Request::get("/maintenance/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["message"], "down");
    assert_eq!(body["retry_after_seconds"], 60);
}

#[tokio::test]
async fn admin_toggle_returns_501_when_token_env_unset() {
    let _guard = AdminTokenGuard::unset().await;
    let state = MaintenanceState::from_config(&cfg(false));
    let app = Router::new()
        .route("/maintenance", post(admin_toggle_handler))
        .with_state(state);

    let req = Request::post("/maintenance")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"enabled":true}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn admin_toggle_rejects_wrong_token() {
    let _guard = AdminTokenGuard::set("secret-abc-123").await;
    let state = MaintenanceState::from_config(&cfg(false));
    let app = Router::new()
        .route("/maintenance", post(admin_toggle_handler))
        .with_state(state);

    let req = Request::post("/maintenance")
        .header("content-type", "application/json")
        .header("authorization", "Bearer wrong-token")
        .body(Body::from(r#"{"enabled":true}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_toggle_accepts_correct_token_and_applies_update() {
    let _guard = AdminTokenGuard::set("secret-xyz").await;
    let state = MaintenanceState::from_config(&cfg(false));
    let state_for_assert = state.clone();
    let app = Router::new()
        .route("/maintenance", post(admin_toggle_handler))
        .with_state(state);

    let req = Request::post("/maintenance")
        .header("content-type", "application/json")
        .header("authorization", "Bearer secret-xyz")
        .body(Body::from(r#"{"enabled":true,"message":"hello"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let snap = state_for_assert.snapshot();
    assert!(snap.enabled);
    assert_eq!(snap.message, "hello");
}

#[tokio::test]
async fn admin_toggle_accepts_x_maintenance_admin_token_header() {
    let _guard = AdminTokenGuard::set("alt-header-tok").await;
    let state = MaintenanceState::from_config(&cfg(false));
    let app = Router::new()
        .route("/maintenance", post(admin_toggle_handler))
        .with_state(state);

    let req = Request::post("/maintenance")
        .header("content-type", "application/json")
        .header("x-maintenance-admin-token", "alt-header-tok")
        .body(Body::from(r#"{"enabled":true}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn apply_update_mutates_fields_individually() {
    let state = MaintenanceState::from_config(&cfg(false));
    state.apply_update(MaintenanceUpdate {
        enabled: Some(true),
        message: Some("rolling out v2".to_string()),
        retry_after_seconds: Some(900),
        severity: Some("critical".to_string()),
        estimated_end_at: Some("2026-04-25T18:00:00Z".to_string()),
    });

    let s = state.snapshot();
    assert!(s.enabled);
    assert_eq!(s.message, "rolling out v2");
    assert_eq!(s.retry_after_seconds, 900);
    assert_eq!(s.severity, "critical");
    assert!(s.estimated_end_at.is_some());

    state.apply_update(MaintenanceUpdate {
        enabled: None,
        message: None,
        retry_after_seconds: None,
        severity: None,
        estimated_end_at: Some(String::new()),
    });
    assert!(state.snapshot().estimated_end_at.is_none());
}

//! Backbone Maintenance Mode — toggleable 503 gate.
//!
//! When enabled, every request is rejected with HTTP 503 unless the request
//! path begins with one of the configured `allow_paths`.
//!
//! ## Consumer-app contract
//!
//! On a gated request, the response is:
//! - Status: `503 Service Unavailable`
//! - Header: `Retry-After: <seconds>` (RFC 7231)
//! - Header: `X-Service-Status: maintenance` (machine-readable)
//! - Body (JSON):
//!   ```json
//!   {
//!     "status": "maintenance",
//!     "message": "...",
//!     "retry_after": 300,
//!     "severity": "warning",
//!     "started_at": "2026-04-25T12:00:00Z",
//!     "estimated_end_at": "2026-04-25T13:00:00Z"
//!   }
//!   ```
//!
//! Consumer apps should treat any 503 with `X-Service-Status: maintenance` as
//! a maintenance signal and surface a banner or block UI accordingly.
//! `GET /maintenance/status` is always reachable (200) for proactive polling.
//!
//! ## Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use axum::{Router, routing::{get, post}};
//! use backbone_maintenance::{
//!     MaintenanceConfig, MaintenanceState,
//!     maintenance_middleware, status_handler, admin_toggle_handler,
//! };
//!
//! # async fn build() -> Router {
//! let cfg = MaintenanceConfig::default();
//! let state = MaintenanceState::from_config(&cfg);
//!
//! // Routes for status polling and admin toggle (must remain reachable
//! // while the gate is on — list `/maintenance` in `allow_paths`).
//! let maintenance_router = Router::new()
//!     .route("/maintenance/status", get(status_handler))
//!     .route("/maintenance", post(admin_toggle_handler))
//!     .with_state(state.clone());
//!
//! // Apply the gate as the OUTERMOST middleware so it can short-circuit
//! // before any downstream layer pays its cost.
//! Router::new()
//!     .merge(maintenance_router)
//!     // .merge(your_app_routes)
//!     .layer(axum::middleware::from_fn_with_state(
//!         state,
//!         maintenance_middleware,
//!     ))
//! # }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Header set on every gated 503 response. Consumers can key off this header
/// to distinguish a maintenance 503 from any other source of 503.
pub const X_SERVICE_STATUS_HEADER: &str = "x-service-status";

/// Header value for the maintenance status header.
pub const X_SERVICE_STATUS_MAINTENANCE: &str = "maintenance";

/// Environment variable name for the admin toggle bearer token. When unset
/// or empty, `admin_toggle_handler` returns 501 Not Implemented (closed by
/// default).
pub const MAINTENANCE_ADMIN_TOKEN_ENV: &str = "MAINTENANCE_ADMIN_TOKEN";

/// Maintenance-mode configuration. Typically deserialized from the consumer
/// app's YAML config under a `maintenance:` key.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MaintenanceConfig {
    /// Whether the gate is active at boot. Can be toggled at runtime via
    /// the admin endpoint.
    pub enabled: bool,

    /// Human-readable message returned in the 503 body.
    pub message: String,

    /// Value sent in the `Retry-After` header and in the response body.
    pub retry_after_seconds: u64,

    /// `"info" | "warning" | "critical"` — surfaced to consumer apps for
    /// banner styling. Free-form; treat as advisory.
    pub severity: String,

    /// Optional ISO 8601 timestamp; consumers can render an ETA banner.
    #[serde(default)]
    pub estimated_end_at: Option<String>,

    /// Path prefixes that bypass the gate. Health probes, the maintenance
    /// admin/status endpoints themselves, and any login route MUST be
    /// included or the system can become unrecoverable.
    pub allow_paths: Vec<String>,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        MaintenanceConfig {
            enabled: false,
            message: "Service is undergoing scheduled maintenance. Please try again shortly."
                .to_string(),
            retry_after_seconds: 300,
            severity: "warning".to_string(),
            estimated_end_at: None,
            allow_paths: vec![
                "/health".to_string(),
                "/readyz".to_string(),
                "/livez".to_string(),
                "/metrics".to_string(),
                "/maintenance".to_string(),
            ],
        }
    }
}

/// Mutable details surfaced in the 503 body and `/maintenance/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceDetails {
    /// Human-readable message displayed to clients.
    pub message: String,
    /// `Retry-After` value (seconds) and field in the response body.
    pub retry_after_seconds: u64,
    /// Severity classification (e.g. "info", "warning", "critical").
    pub severity: String,
    /// When the gate was last toggled on (None when off).
    pub started_at: Option<DateTime<Utc>>,
    /// Optional ETA when maintenance is expected to end.
    pub estimated_end_at: Option<DateTime<Utc>>,
}

/// Shared maintenance state. Cloned cheaply via `Arc`. Mounted as Axum
/// router state and as middleware state.
pub struct MaintenanceState {
    enabled: AtomicBool,
    details: RwLock<MaintenanceDetails>,
    allow_paths: Vec<String>,
}

impl MaintenanceState {
    /// Build a fresh state from config. Stamps `started_at = now` if the
    /// gate is enabled at boot.
    pub fn from_config(cfg: &MaintenanceConfig) -> Arc<Self> {
        let estimated_end_at = cfg
            .estimated_end_at
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            });

        let started_at = if cfg.enabled { Some(Utc::now()) } else { None };

        Arc::new(Self {
            enabled: AtomicBool::new(cfg.enabled),
            details: RwLock::new(MaintenanceDetails {
                message: cfg.message.clone(),
                retry_after_seconds: cfg.retry_after_seconds,
                severity: cfg.severity.clone(),
                started_at,
                estimated_end_at,
            }),
            allow_paths: cfg.allow_paths.clone(),
        })
    }

    /// Cheap atomic read of the on/off flag.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Snapshot of the current state for serialization (status endpoint and
    /// 503 body construction).
    pub fn snapshot(&self) -> MaintenanceSnapshot {
        let d = self.details.read().expect("maintenance details lock poisoned");
        MaintenanceSnapshot {
            enabled: self.is_enabled(),
            message: d.message.clone(),
            retry_after_seconds: d.retry_after_seconds,
            severity: d.severity.clone(),
            started_at: d.started_at,
            estimated_end_at: d.estimated_end_at,
        }
    }

    /// Apply a partial update atomically. Toggling on stamps `started_at` if
    /// not already set; toggling off clears it.
    ///
    /// All field changes are applied under a single write lock so concurrent
    /// readers cannot observe a half-updated snapshot.
    pub fn apply_update(&self, update: MaintenanceUpdate) -> MaintenanceSnapshot {
        let mut d = self.details.write().expect("maintenance details lock poisoned");

        if let Some(enabled) = update.enabled {
            let was = self.enabled.swap(enabled, Ordering::Relaxed);
            match (enabled, was) {
                (true, false) => d.started_at = Some(Utc::now()),
                (false, true) => d.started_at = None,
                _ => {}
            }
        }
        if let Some(message) = update.message {
            d.message = message;
        }
        if let Some(retry) = update.retry_after_seconds {
            d.retry_after_seconds = retry;
        }
        if let Some(severity) = update.severity {
            d.severity = severity;
        }
        if let Some(end) = update.estimated_end_at {
            // empty string clears the value
            d.estimated_end_at = if end.is_empty() {
                None
            } else {
                DateTime::parse_from_rfc3339(&end)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            };
        }

        let snap = MaintenanceSnapshot {
            enabled: self.is_enabled(),
            message: d.message.clone(),
            retry_after_seconds: d.retry_after_seconds,
            severity: d.severity.clone(),
            started_at: d.started_at,
            estimated_end_at: d.estimated_end_at,
        };
        drop(d);
        snap
    }
}

/// Snapshot returned by `/maintenance/status` and used to render 503 bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceSnapshot {
    /// Current on/off state.
    pub enabled: bool,
    /// Current message.
    pub message: String,
    /// Current `Retry-After` value.
    pub retry_after_seconds: u64,
    /// Current severity classification.
    pub severity: String,
    /// When the gate was last toggled on.
    pub started_at: Option<DateTime<Utc>>,
    /// Optional ETA when maintenance is expected to end.
    pub estimated_end_at: Option<DateTime<Utc>>,
}

/// Partial update accepted by `POST /maintenance`. Any field omitted is left
/// unchanged. `estimated_end_at: ""` (empty string) clears the field.
#[derive(Debug, Default, Deserialize)]
pub struct MaintenanceUpdate {
    /// Toggle the gate on or off.
    pub enabled: Option<bool>,
    /// Replace the message.
    pub message: Option<String>,
    /// Replace the `Retry-After` value.
    pub retry_after_seconds: Option<u64>,
    /// Replace the severity classification.
    pub severity: Option<String>,
    /// Replace the ETA. Empty string clears.
    pub estimated_end_at: Option<String>,
}

/// Axum middleware: gate every request unless maintenance is off or the path
/// matches an `allow_paths` prefix.
pub async fn maintenance_middleware(
    State(state): State<Arc<MaintenanceState>>,
    request: Request,
    next: Next,
) -> Response {
    if !state.is_enabled() {
        return next.run(request).await;
    }

    let path = request.uri().path();
    if state.allow_paths.iter().any(|p| path.starts_with(p)) {
        return next.run(request).await;
    }

    let snap = state.snapshot();
    build_503(&snap)
}

fn build_503(snap: &MaintenanceSnapshot) -> Response {
    let body = Json(json!({
        "status": "maintenance",
        "message": snap.message,
        "retry_after": snap.retry_after_seconds,
        "severity": snap.severity,
        "started_at": snap.started_at,
        "estimated_end_at": snap.estimated_end_at,
    }));

    let mut response = (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&snap.retry_after_seconds.to_string()) {
        headers.insert(header::RETRY_AFTER, v);
    }
    headers.insert(
        X_SERVICE_STATUS_HEADER,
        HeaderValue::from_static(X_SERVICE_STATUS_MAINTENANCE),
    );
    response
}

/// `GET /maintenance/status` — always 200. Consumer apps poll this to detect
/// maintenance proactively without getting 503'd.
pub async fn status_handler(
    State(state): State<Arc<MaintenanceState>>,
) -> Json<MaintenanceSnapshot> {
    Json(state.snapshot())
}

/// `POST /maintenance` — auth-gated by the `MAINTENANCE_ADMIN_TOKEN` env var.
///
/// - If the env var is unset → 501 Not Implemented (closed by default).
/// - If `Authorization: Bearer <token>` (or `X-Maintenance-Admin-Token`) does
///   not match → 401.
/// - On match → applies the update and returns the new snapshot.
pub async fn admin_toggle_handler(
    State(state): State<Arc<MaintenanceState>>,
    headers: axum::http::HeaderMap,
    Json(update): Json<MaintenanceUpdate>,
) -> Response {
    let expected = match std::env::var(MAINTENANCE_ADMIN_TOKEN_ENV) {
        Ok(t) if !t.is_empty() => t,
        _ => {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({
                    "error": "maintenance_admin_disabled",
                    "message": format!(
                        "{} env var is not configured",
                        MAINTENANCE_ADMIN_TOKEN_ENV
                    ),
                })),
            )
                .into_response();
        }
    };

    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(str::to_string))
        .or_else(|| {
            headers
                .get("x-maintenance-admin-token")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        });

    match presented {
        Some(t) if constant_time_eq(t.as_bytes(), expected.as_bytes()) => {
            let snap = state.apply_update(update);
            tracing::warn!(
                enabled = snap.enabled,
                severity = %snap.severity,
                "Maintenance mode updated via admin endpoint"
            );
            (StatusCode::OK, Json(snap)).into_response()
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response(),
    }
}

/// Constant-time byte comparison. Avoids early-exit timing leaks when
/// comparing bearer tokens. Equivalent to the `subtle` crate but kept
/// inline so this crate has no extra dep.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_safe_allow_paths() {
        let cfg = MaintenanceConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.allow_paths.iter().any(|p| p == "/health"));
        assert!(cfg.allow_paths.iter().any(|p| p == "/maintenance"));
    }

    #[test]
    fn from_config_off_has_no_started_at() {
        let cfg = MaintenanceConfig::default();
        let state = MaintenanceState::from_config(&cfg);
        assert!(!state.is_enabled());
        assert!(state.snapshot().started_at.is_none());
    }

    #[test]
    fn from_config_on_stamps_started_at() {
        let cfg = MaintenanceConfig {
            enabled: true,
            ..MaintenanceConfig::default()
        };
        let state = MaintenanceState::from_config(&cfg);
        assert!(state.is_enabled());
        assert!(state.snapshot().started_at.is_some());
    }

    #[test]
    fn apply_update_toggle_on_stamps_started_at() {
        let cfg = MaintenanceConfig::default();
        let state = MaintenanceState::from_config(&cfg);
        let snap = state.apply_update(MaintenanceUpdate {
            enabled: Some(true),
            ..Default::default()
        });
        assert!(snap.enabled);
        assert!(snap.started_at.is_some());
    }

    #[test]
    fn apply_update_toggle_off_clears_started_at() {
        let cfg = MaintenanceConfig {
            enabled: true,
            ..MaintenanceConfig::default()
        };
        let state = MaintenanceState::from_config(&cfg);
        assert!(state.snapshot().started_at.is_some());

        let snap = state.apply_update(MaintenanceUpdate {
            enabled: Some(false),
            ..Default::default()
        });
        assert!(!snap.enabled);
        assert!(snap.started_at.is_none());
    }

    #[test]
    fn apply_update_estimated_end_at_empty_string_clears() {
        let cfg = MaintenanceConfig::default();
        let state = MaintenanceState::from_config(&cfg);
        state.apply_update(MaintenanceUpdate {
            estimated_end_at: Some("2026-04-25T13:00:00Z".to_string()),
            ..Default::default()
        });
        assert!(state.snapshot().estimated_end_at.is_some());

        state.apply_update(MaintenanceUpdate {
            estimated_end_at: Some(String::new()),
            ..Default::default()
        });
        assert!(state.snapshot().estimated_end_at.is_none());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }
}

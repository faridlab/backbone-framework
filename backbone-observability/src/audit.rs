//! Security audit logging middleware.
//!
//! Logs security-relevant events: data mutations (POST/PUT/PATCH/DELETE),
//! authentication failures (401), authorization denials (403), and rate-limit
//! rejections (429).
//!
//! ## Log shape
//!
//! Every audit log entry is a structured `tracing::info!` event with the
//! `audit = true` field, plus:
//! - `event` — `"auth_failure" | "access_denied" | "rate_limited" | "data_create" | "data_update" | "data_delete"`
//! - `method` — HTTP method as string
//! - `path` — Request path
//! - `status` — Response status code (u16)
//! - `ip` — Client IP (see `# Security` below for trust semantics)
//! - `user_agent` — Request User-Agent (or "unknown")
//!
//! Downstream log shippers can key off `audit=true` to route audit events to
//! a dedicated stream (security SIEM, separate index, etc.).
//!
//! # Security
//!
//! The `ip` field is forensic data — if it can be spoofed, the audit trail is
//! worthless. By default this middleware does **not** trust client-supplied
//! `X-Forwarded-For` / `X-Real-IP` headers and reports `ip="unknown"`.
//!
//! Set [`AuditConfig::trust_proxy_headers`] to `true` only when the service
//! sits behind a reverse proxy that **strips and re-sets** these headers
//! before forwarding (typical for ALB, nginx with `proxy_set_header
//! X-Forwarded-For $remote_addr`, Cloudflare with `CF-Connecting-IP`, etc.).
//! Enabling this in front of an untrusted network lets any client forge their
//! IP in the audit log.
//!
//! ## Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use axum::{Router, routing::get};
//! use backbone_observability::audit::{audit_middleware, AuditConfig};
//!
//! // Behind a trusted reverse proxy:
//! let cfg = Arc::new(AuditConfig { trust_proxy_headers: true });
//! let app: Router = Router::new()
//!     .route("/", get(|| async { "ok" }))
//!     .layer(axum::middleware::from_fn_with_state(cfg, audit_middleware));
//! ```

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};

/// Configuration for [`audit_middleware`].
#[derive(Debug, Clone, Default)]
pub struct AuditConfig {
    /// When `true`, consult `X-Forwarded-For` (first hop) then `X-Real-IP`
    /// for the client IP. **Only set this when running behind a reverse
    /// proxy that strips and re-sets these headers** — otherwise clients can
    /// spoof their IP in audit logs.
    ///
    /// When `false` (the default), the audit log records `ip="unknown"`.
    /// This is the safe default: no spoofable forensic data.
    pub trust_proxy_headers: bool,
}

/// Axum middleware: emits a structured audit log entry for every
/// security-relevant request/response pair.
///
/// Wire with `axum::middleware::from_fn_with_state(Arc::new(AuditConfig {...}), audit_middleware)`.
pub async fn audit_middleware(
    State(cfg): State<Arc<AuditConfig>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let ip = extract_client_ip(&req, cfg.trust_proxy_headers);
    let user_agent = extract_user_agent(&req);

    let response = next.run(req).await;
    let status = response.status();

    if should_audit(&method, status) {
        ::tracing::info!(
            audit = true,
            event = classify_event(&method, status),
            method = %method,
            path = %path,
            status = status.as_u16(),
            ip = %ip,
            user_agent = %user_agent,
            "Security audit event"
        );
    }

    response
}

/// Returns true if the (method, status) pair warrants an audit entry.
pub fn should_audit(method: &Method, status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED
        || status == StatusCode::FORBIDDEN
        || status == StatusCode::TOO_MANY_REQUESTS
        || is_mutation(method)
}

fn is_mutation(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Maps a (method, status) pair to a stable event classification string.
pub fn classify_event(method: &Method, status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED => "auth_failure",
        StatusCode::FORBIDDEN => "access_denied",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        _ => match *method {
            Method::POST => "data_create",
            Method::PUT | Method::PATCH => "data_update",
            Method::DELETE => "data_delete",
            _ => "unknown",
        },
    }
}

/// Best-effort client IP extraction.
///
/// When `trust_proxy_headers` is `false` (the safe default), returns
/// `"unknown"` regardless of headers — preventing IP spoofing in the audit
/// trail.
///
/// When `true`, consults `X-Forwarded-For` (first non-empty hop) then
/// `X-Real-IP`. Empty/whitespace header values fall through correctly.
pub fn extract_client_ip(req: &Request, trust_proxy_headers: bool) -> String {
    if !trust_proxy_headers {
        return "unknown".to_string();
    }

    let first_xff_hop = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    first_xff_hop
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract the User-Agent header, defaulting to `"unknown"`.
pub fn extract_user_agent(req: &Request) -> String {
    req.headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn req_with(headers: &[(&str, &str)]) -> Request {
        let mut builder = Request::builder().uri("/");
        for (k, v) in headers {
            builder = builder.header(*k, HeaderValue::from_str(v).unwrap());
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn audits_auth_failure() {
        assert!(should_audit(&Method::GET, StatusCode::UNAUTHORIZED));
        assert!(should_audit(&Method::GET, StatusCode::FORBIDDEN));
    }

    #[test]
    fn audits_rate_limit() {
        assert!(should_audit(&Method::GET, StatusCode::TOO_MANY_REQUESTS));
    }

    #[test]
    fn audits_mutations_regardless_of_status() {
        assert!(should_audit(&Method::POST, StatusCode::OK));
        assert!(should_audit(&Method::PUT, StatusCode::CREATED));
        assert!(should_audit(&Method::PATCH, StatusCode::OK));
        assert!(should_audit(&Method::DELETE, StatusCode::NO_CONTENT));
    }

    #[test]
    fn skips_safe_reads_with_2xx() {
        assert!(!should_audit(&Method::GET, StatusCode::OK));
        assert!(!should_audit(&Method::HEAD, StatusCode::OK));
    }

    #[test]
    fn classify_event_status_takes_precedence_over_method() {
        assert_eq!(
            classify_event(&Method::POST, StatusCode::UNAUTHORIZED),
            "auth_failure"
        );
        assert_eq!(
            classify_event(&Method::POST, StatusCode::FORBIDDEN),
            "access_denied"
        );
        assert_eq!(
            classify_event(&Method::POST, StatusCode::TOO_MANY_REQUESTS),
            "rate_limited"
        );
    }

    #[test]
    fn classify_event_maps_methods_to_crud_verbs() {
        assert_eq!(classify_event(&Method::POST, StatusCode::OK), "data_create");
        assert_eq!(classify_event(&Method::PUT, StatusCode::OK), "data_update");
        assert_eq!(classify_event(&Method::PATCH, StatusCode::OK), "data_update");
        assert_eq!(
            classify_event(&Method::DELETE, StatusCode::OK),
            "data_delete"
        );
    }

    #[test]
    fn ip_returns_unknown_when_proxy_not_trusted() {
        let req = req_with(&[("x-forwarded-for", "1.2.3.4"), ("x-real-ip", "5.6.7.8")]);
        assert_eq!(extract_client_ip(&req, false), "unknown");
    }

    #[test]
    fn ip_uses_first_xff_hop_when_trusted() {
        let req = req_with(&[("x-forwarded-for", "1.2.3.4, 10.0.0.1, 10.0.0.2")]);
        assert_eq!(extract_client_ip(&req, true), "1.2.3.4");
    }

    #[test]
    fn ip_falls_through_to_real_ip_when_xff_missing() {
        let req = req_with(&[("x-real-ip", "9.9.9.9")]);
        assert_eq!(extract_client_ip(&req, true), "9.9.9.9");
    }

    #[test]
    fn ip_falls_through_to_real_ip_when_xff_blank() {
        // Empty XFF (or whitespace-only first hop) must not poison the result.
        let req = req_with(&[("x-forwarded-for", " "), ("x-real-ip", "9.9.9.9")]);
        assert_eq!(extract_client_ip(&req, true), "9.9.9.9");
    }

    #[test]
    fn ip_unknown_when_no_headers_present() {
        let req = req_with(&[]);
        assert_eq!(extract_client_ip(&req, true), "unknown");
    }
}

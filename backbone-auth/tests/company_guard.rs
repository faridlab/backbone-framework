//! Contract tests for the HTTP company guard (feature `axum`).
//!
//! The guard's whole job is to be **fail-closed**: a request that cannot prove which tenant it acts for
//! must never reach a handler. These drive the real router in-process via `tower::ServiceExt::oneshot`;
//! no database is involved, because the guard's contract is about the token, not about storage.
//!
//! Mirrors the `TG-*` cases that `backbone-pos` proved before this pattern was promoted here:
//!
//! TG-1  unauthenticated request              → 401
//! TG-2  token with no `company_id` claim      → 401  (authenticated, but no tenant → still rejected)
//! TG-3  expired token                         → 401
//! TG-4  token signed with the wrong secret    → 401
//! TG-5  valid token                           → 200, and the handler sees the claim's `company_id`
//! TG-6  `branch_id` is optional               → 200 without it

#![cfg(feature = "axum")]

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    middleware::from_fn_with_state,
    routing::post,
    Router,
};
use backbone_auth::company::{company_auth, CompanyContext, CompanyVerifier};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &[u8] = b"tenant-guard-framework-test-secret";
const WRONG_SECRET: &[u8] = b"not-the-signing-secret";

/// A far-future expiry, so a passing test never depends on wall-clock drift.
const NOT_EXPIRED: usize = 9_999_999_999;
/// 2001-09-09 — comfortably in the past.
const EXPIRED: usize = 1_000_000_000;

#[derive(Serialize)]
struct TestClaims {
    sub: String,
    exp: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    company_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch_id: Option<Uuid>,
}

fn token_with(secret: &[u8], exp: usize, company_id: Option<Uuid>, branch_id: Option<Uuid>) -> String {
    let claims = TestClaims { sub: "user-1".into(), exp, company_id, branch_id };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret)).unwrap()
}

/// Echoes the tenant the guard proved, so a 200 also asserts the extractor wired the right values.
async fn guarded_handler(tenant: CompanyContext) -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("x-company-id", tenant.company_id.to_string())
        .header(
            "x-branch-id",
            tenant.branch_id.map(|b| b.to_string()).unwrap_or_else(|| "none".into()),
        )
        .header("x-user-id", tenant.user_id)
        .body(Body::empty())
        .unwrap()
}

fn app() -> Router {
    Router::new()
        .route("/guarded", post(guarded_handler))
        .layer(from_fn_with_state(CompanyVerifier::hs256(SECRET), company_auth))
}

async fn call(bearer: Option<&str>) -> axum::response::Response {
    let mut req = Request::builder().method("POST").uri("/guarded");
    if let Some(t) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    app().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn tg1_unauthenticated_request_is_rejected() {
    assert_eq!(call(None).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tg2_token_without_company_id_is_rejected() {
    // The security crux: this token is perfectly valid and names a real user. It still must not pass,
    // because a writer that cannot name its tenant is how cross-tenant writes happen.
    let t = token_with(SECRET, NOT_EXPIRED, None, None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tg3_expired_token_is_rejected() {
    let t = token_with(SECRET, EXPIRED, Some(Uuid::new_v4()), None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tg4_token_signed_with_wrong_secret_is_rejected() {
    let t = token_with(WRONG_SECRET, NOT_EXPIRED, Some(Uuid::new_v4()), None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tg5_valid_token_passes_and_handler_sees_the_signed_tenant() {
    let company = Uuid::new_v4();
    let branch = Uuid::new_v4();
    let t = token_with(SECRET, NOT_EXPIRED, Some(company), Some(branch));

    let res = call(Some(&t)).await;

    assert_eq!(res.status(), StatusCode::OK);
    let h = res.headers();
    assert_eq!(h["x-company-id"], company.to_string());
    assert_eq!(h["x-branch-id"], branch.to_string());
    assert_eq!(h["x-user-id"], "user-1");
}

#[tokio::test]
async fn tg6_branch_id_is_optional() {
    let company = Uuid::new_v4();
    let t = token_with(SECRET, NOT_EXPIRED, Some(company), None);

    let res = call(Some(&t)).await;

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-company-id"], company.to_string());
    assert_eq!(res.headers()["x-branch-id"], "none");
}

#[tokio::test]
async fn a_bare_token_without_the_bearer_prefix_is_rejected() {
    let t = token_with(SECRET, NOT_EXPIRED, Some(Uuid::new_v4()), None);
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/guarded")
                .header(header::AUTHORIZATION, t) // no "Bearer " prefix
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

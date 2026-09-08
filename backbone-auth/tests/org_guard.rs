//! Contract tests for the org-tree session guard and its issuer (feature `axum`, ADR-0028).
//!
//! Two groups, mirroring `tests/company_guard.rs`:
//!
//! - The **issuer** proves its round-trip without any HTTP: what `OrgIssuer` mints,
//!   `OrgVerifier` verifies — and what it must refuse (a refresh token, a token with no acting
//!   unit is never minted because the type requires one).
//! - The **guard** proves its fail-closed contract in-process via `tower::ServiceExt::oneshot`:
//!   every request that cannot prove an org session is 401 before any database is touched.
//!   (The 200 path resolves the scope over a tenant pool — that lives in the DSN-gated live
//!   proof, not here; without a pool on the request the guard answers 500, which this suite
//!   pins as the wiring-error signal.)

#![cfg(feature = "axum")]

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    middleware::from_fn_with_state,
    routing::post,
    Router,
};
use backbone_auth::org::{org_auth, OrgIssuer, OrgVerifier};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &[u8] = b"org-guard-framework-test-secret";
const WRONG_SECRET: &[u8] = b"not-the-signing-secret";

/// A far-future expiry, so a passing test never depends on wall-clock drift.
const NOT_EXPIRED: usize = 9_999_999_999;
/// 2001-09-09 — comfortably in the past.
const EXPIRED: usize = 1_000_000_000;

// ── Issuer round-trip ─────────────────────────────────────────────────────────

#[test]
fn issuer_access_token_round_trips_through_the_verifier() {
    let issuer = OrgIssuer::hs256(SECRET);
    let verifier = OrgVerifier::hs256(SECRET);
    let user = Uuid::new_v4();
    let acting = Uuid::new_v4();
    let sister = Uuid::new_v4();

    let token = issuer
        .issue_access(&user.to_string(), acting, &[sister], None, Duration::from_secs(3600))
        .unwrap();
    let ctx = verifier.verify(&token).expect("minted access token must verify");

    assert_eq!(ctx.acting_unit_id, acting);
    assert_eq!(ctx.entitled_units, vec![sister]);
    assert_eq!(ctx.user_id, user.to_string());
    // An org-only mint carries no legacy twin — the field stays absent from the wire, so the
    // token decodes exactly like a pre-twin mint.
    assert_eq!(ctx.legacy_company_id, None);
}

#[test]
fn issuer_refresh_token_is_refused_by_the_verifier() {
    let issuer = OrgIssuer::hs256(SECRET);
    let verifier = OrgVerifier::hs256(SECRET);

    let refresh = issuer
        .issue_refresh("user-1", Uuid::new_v4(), &[], None, Duration::from_secs(7 * 24 * 3600))
        .unwrap();
    // Same signature, same claims shape — the `typ` claim is the only difference, and it is
    // load-bearing: a rotation credential must never open a scoped session.
    assert!(verifier.verify(&refresh).is_none());
}

/// The tenancy-transition contract: one minted credential passes BOTH guards. The re-key
/// sweep flips surface by surface, so the org guard and the company guard must accept the
/// same token while it runs — and the refresh twin must pass neither.
#[test]
fn a_twin_claim_mint_passes_both_guards() {
    use backbone_auth::company::CompanyVerifier;

    let issuer = OrgIssuer::hs256(SECRET);
    let org_verifier = OrgVerifier::hs256(SECRET);
    let company_verifier = CompanyVerifier::hs256(SECRET);
    let user = Uuid::new_v4();
    let company = Uuid::new_v4();

    let access = issuer
        .issue_access(&user.to_string(), company, &[], Some(company), Duration::from_secs(3600))
        .unwrap();
    let org_ctx = org_verifier
        .verify(&access)
        .expect("the org guard accepts the twin-claim token");
    assert_eq!(org_ctx.acting_unit_id, company);
    assert_eq!(org_ctx.legacy_company_id, Some(company));
    let company_ctx = company_verifier
        .verify(&access)
        .expect("the company guard accepts the same token during the transition");
    assert_eq!(company_ctx.company_id, company);
    assert_eq!(company_ctx.user_id, user.to_string());

    let refresh = issuer
        .issue_refresh(&user.to_string(), company, &[], Some(company), Duration::from_secs(7 * 24 * 3600))
        .unwrap();
    assert!(org_verifier.verify(&refresh).is_none());
    assert!(company_verifier.verify(&refresh).is_none());
}

/// An org-only mint (no twin) must NOT open a company-guarded surface: the transition
/// contract is one token → both guards, never org-only → company access.
#[test]
fn an_org_only_mint_is_refused_by_the_company_guard() {
    use backbone_auth::company::CompanyVerifier;

    let issuer = OrgIssuer::hs256(SECRET);
    let company_verifier = CompanyVerifier::hs256(SECRET);

    let org_only = issuer
        .issue_access("user-1", Uuid::new_v4(), &[], None, Duration::from_secs(3600))
        .unwrap();
    assert!(company_verifier.verify(&org_only).is_none());
}

#[test]
fn a_token_signed_by_a_different_secret_is_refused() {
    let issuer = OrgIssuer::hs256(WRONG_SECRET);
    let verifier = OrgVerifier::hs256(SECRET);

    let token = issuer
        .issue_access("user-1", Uuid::new_v4(), &[], None, Duration::from_secs(3600))
        .unwrap();
    assert!(verifier.verify(&token).is_none());
}

// ── Guard contract (fail-closed, no database) ─────────────────────────────────

#[derive(Serialize)]
struct TestClaims {
    sub: String,
    exp: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_unit_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    typ: Option<String>,
}

fn token_with(
    secret: &[u8],
    exp: usize,
    org_unit_id: Option<Uuid>,
    typ: Option<String>,
) -> String {
    let claims = TestClaims { sub: "user-1".into(), exp, org_unit_id, typ };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret)).unwrap()
}

fn app() -> Router {
    Router::new()
        .route("/guarded", post(|| async { StatusCode::OK }))
        .layer(from_fn_with_state(OrgVerifier::hs256(SECRET), org_auth))
}

async fn call(bearer: Option<&str>) -> axum::response::Response {
    let mut req = Request::builder().method("POST").uri("/guarded");
    if let Some(t) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    app().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn og1_unauthenticated_request_is_rejected() {
    assert_eq!(call(None).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn og2_token_without_org_unit_id_is_rejected() {
    // The security crux, same as the company twin: a valid token naming a real user still must
    // not pass, because a writer that cannot name its org node is how cross-node writes happen.
    let t = token_with(SECRET, NOT_EXPIRED, None, None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn og3_expired_token_is_rejected() {
    let t = token_with(SECRET, EXPIRED, Some(Uuid::new_v4()), None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn og4_token_signed_with_wrong_secret_is_rejected() {
    let t = token_with(WRONG_SECRET, NOT_EXPIRED, Some(Uuid::new_v4()), None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn og5_refresh_typed_token_is_rejected() {
    // Minted by the real issuer: exactly the credential a refresh endpoint hands out. The
    // signature is valid and the acting unit is present — the guard still refuses it.
    let refresh = OrgIssuer::hs256(SECRET)
        .issue_refresh("user-1", Uuid::new_v4(), &[], None, Duration::from_secs(7 * 24 * 3600))
        .unwrap();
    assert_eq!(call(Some(&refresh)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn og6_unknown_typed_token_is_rejected() {
    // Fail-closed on unrecognized purposes: an issuer minting a new token type must not
    // silently widen what this guard accepts.
    let t = token_with(SECRET, NOT_EXPIRED, Some(Uuid::new_v4()), Some("godmode".into()));
    assert_eq!(call(Some(&t)).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn og7_typless_legacy_token_reaches_the_pool_check() {
    // Tokens minted before `typ` existed carry no claim and stay accepted by the verifier —
    // so the request proceeds past token validation to the wiring check (no pool inserted →
    // 500, the loud wiring-error signal, never a 200).
    let t = token_with(SECRET, NOT_EXPIRED, Some(Uuid::new_v4()), None);
    assert_eq!(call(Some(&t)).await.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn og8_missing_tenant_pool_is_a_wiring_error_not_a_pass() {
    // A valid access token with a pool-less request proves the router did not run outside the
    // guard. That is a 500 wiring error — the guard must never let the request through.
    let issuer = OrgIssuer::hs256(SECRET);
    let access = issuer
        .issue_access("user-1", Uuid::new_v4(), &[], None, Duration::from_secs(3600))
        .unwrap();
    assert_eq!(call(Some(&access)).await.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

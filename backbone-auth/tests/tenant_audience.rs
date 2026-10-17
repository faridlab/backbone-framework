//! Tenant binding of session tokens (feature `axum`).
//!
//! Every tenant of a database-per-tenant deployment verifies with the same signing key, and
//! tenant databases seeded from the same fixtures share org-node ids. A token's signature and its
//! acting unit therefore cannot tell tenants apart: the token has to say which tenant it was
//! minted for. The issuer writes the tenant into `aud`; a verifier bound to the request's tenant
//! refuses a token minted for another one before any database is touched.
//!
//! The request's tenant reaches the guard as a request extension (the tenant router inserts it);
//! these tests insert a local stand-in through a layer, since the binding reads it through a
//! caller-supplied function and never names the router's type.

#![cfg(feature = "axum")]

use axum::{
    body::Body,
    http::{header, Extensions, Request, StatusCode},
    middleware::from_fn_with_state,
    routing::post,
    Extension, Router,
};
use backbone_auth::company::{company_auth, CompanyVerifier};
use backbone_auth::org::{org_auth, OrgIssuer, OrgVerifier};
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &[u8] = b"tenant-audience-framework-test-secret";
const HOUR: Duration = Duration::from_secs(3600);

/// The test's stand-in for the tenant router's resolved tenant.
#[derive(Clone)]
struct RequestTenant(&'static str);

fn tenant_of(ext: &Extensions) -> Option<String> {
    ext.get::<RequestTenant>().map(|t| t.0.to_string())
}

/// Attach the request's tenant the way the tenant router does — as a request extension —
/// outside the guard, or leave it off for a surface that resolves no tenant.
fn with_tenant(router: Router, tenant: Option<&'static str>) -> Router {
    match tenant {
        Some(t) => router.layer(Extension(RequestTenant(t))),
        None => router,
    }
}

fn org_app(verifier: OrgVerifier, tenant: Option<&'static str>) -> Router {
    let router = Router::new()
        .route("/guarded", post(|| async { StatusCode::OK }))
        .layer(from_fn_with_state(verifier, org_auth));
    with_tenant(router, tenant)
}

fn company_app(verifier: CompanyVerifier, tenant: Option<&'static str>) -> Router {
    let router = Router::new()
        .route("/guarded", post(|| async { StatusCode::OK }))
        .layer(from_fn_with_state(verifier, company_auth));
    with_tenant(router, tenant)
}

async fn call(app: Router, token: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri("/guarded")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

fn access_for(tenant: Option<&str>, unit: Uuid) -> String {
    let issuer = OrgIssuer::hs256(SECRET);
    let issuer = match tenant {
        Some(t) => issuer.for_audience(t),
        None => issuer,
    };
    issuer
        .issue_access("user-1", unit, &[], Some(unit), HOUR)
        .unwrap()
}

fn bound_org() -> OrgVerifier {
    OrgVerifier::hs256(SECRET).bind_audience(tenant_of)
}

fn bound_company() -> CompanyVerifier {
    CompanyVerifier::hs256(SECRET).bind_audience(tenant_of)
}

// ── Issuer + verifier, no HTTP ────────────────────────────────────────────────

#[test]
fn a_token_minted_for_a_tenant_verifies_only_for_that_tenant() {
    let unit = Uuid::new_v4();
    let token = access_for(Some("acme"), unit);
    let verifier = OrgVerifier::hs256(SECRET);

    let ctx = verifier
        .verify_for_audience(&token, Some("acme"))
        .expect("the minting tenant accepts its own token");
    assert_eq!(ctx.acting_unit_id, unit);
    assert!(verifier
        .verify_for_audience(&token, Some("globex"))
        .is_none());
    // A surface with no tenant (the boot database) is not the token's tenant either.
    assert!(verifier.verify_for_audience(&token, None).is_none());
}

#[test]
fn an_unbound_verifier_still_accepts_a_tenant_bound_token() {
    // Binding is the composer's opt-in: a verifier that was never told the request's tenant
    // keeps accepting tokens, with or without `aud`, exactly as before.
    let token = access_for(Some("acme"), Uuid::new_v4());
    assert!(OrgVerifier::hs256(SECRET).verify(&token).is_some());
    assert!(CompanyVerifier::hs256(SECRET).verify(&token).is_some());
}

// ── Org guard ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn org_guard_refuses_a_token_minted_for_another_tenant() {
    // The cross-tenant case: same key, same seeded node id, different tenant.
    let token = access_for(Some("globex"), Uuid::new_v4());
    assert_eq!(
        call(org_app(bound_org(), Some("acme")), &token).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn org_guard_lets_the_minting_tenant_through_to_the_scope() {
    // Past the binding the guard reaches its pool check: no pool here, so the wiring 500.
    let token = access_for(Some("acme"), Uuid::new_v4());
    assert_eq!(
        call(org_app(bound_org(), Some("acme")), &token).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn org_guard_refuses_an_unbound_token_on_a_tenant_surface() {
    let token = access_for(None, Uuid::new_v4());
    assert_eq!(
        call(org_app(bound_org(), Some("acme")), &token).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn org_guard_accepts_an_unbound_token_only_behind_the_compatibility_flag() {
    let token = access_for(None, Uuid::new_v4());
    #[allow(deprecated)]
    let verifier = bound_org().accept_unbound_tokens(true);
    assert_eq!(
        call(org_app(verifier, Some("acme")), &token).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn org_guard_refuses_a_tenant_token_where_no_tenant_was_resolved() {
    let token = access_for(Some("acme"), Uuid::new_v4());
    assert_eq!(
        call(org_app(bound_org(), None), &token).await,
        StatusCode::UNAUTHORIZED
    );
}

// ── Company guard ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn company_guard_refuses_a_token_minted_for_another_tenant() {
    let token = access_for(Some("globex"), Uuid::new_v4());
    assert_eq!(
        call(company_app(bound_company(), Some("acme")), &token).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn company_guard_accepts_the_minting_tenant() {
    let token = access_for(Some("acme"), Uuid::new_v4());
    assert_eq!(
        call(company_app(bound_company(), Some("acme")), &token).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn company_guard_on_a_surface_without_a_tenant_accepts_only_unbound_tokens() {
    // The boot database's surfaces resolve no tenant: its own unbound tokens pass, a token
    // bound to a tenant does not.
    let boot = access_for(None, Uuid::new_v4());
    let tenant = access_for(Some("acme"), Uuid::new_v4());
    assert_eq!(
        call(company_app(bound_company(), None), &boot).await,
        StatusCode::OK
    );
    assert_eq!(
        call(company_app(bound_company(), None), &tenant).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn company_guard_refuses_an_unbound_token_on_a_tenant_surface() {
    let token = access_for(None, Uuid::new_v4());
    assert_eq!(
        call(company_app(bound_company(), Some("acme")), &token).await,
        StatusCode::UNAUTHORIZED
    );
}

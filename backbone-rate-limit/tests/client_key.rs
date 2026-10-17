//! Who a request is counted against, and what happens when the counter store fails.
//!
//! A limiter keyed on something the client chooses limits nothing: before, a client named its
//! own bucket through an `x-rate-limit-key` header, and a client that sent none shared one
//! global bucket with everyone else. The key is now the connection's peer address; forwarding
//! headers count only when the peer is a configured trusted proxy; an authenticated route can
//! key on its own principal instead.

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::from_fn_with_state,
    routing::get,
    Router,
};
use backbone_rate_limit::{
    rate_limit_middleware, FailMode, IncrementOutcome, RateLimitConfig, RateLimitError,
    RateLimitMiddleware, RateLimitResult, StorageBackend, TrustedProxies,
};
use std::sync::Arc;
use tower::ServiceExt;

/// One request per window: the second request on the same key is refused.
fn one_per_window() -> RateLimitConfig {
    RateLimitConfig::new("test", 1, 60, true)
}

fn app<B: StorageBackend + Clone + 'static>(mw: RateLimitMiddleware<B>) -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(from_fn_with_state(Arc::new(mw), rate_limit_middleware::<B>))
}

fn request(peer: &str, headers: &[(&str, &str)]) -> Request {
    let mut builder = Request::builder().uri("/");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let addr: SocketAddr = peer.parse().unwrap();
    builder
        .extension(ConnectInfo(addr))
        .body(Body::empty())
        .unwrap()
}

async fn status(app: &Router, req: Request) -> StatusCode {
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn each_peer_has_its_own_budget() {
    let app = app(RateLimitMiddleware::new(one_per_window()));
    assert_eq!(
        status(&app, request("203.0.113.1:5000", &[])).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&app, request("203.0.113.1:5001", &[])).await,
        StatusCode::TOO_MANY_REQUESTS,
        "the same address on another port is the same client"
    );
    assert_eq!(
        status(&app, request("203.0.113.2:5000", &[])).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_client_cannot_name_its_own_bucket() {
    let app = app(RateLimitMiddleware::new(one_per_window()));
    assert_eq!(
        status(
            &app,
            request("203.0.113.1:5000", &[("x-rate-limit-key", "a")])
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        status(
            &app,
            request("203.0.113.1:5000", &[("x-rate-limit-key", "b")])
        )
        .await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn forwarding_headers_from_an_untrusted_peer_are_ignored() {
    let app = app(RateLimitMiddleware::new(one_per_window()));
    let first = request("203.0.113.1:5000", &[("x-forwarded-for", "198.51.100.1")]);
    let second = request(
        "203.0.113.1:5000",
        &[
            ("x-forwarded-for", "198.51.100.2"),
            ("x-real-ip", "198.51.100.3"),
        ],
    );
    assert_eq!(status(&app, first).await, StatusCode::OK);
    assert_eq!(status(&app, second).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn behind_a_trusted_proxy_the_forwarded_client_is_the_key() {
    let proxies = TrustedProxies::parse("10.0.0.0/8, 172.16.0.0/12").unwrap();
    let app = app(RateLimitMiddleware::new(one_per_window()).with_trusted_proxies(proxies));
    let proxy = "172.18.0.5:40000";

    assert_eq!(
        status(&app, request(proxy, &[("x-forwarded-for", "198.51.100.1")])).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&app, request(proxy, &[("x-forwarded-for", "198.51.100.2")])).await,
        StatusCode::OK,
        "two clients behind one proxy keep separate budgets"
    );
    // A client-written leftmost entry is not trusted: the key is the rightmost address the
    // trusted proxies did not add themselves.
    assert_eq!(
        status(
            &app,
            request(
                proxy,
                &[("x-forwarded-for", "9.9.9.9, 198.51.100.1, 10.0.0.7")]
            )
        )
        .await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // X-Real-IP counts when the proxy sends no X-Forwarded-For.
    assert_eq!(
        status(&app, request(proxy, &[("x-real-ip", "198.51.100.3")])).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&app, request(proxy, &[("x-real-ip", "198.51.100.3")])).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn an_authenticated_route_can_key_on_its_principal() {
    // The extractor stands in for "the user id the auth layer proved"; a request it cannot key
    // falls back to the peer address.
    let mw = RateLimitMiddleware::new(one_per_window()).with_key_extractor(|req: &Request| {
        req.headers()
            .get("x-test-user")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    });
    let app = app(mw);
    let peer = "203.0.113.1:5000";
    assert_eq!(
        status(&app, request(peer, &[("x-test-user", "alice")])).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&app, request(peer, &[("x-test-user", "bob")])).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&app, request(peer, &[("x-test-user", "alice")])).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(status(&app, request(peer, &[])).await, StatusCode::OK);
    assert_eq!(
        status(&app, request(peer, &[])).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// A counter store that is down.
#[derive(Clone)]
struct BrokenStore;

#[async_trait::async_trait]
impl StorageBackend for BrokenStore {
    async fn increment(&self, _: &str, _: &RateLimitConfig) -> RateLimitResult<IncrementOutcome> {
        Err(RateLimitError::RedisConnection("down".into()))
    }
    async fn get_count(&self, _: &str, _: &RateLimitConfig) -> RateLimitResult<u64> {
        Err(RateLimitError::GetError)
    }
    async fn reset(&self, _: &str, _: &RateLimitConfig) -> RateLimitResult<()> {
        Err(RateLimitError::ResetError)
    }
}

#[tokio::test]
async fn a_failed_store_refuses_the_request_by_default() {
    let app = app(RateLimitMiddleware::with_backend(
        BrokenStore,
        one_per_window(),
    ));
    assert_eq!(
        status(&app, request("203.0.113.1:5000", &[])).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn failing_open_is_an_explicit_choice() {
    let app = app(
        RateLimitMiddleware::with_backend(BrokenStore, one_per_window())
            .with_fail_mode(FailMode::Open),
    );
    assert_eq!(
        status(&app, request("203.0.113.1:5000", &[])).await,
        StatusCode::OK
    );
}

#[test]
fn trusted_proxies_refuse_a_malformed_entry() {
    assert!(TrustedProxies::parse("10.0.0.0/33").is_err());
    assert!(TrustedProxies::parse("not-an-ip").is_err());
    assert!(TrustedProxies::parse("").unwrap().is_empty());
    assert!(TrustedProxies::parse("::1, fd00::/8, 127.0.0.1").is_ok());
}

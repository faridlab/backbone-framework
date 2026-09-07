//! The routing middleware over an in-memory factory: resolution → build-once dispatch → runtime
//! attached; fail-closed responses for hosts that name no tenant and tenants that cannot build.
//! The database-backed story (real provisioner, two tenant databases, the fence) lives in
//! `router_live.rs`, gated on a DSN.

#![cfg(feature = "axum")]

use std::sync::Arc;

use axum::{extract::Extension, routing::get, Router};
use http_body_util::BodyExt;
use tower::ServiceExt;

use backbone_tenant::axum_router::{tenant_route, TenantRouter, TENANT_OVERRIDE_HEADER};
use backbone_tenant::resolve::HostResolver;
use backbone_tenant::{TenantId, TenantRegistry, TenantRuntimeFactory};

/// A runtime with no database — proves the pool hand-off is optional, not load-bearing.
#[derive(Debug, Clone, PartialEq)]
struct Rt(String);

impl backbone_tenant::axum_router::ProvidesDatabase for Rt {
    fn database(&self) -> Option<&sqlx::PgPool> {
        None
    }
}

#[derive(Clone, Default)]
struct EchoFactory;

#[async_trait::async_trait]
impl TenantRuntimeFactory for EchoFactory {
    type Runtime = Rt;
    type Error = std::io::Error;

    async fn build(&self, tenant: &TenantId) -> Result<Rt, std::io::Error> {
        Ok(Rt(tenant.as_str().to_string()))
    }
}

async fn runtime_marker(Extension(rt): Extension<Arc<Rt>>) -> String {
    format!("tenant:{}", rt.0)
}

fn app<F>(resolver: HostResolver, registry: TenantRegistry<F>) -> Router
where
    F: TenantRuntimeFactory<Runtime = Rt> + 'static,
{
    Router::new()
        .route("/who", get(runtime_marker))
        .layer(axum::middleware::from_fn_with_state(
            TenantRouter::new(resolver, registry),
            tenant_route,
        ))
}

async fn body(res: axum::response::Response) -> String {
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

#[tokio::test]
async fn a_subdomain_request_lands_on_its_tenant_runtime() {
    let app = app(HostResolver::new("app.example.com"), TenantRegistry::new(EchoFactory, 8));
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/who")
                .header("host", "acme.app.example.com")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(body(res).await, "tenant:acme");
}

#[tokio::test]
async fn two_tenants_get_two_runtimes_and_builds_happen_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Default)]
    struct CountingFactory { builds: Arc<AtomicUsize> }
    #[async_trait::async_trait]
    impl TenantRuntimeFactory for CountingFactory {
        type Runtime = Rt;
        type Error = std::io::Error;
        async fn build(&self, tenant: &TenantId) -> Result<Rt, std::io::Error> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            Ok(Rt(tenant.as_str().to_string()))
        }
    }

    let builds = Arc::new(AtomicUsize::new(0));
    let factory = CountingFactory { builds: builds.clone() };
    let registry = TenantRegistry::new(factory, 8);
    let app = app(HostResolver::new("app.example.com"), registry);

    for host in ["acme.app.example.com", "globex.app.example.com", "acme.app.example.com"] {
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/who")
                    .header("host", host)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "{host} must route");
    }
    assert_eq!(builds.load(Ordering::SeqCst), 2, "two tenants → two builds; repeats are cached");
}

#[tokio::test]
async fn unknown_and_apex_hosts_never_reach_a_handler() {
    let app = app(HostResolver::new("app.example.com"), TenantRegistry::new(EchoFactory, 8));
    for host in ["elsewhere.example.org", "app.example.com", "Bad.Slug!.app.example.com"] {
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/who")
                    .header("host", host)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 404, "{host} must not name a tenant");
    }
}

#[tokio::test]
async fn an_override_header_against_a_strict_resolver_is_refused() {
    let app = app(HostResolver::new("app.example.com"), TenantRegistry::new(EchoFactory, 8));
    let res = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/who")
                .header("host", "acme.app.example.com")
                .header(TENANT_OVERRIDE_HEADER, "other")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn a_tenant_that_cannot_build_answers_unavailable_not_someone_elses_runtime() {
    #[derive(Clone)]
    struct BrokenFactory;
    #[async_trait::async_trait]
    impl TenantRuntimeFactory for BrokenFactory {
        type Runtime = Rt;
        type Error = std::io::Error;
        async fn build(&self, _t: &TenantId) -> Result<Rt, std::io::Error> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "db unreachable"))
        }
    }

    let app = app(HostResolver::new("app.example.com"), TenantRegistry::new(BrokenFactory, 8));
    let res = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/who")
                .header("host", "acme.app.example.com")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

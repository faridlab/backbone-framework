//! Live end-to-end proof of the tenancy request path (ADR-0027 + ADR-0028): host → tenant →
//! that tenant's database → org-scoped session → fenced rows.
//!
//! Provisions TWO real tenant databases through `TenantProvisioner` (each with a minimal org
//! spine and an org-fenced table, the same shapes the organization/inventory migrations emit),
//! serves one axum app over both — `tenant_route` outermost, `org_auth` inside it — and asserts:
//!
//! - a request per tenant host sees only that tenant's subtree (+ shared root rows) — never the
//!   other tenant's, and never rows outside its entitlement union;
//! - a token minted for tenant A's org tree is REFUSED on tenant B's host (403) — identity
//!   without tenancy is not access;
//! - an `entitled_units` claim widens the union to the entitled sister company;
//! - an unknown host is 404 before auth ever runs; a missing token is 401.
//!
//! Gated on `BACKBONE_TENANT_ROUTER_DSN` (a maintenance-database DSN whose role may
//! `CREATE DATABASE`, e.g. `postgresql://postgres:postgres@127.0.0.1:5433/postgres`). When
//! unset the test skips. It leaves no residue: databases dropped via `deprovision`, the
//! per-run app role dropped after.

#![cfg(all(feature = "axum", feature = "provision"))]

use std::sync::Arc;

use axum::{
    extract::{Extension, Request},
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use backbone_auth::org::{org_auth, OrgClaims, OrgContext, OrgVerifier};
use backbone_orm::company_scope::fetch_all_scoped;
use backbone_tenant::axum_router::{tenant_route, TenantRouter};
use backbone_tenant::provision::{PgPoolFactory, TenantProvisioner};
use backbone_tenant::resolve::HostResolver;
use backbone_tenant::{TenantId, TenantRegistry};

const JWT_SECRET: &[u8] = b"router-live-test-secret";
const BASE_DOMAIN: &str = "erp.test";

fn dsn() -> Option<String> {
    std::env::var("BACKBONE_TENANT_ROUTER_DSN").ok()
}

fn mint(claims: &OrgClaims) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET),
    )
    .unwrap()
}

/// A tenant's fixtures: its slug, its org-tree ids, the seeded row codes.
struct TenantFixtures {
    slug: String,
    root: Uuid,
    company: Uuid,
    branch: Uuid,
    sister: Uuid,
}

impl TenantFixtures {
    fn new(prefix: &str) -> Self {
        Self {
            slug: format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..8]),
            root: Uuid::new_v4(),
            company: Uuid::new_v4(),
            branch: Uuid::new_v4(),
            sister: Uuid::new_v4(),
        }
    }

    fn host(&self) -> String {
        format!("{}.{}", self.slug, BASE_DOMAIN)
    }
}

/// The schema each provisioned tenant database gets: the org spine the scope resolver needs,
/// one org-fenced table, and grants to the shared per-run app role (the role RLS binds to).
fn tenant_statements(role: &str) -> Vec<String> {
    let mut stmts = vec![
        "CREATE SCHEMA organization".to_string(),
        "CREATE TABLE organization.org_units ( \
             id uuid PRIMARY KEY, kind text NOT NULL, parent_id uuid, \
             code text, name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{}' \
         )"
        .to_string(),
        "CREATE UNIQUE INDEX one_root ON organization.org_units (kind) WHERE kind = 'root'"
            .to_string(),
        "CREATE OR REPLACE FUNCTION organization.org_unit_subtree(p_roots uuid[]) \
             RETURNS SETOF uuid LANGUAGE sql STABLE AS $$ \
             WITH RECURSIVE tree AS ( \
                 SELECT o.id FROM organization.org_units o WHERE o.id = ANY(p_roots) \
                 UNION ALL \
                 SELECT o.id FROM organization.org_units o JOIN tree t ON o.parent_id = t.id \
             ) SELECT id FROM tree $$"
            .to_string(),
        "CREATE OR REPLACE FUNCTION organization.org_unit_root() RETURNS uuid \
             LANGUAGE sql STABLE AS $$ \
             SELECT id FROM organization.org_units WHERE kind = 'root' $$"
            .to_string(),
        "CREATE SCHEMA tenant_router_live".to_string(),
        "CREATE TABLE tenant_router_live.items ( \
             id uuid PRIMARY KEY, org_unit_id uuid NOT NULL, code text \
         )"
        .to_string(),
        "ALTER TABLE tenant_router_live.items ENABLE ROW LEVEL SECURITY".to_string(),
        "ALTER TABLE tenant_router_live.items FORCE ROW LEVEL SECURITY".to_string(),
        "CREATE POLICY items_org_isolation ON tenant_router_live.items FOR ALL \
             USING (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])) \
             WITH CHECK (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[]))"
            .to_string(),
    ];
    stmts.push(format!("GRANT USAGE ON SCHEMA organization, tenant_router_live TO {role}"));
    stmts.push(format!("GRANT SELECT ON organization.org_units TO {role}"));
    stmts.push(format!("GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_router_live.items TO {role}"));
    stmts
}

/// Seed the org tree and one row per unit (as the admin user, which bypasses RLS). The row at
/// the root is the tenant-wide shared row every session sees; the sister-company row is what
/// only an `entitled_units` claim can reach.
async fn seed_tenant(provisioner: &TenantProvisioner, tenant: &TenantId, fx: &TenantFixtures, tag: &str) {
    let dsn = provisioner.tenant_dsn(tenant).unwrap();
    let admin = PgPoolOptions::new().max_connections(2).connect(&dsn).await.unwrap();
    sqlx::query(
        "INSERT INTO organization.org_units (id, kind, parent_id, code, name) VALUES \
             ($1, 'root',    NULL, 'ROOT', 'Tenant root'), \
             ($2, 'company', $1,   'CO',   'Company'), \
             ($3, 'branch',  $2,   'BR',   'Branch'), \
             ($4, 'company', $1,   'SIS',  'Sister Company')",
    )
    .bind(fx.root)
    .bind(fx.company)
    .bind(fx.branch)
    .bind(fx.sister)
    .execute(&admin)
    .await
    .unwrap();

    async fn row(admin: &PgPool, unit: Uuid, code: &str) {
        sqlx::query("INSERT INTO tenant_router_live.items (id, org_unit_id, code) VALUES ($1, $2, $3)")
            .bind(Uuid::new_v4())
            .bind(unit)
            .bind(code)
            .execute(admin)
            .await
            .unwrap();
    }
    row(&admin, fx.root, "ROOT-SHARED").await;
    row(&admin, fx.company, &format!("{tag}-CO")).await;
    row(&admin, fx.branch, &format!("{tag}-BR")).await;
    row(&admin, fx.sister, &format!("{tag}-SIS")).await;
    admin.close().await;
}

/// The guarded handler: runs INSIDE `with_org_request_scope` (org_auth wrapped it), so the
/// scoped fetch rides the request's dedicated connection and inherits the fence variables.
/// An ID-only query would be fenced identically — the connection carries the scope.
async fn items(Extension(pool): Extension<PgPool>, org: OrgContext) -> Response {
    let codes: Vec<String> = fetch_all_scoped(
        &pool,
        sqlx::query_as::<_, (String,)>("SELECT code FROM tenant_router_live.items ORDER BY code"),
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.0)
    .collect();
    Json(serde_json::json!({
        "acting_unit": org.acting_unit_id,
        "codes": codes,
    }))
    .into_response()
}

async fn send(app: &Router, host: &str, token: Option<&str>) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri("/items").header("host", host);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let res = app
        .clone()
        .oneshot(builder.body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

#[tokio::test]
async fn two_tenants_one_app_the_fence_holds() {
    let Some(admin_dsn) = dsn() else {
        eprintln!("skipping: set BACKBONE_TENANT_ROUTER_DSN");
        return;
    };

    // Per-run names so a failed prior run can never collide with this one.
    let role = format!("router_app_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let acme = TenantFixtures::new("acme");
    let globex = TenantFixtures::new("globex");
    let acme_id = TenantId::from(acme.slug.as_str());
    let globex_id = TenantId::from(globex.slug.as_str());

    let provisioner = TenantProvisioner::new(&admin_dsn);
    let admin = PgPoolOptions::new().max_connections(4).connect(&admin_dsn).await.unwrap();

    // The shared runtime login the tenant databases grant to — NOT the provisioning admin, so
    // RLS actually binds the runtime pools' connections.
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD 'routerpw'"))
        .execute(&admin)
        .await
        .unwrap();

    let owned_statements = tenant_statements(&role);
    let statements: Vec<&str> = owned_statements.iter().map(|s| s.as_str()).collect();
    provisioner.provision(&acme_id, &statements).await.unwrap();
    provisioner.provision(&globex_id, &statements).await.unwrap();
    seed_tenant(&provisioner, &acme_id, &acme, "ACME").await;
    seed_tenant(&provisioner, &globex_id, &globex, "GLOBEX").await;

    // The production-shaped runtime: pools on each tenant's database, connected as the
    // least-privilege role, built once per tenant behind the registry.
    let factory = PgPoolFactory::new(provisioner.clone())
        .connect_as(&role, "routerpw")
        .max_connections(2);
    let registry = Arc::new(TenantRegistry::new(factory, 8));
    let router_state = TenantRouter::with_registry(HostResolver::new(BASE_DOMAIN), registry.clone());

    // tenant_route is added LAST so it is the OUTERMOST layer: it picks the tenant (and the
    // pool) before org_auth scopes over it.
    let app = Router::new()
        .route("/items", get(items))
        .layer(from_fn_with_state(OrgVerifier::hs256(JWT_SECRET), org_auth))
        .layer(from_fn_with_state(router_state, tenant_route));

    let branch_token = |acting: Uuid, entitled: &[Uuid]| {
        mint(&OrgClaims {
            sub: "user-1".into(),
            exp: 4_102_444_800, // 2100-01-01: expiry is not what this test exercises
            org_unit_id: Some(acting),
            entitled_units: entitled.to_vec(),
        })
    };

    // 1. Each tenant's branch session sees its own subtree + the shared root row — and nothing
    //    of the other tenant's or the unentitled sister's.
    let (status, body) = send(&app, &acme.host(), Some(&branch_token(acme.branch, &[]))).await;
    assert_eq!(status, StatusCode::OK, "acme request: {body}");
    assert_eq!(body["codes"], serde_json::json!(["ACME-BR", "ROOT-SHARED"]));

    let (status, body) = send(&app, &globex.host(), Some(&branch_token(globex.branch, &[]))).await;
    assert_eq!(status, StatusCode::OK, "globex request: {body}");
    assert_eq!(body["codes"], serde_json::json!(["GLOBEX-BR", "ROOT-SHARED"]));

    // 2. Cross-tenant: acme's token is a valid identity carrying a unit that is not in globex's
    //    tree. The guard must refuse (403), never open a narrower session.
    let (status, _) = send(&app, &globex.host(), Some(&branch_token(acme.branch, &[]))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "acme token on globex host must be refused");

    // 3. Entitlement union: the same acting branch, plus the sister company, widens the fence
    //    to the sister's rows — still only within acme's tree.
    let (status, body) =
        send(&app, &acme.host(), Some(&branch_token(acme.branch, &[acme.sister]))).await;
    assert_eq!(status, StatusCode::OK, "entitled request: {body}");
    assert_eq!(
        body["codes"],
        serde_json::json!(["ACME-BR", "ACME-SIS", "ROOT-SHARED"]),
        "entitled_units must widen the union to the sister company's subtree"
    );

    // 4. Routing fails closed before auth: an unknown host never reaches a handler.
    let (status, _) = send(&app, "nowhere.example.org", Some(&branch_token(acme.branch, &[]))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 5. Auth fails closed: no token, no session.
    let (status, _) = send(&app, &acme.host(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Both tenants ended up resident, one runtime each.
    assert_eq!(registry.len().await, 2);

    // Residue-free teardown: databases first (drops the grants), then the role.
    provisioner.deprovision(&acme_id).await.unwrap();
    provisioner.deprovision(&globex_id).await.unwrap();
    sqlx::query(&format!("DROP ROLE {role}")).execute(&admin).await.unwrap();
    admin.close().await;
}

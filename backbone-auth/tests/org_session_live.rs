//! Live end-to-end proof of the org session chain: issuer → guard → spine → fence
//! (feature `axum`, ADR-0027/0028).
//!
//! Gated on `BACKBONE_AUTH_ORG_DSN` (a superuser DSN to a MAINTENANCE database — the test
//! creates and drops its own disposable `backbone_auth_org_live` database from it, so no
//! pre-provisioning is needed and nothing is left behind). The no-database contract lives in
//! `tests/org_guard.rs`; this file proves the chain that only exists against a real database:
//!
//! 1. `OrgIssuer` mints an access token for a real acting node (with an entitled sister
//!    company) — the exact credential a session bridge will hand a client.
//! 2. `org_auth` verifies it, resolves the entitlement union over the request's tenant pool
//!    (inserted the way `tenant_route` inserts it), and runs the handler scoped.
//! 3. The handler's fenced read — through the app role's pool, inside the request scope —
//!    returns exactly the entitled subtree union plus root rows, and nothing else.
//! 4. The issuer's refresh token is 401 on the same route; an access token naming a unit the
//!    tree does not hold is 403.
//!
//! Zero-residue: schemas, the per-run role, and the disposable database are dropped at the end.

#![cfg(feature = "axum")]

use axum::{
    body::Body,
    extract::Extension,
    http::{header, Request, StatusCode},
    middleware::{from_fn, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use backbone_auth::org::{org_auth, OrgContext, OrgIssuer, OrgVerifier};
use backbone_orm::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &[u8] = b"org-session-live-framework-test-secret";
const APP_PASSWORD: &str = "orglivetestpw";
/// The disposable database the proof builds its spine in — dedicated, so the `organization`
/// schema this test drops and recreates can never collide with another suite's.
const PROOF_DB: &str = "backbone_auth_org_live";

fn dsn() -> Option<String> {
    std::env::var("BACKBONE_AUTH_ORG_DSN").ok()
}

/// Mint the per-run role name — a fixed name breaks on shared dev clusters (`DROP ROLE` fails
/// while the role holds grants in another database), a fresh one can never collide.
fn role_name() -> String {
    format!("org_live_app_{}", &Uuid::new_v4().simple().to_string()[..8])
}

/// The same DSN retargeted at the disposable proof database.
fn proof_db_dsn(maintenance_dsn: &str) -> String {
    let (before_db, ..) = maintenance_dsn.rsplit_once('/').unwrap();
    format!("{before_db}/{PROOF_DB}")
}

async fn admin_pool(dsn: &str) -> PgPool {
    PgPoolOptions::new().max_connections(2).connect(dsn).await.unwrap()
}

async fn app_pool(dsn: &str, role: &str) -> PgPool {
    let after_at = dsn.rsplit('@').next().unwrap();
    let url = format!("postgresql://{role}:{APP_PASSWORD}@{after_at}");
    PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
}

/// The minimal org spine + one org-fenced table, in the same shapes the organization
/// migration emits — the resolver and the fence read these, nothing more is needed.
async fn setup(admin: &PgPool, role: &str, root: Uuid, company: Uuid, branch: Uuid, other: Uuid) {
    sqlx::raw_sql(&format!(
        "DROP SCHEMA IF EXISTS organization CASCADE; \
         DROP SCHEMA IF EXISTS org_live_test CASCADE; \
         CREATE SCHEMA organization; \
         CREATE TABLE organization.org_units ( \
             id uuid PRIMARY KEY, kind text NOT NULL, parent_id uuid, \
             code text, name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{{}}' \
         ); \
         CREATE UNIQUE INDEX one_root ON organization.org_units (kind) WHERE kind = 'root'; \
         CREATE OR REPLACE FUNCTION organization.org_unit_subtree(p_roots uuid[]) \
             RETURNS SETOF uuid LANGUAGE sql STABLE AS $$ \
             WITH RECURSIVE tree AS ( \
                 SELECT o.id FROM organization.org_units o WHERE o.id = ANY(p_roots) \
                 UNION ALL \
                 SELECT o.id FROM organization.org_units o JOIN tree t ON o.parent_id = t.id \
             ) SELECT id FROM tree $$; \
         CREATE OR REPLACE FUNCTION organization.org_unit_root() RETURNS uuid \
             LANGUAGE sql STABLE AS $$ \
             SELECT id FROM organization.org_units WHERE kind = 'root' $$; \
         CREATE SCHEMA org_live_test; \
         CREATE TABLE org_live_test.t (id uuid PRIMARY KEY, org_unit_id uuid NOT NULL, code text); \
         ALTER TABLE org_live_test.t ENABLE ROW LEVEL SECURITY; \
         ALTER TABLE org_live_test.t FORCE ROW LEVEL SECURITY; \
         CREATE POLICY t_org_isolation ON org_live_test.t FOR ALL \
             USING (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])) \
             WITH CHECK (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])); \
         CREATE ROLE {role} LOGIN PASSWORD '{APP_PASSWORD}'; \
         GRANT USAGE ON SCHEMA organization, org_live_test TO {role}; \
         GRANT SELECT ON organization.org_units TO {role}; \
         GRANT SELECT ON org_live_test.t TO {role};",
    ))
    .execute(admin)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO organization.org_units (id, kind, parent_id, code, name) VALUES \
             ($1, 'root',    NULL, 'ROOT',  'Tenant root'), \
             ($2, 'company', $1,   'CO',    'Company'), \
             ($3, 'branch',  $2,   'BR',    'Branch'), \
             ($4, 'company', $1,   'OTHER', 'Other Company')",
    )
    .bind(root)
    .bind(company)
    .bind(branch)
    .bind(other)
    .execute(admin)
    .await
    .unwrap();

    // One row per node kind: the acting branch, its parent company (NOT in the branch's
    // subtree — subtree() descends only), the entitled sister company, and a root-shared row.
    sqlx::query(
        "INSERT INTO org_live_test.t (id, org_unit_id, code) VALUES \
             ($1, $2, 'BR-WH'), ($3, $4, 'CO-WH'), ($5, $6, 'OTHER-WH'), ($7, $8, 'ROOT-SHARED')",
    )
    .bind(Uuid::new_v4())
    .bind(branch)
    .bind(Uuid::new_v4())
    .bind(company)
    .bind(Uuid::new_v4())
    .bind(other)
    .bind(Uuid::new_v4())
    .bind(root)
    .execute(admin)
    .await
    .unwrap();
}

/// The guarded handler: echoes the proven session and reads through the fence — the same two
/// things every org-guarded handler in a real service does.
async fn org_data(Extension(pool): Extension<PgPool>, org: OrgContext) -> Response {
    let codes: Vec<String> = backbone_orm::company_scope::fetch_all_scoped(
        &pool,
        sqlx::query_as::<_, (String,)>("SELECT code FROM org_live_test.t ORDER BY code"),
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.0)
    .collect();
    (
        StatusCode::OK,
        [
            ("x-acting-unit", org.acting_unit_id.to_string()),
            ("x-entitled", org.entitled_units.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",")),
            ("x-user", org.user_id.clone()),
            ("x-codes", codes.join(",")),
        ],
    )
        .into_response()
}

async fn call(app: Router, bearer: &str) -> Response {
    app.oneshot(
        Request::builder()
            .method("GET")
            .uri("/org-data")
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn issuer_guard_spine_fence_chain() {
    let Some(maintenance_dsn) = dsn() else {
        eprintln!("skipping: set BACKBONE_AUTH_ORG_DSN to a maintenance database");
        return;
    };
    // A dedicated disposable database: created from the maintenance DSN, dropped at the end.
    // Re-create (not reuse) so a crashed earlier run leaves no stale spine behind. Each
    // statement rides its own simple-protocol query — CREATE/DROP DATABASE may not run inside
    // a transaction block, and a multi-statement string would be one implicit transaction.
    let maintenance = admin_pool(&maintenance_dsn).await;
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {PROOF_DB} WITH (FORCE)"))
        .execute(&maintenance)
        .await
        .unwrap();
    sqlx::raw_sql(&format!("CREATE DATABASE {PROOF_DB}"))
        .execute(&maintenance)
        .await
        .unwrap();
    let proof_dsn = proof_db_dsn(&maintenance_dsn);

    let root = Uuid::new_v4();
    let company = Uuid::new_v4();
    let branch = Uuid::new_v4();
    let other = Uuid::new_v4();
    let role = role_name();
    let admin = admin_pool(&proof_dsn).await;
    setup(&admin, &role, root, company, branch, other).await;
    let pool = app_pool(&proof_dsn, &role).await;

    // The wiring tenant_route performs: the tenant pool rides the request extensions, OUTSIDE
    // the guard — org_auth resolves the scope over exactly this pool.
    let insert_pool = {
        let pool = pool.clone();
        from_fn(move |mut req: Request<Body>, next: axum::middleware::Next| {
            let pool = pool.clone();
            async move {
                req.extensions_mut().insert(pool);
                next.run(req).await
            }
        })
    };
    let app = Router::new()
        .route("/org-data", get(org_data))
        .layer(from_fn_with_state(OrgVerifier::hs256(SECRET), org_auth))
        .layer(insert_pool);

    let issuer = OrgIssuer::hs256(SECRET);
    let user = Uuid::new_v4().to_string();

    // 1. The full happy path: minted access token → guard → entitlement-union fence.
    let access = issuer
        .issue_access(&user, branch, &[other], None, Duration::from_secs(3600))
        .unwrap();
    let res = call(app.clone(), &access).await;
    assert_eq!(res.status(), StatusCode::OK, "minted access token must pass the guard");
    let h = res.headers();
    assert_eq!(h["x-acting-unit"], branch.to_string());
    assert_eq!(h["x-entitled"], other.to_string());
    assert_eq!(h["x-user"], user);
    // Branch's own subtree (itself) + the entitled sister's subtree + the root's shared row.
    // The parent company's row is NOT visible: subtree() descends, it does not climb.
    assert_eq!(h["x-codes"], "BR-WH,OTHER-WH,ROOT-SHARED");

    // 2. The refresh twin of the very same session is refused on the guarded route.
    let refresh = issuer
        .issue_refresh(&user, branch, &[other], None, Duration::from_secs(7 * 24 * 3600))
        .unwrap();
    let res = call(app.clone(), &refresh).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "refresh token must not open a scoped session");

    // 3. An access token naming a unit this tree does not hold is 403 — identity without
    //    tenancy is not access.
    let ghost = issuer
        .issue_access(&user, Uuid::new_v4(), &[], None, Duration::from_secs(3600))
        .unwrap();
    let res = call(app.clone(), &ghost).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "unknown acting unit must be refused, not narrowed");

    // 4. No token, no session.
    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/org-data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Zero residue: schemas + role in the proof database, then the database itself.
    sqlx::raw_sql(&format!(
        "DROP SCHEMA organization CASCADE; DROP SCHEMA org_live_test CASCADE; DROP ROLE {role};"
    ))
    .execute(&admin)
    .await
    .unwrap();
    pool.close().await;
    admin.close().await;
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {PROOF_DB} WITH (FORCE);"))
        .execute(&maintenance)
        .await
        .unwrap();
    maintenance.close().await;
}

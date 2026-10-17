//! Live proof of grants read as data (feature `identity-grants`).
//!
//! Gated on `BACKBONE_AUTH_GRANTS_DSN`, a superuser DSN to a MAINTENANCE database: the test creates
//! and drops its own disposable `backbone_auth_grants_live` database from it, so nothing is
//! provisioned beforehand and nothing is left behind.
//!
//! It builds the identity tables [`IdentityGrantSource`] reads, in the columns it reads, and proves:
//!
//! 1. each of the four grant routes (an assigned role, the role on the acting unit's membership, a
//!    direct grant on the user, a direct grant in the identity module) reaches the request's
//!    `AuthContext` through `resolve_grants`;
//! 2. a revoked, expired, suspended, soft-deleted or record-narrowed grant is not a grant;
//! 3. a tenant role created as rows, after the service started, grants access with no regeneration
//!    and no restart: the next request sees it;
//! 4. `ADMIN` holds everything by name and a trusted role holds its module whole;
//! 5. the source reads the tenant's own pool off the request, not the one it was built with;
//! 6. fail-closed: with the store unreadable the request is `503` and the connection error is never
//!    sent. (A route with no resolver installed refusing an empty `AuthContext` is proven in
//!    `backbone-core`'s permission tests and in the generator's conformance run.)

#![cfg(feature = "identity-grants")]

use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode},
    middleware::from_fn_with_state,
    routing::get,
    Router,
};
use backbone_auth::middleware::AuthContext;
use backbone_auth::{resolve_grants, GrantResolver, IdentityGrantSource};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const PROOF_DB: &str = "backbone_auth_grants_live";

fn dsn() -> Option<String> {
    std::env::var("BACKBONE_AUTH_GRANTS_DSN").ok()
}

fn proof_db_dsn(maintenance_dsn: &str) -> String {
    let (before_db, ..) = maintenance_dsn.rsplit_once('/').unwrap();
    format!("{before_db}/{PROOF_DB}")
}

const DDL: &str = r#"
CREATE SCHEMA IF NOT EXISTS sapiens;
CREATE TABLE public.roles (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE public.permissions (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE public.role_permissions (role_id uuid NOT NULL, permission_id uuid NOT NULL, metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE public.user_roles (user_id uuid NOT NULL, role_id uuid NOT NULL, metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE public.user_permissions (user_id uuid NOT NULL, permission_id uuid NOT NULL, status text NOT NULL DEFAULT 'active',
    revoked_at timestamptz, expires_at timestamptz, resource_id uuid, metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE sapiens.organization_users (user_id uuid NOT NULL, org_unit_id uuid NOT NULL, role_id uuid, status text NOT NULL DEFAULT 'active',
    metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE sapiens.direct_permission_grants (user_id uuid NOT NULL, permission_id uuid NOT NULL, status text NOT NULL DEFAULT 'active',
    revoked_at timestamptz, expires_at timestamptz, scope jsonb, conditions jsonb, metadata jsonb NOT NULL DEFAULT '{}');
"#;

async fn permission(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO public.permissions (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn role(pool: &PgPool, name: &str, permissions: &[&str]) -> Uuid {
    let id: Uuid = sqlx::query_scalar("INSERT INTO public.roles (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
    for p in permissions {
        let pid = permission(pool, p).await;
        sqlx::query("INSERT INTO public.role_permissions (role_id, permission_id) VALUES ($1, $2)")
            .bind(id)
            .bind(pid)
            .execute(pool)
            .await
            .unwrap();
    }
    id
}

async fn whoami(ctx: Option<Extension<AuthContext>>) -> String {
    match ctx {
        Some(Extension(c)) => format!("{}|{}", c.roles.join(","), c.permissions.join(",")),
        None => "none".into(),
    }
}

fn router(source: IdentityGrantSource) -> Router {
    Router::new()
        .route("/", get(whoami))
        .route_layer(from_fn_with_state(
            GrantResolver::new(source),
            resolve_grants,
        ))
}

async fn ask(
    router: &Router,
    user: Uuid,
    unit: Option<Uuid>,
    pool: Option<PgPool>,
) -> (StatusCode, String) {
    let mut req = Request::builder().uri("/").body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(AuthContext::new(user.to_string()));
    if let Some(unit) = unit {
        // The session guard's context: the acting unit the membership grant is read for.
        req.extensions_mut().insert(backbone_auth::org::OrgContext {
            acting_unit_id: unit,
            entitled_units: vec![],
            legacy_company_id: None,
            user_id: user.to_string(),
        });
    }
    if let Some(pool) = pool {
        req.extensions_mut().insert(pool);
    }
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&body).to_string())
}

fn perms_of(body: &str) -> Vec<String> {
    let (_, perms) = body.split_once('|').unwrap_or(("", ""));
    perms
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn grants_are_read_as_data_and_nothing_is_granted_by_default() {
    let Some(maintenance) = dsn() else {
        eprintln!("BACKBONE_AUTH_GRANTS_DSN not set: skipping the live grants proof");
        return;
    };
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&maintenance)
        .await
        .unwrap();
    sqlx::query(&format!("DROP DATABASE IF EXISTS {PROOF_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {PROOF_DB}"))
        .execute(&admin)
        .await
        .unwrap();
    let outcome = tokio::spawn(prove(proof_db_dsn(&maintenance))).await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {PROOF_DB} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;
    outcome.unwrap();
}

async fn prove(dsn: String) {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&dsn)
        .await
        .unwrap();
    sqlx::raw_sql(DDL).execute(&pool).await.unwrap();
    let source = IdentityGrantSource::new(pool.clone()).trusting_roles(vec![(
        "ACCOUNTANT".to_string(),
        vec!["accounting".to_string()],
    )]);
    let router = router(source);
    let unit = Uuid::new_v4();

    // 1. The four grant routes.
    let by_role = Uuid::new_v4();
    let clerk = role(&pool, "CLERK", &["ticket:read", "ticket:create"]).await;
    sqlx::query("INSERT INTO public.user_roles (user_id, role_id) VALUES ($1, $2)")
        .bind(by_role)
        .bind(clerk)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = ask(&router, by_role, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        perms_of(&body),
        vec!["ticket:create", "ticket:read"],
        "an assigned role, through its permission rows: {body}"
    );
    assert!(body.starts_with("CLERK|"), "{body}");

    let by_unit = Uuid::new_v4();
    let reviewer = role(&pool, "REVIEWER", &["ticket:update"]).await;
    sqlx::query("INSERT INTO sapiens.organization_users (user_id, org_unit_id, role_id) VALUES ($1, $2, $3)")
        .bind(by_unit).bind(unit).bind(reviewer).execute(&pool).await.unwrap();
    let (_, body) = ask(&router, by_unit, Some(unit), None).await;
    assert_eq!(
        perms_of(&body),
        vec!["ticket:update"],
        "the role on the acting unit's membership: {body}"
    );
    let (_, body) = ask(&router, by_unit, Some(Uuid::new_v4()), None).await;
    assert!(
        perms_of(&body).is_empty(),
        "a membership of another unit grants nothing here: {body}"
    );

    let direct = Uuid::new_v4();
    let p = permission(&pool, "project:read").await;
    sqlx::query("INSERT INTO public.user_permissions (user_id, permission_id) VALUES ($1, $2)")
        .bind(direct)
        .bind(p)
        .execute(&pool)
        .await
        .unwrap();
    let p2 = permission(&pool, "project:update").await;
    sqlx::query(
        "INSERT INTO sapiens.direct_permission_grants (user_id, permission_id) VALUES ($1, $2)",
    )
    .bind(direct)
    .bind(p2)
    .execute(&pool)
    .await
    .unwrap();
    let (_, body) = ask(&router, direct, None, None).await;
    assert_eq!(
        perms_of(&body),
        vec!["project:read", "project:update"],
        "both direct grants: {body}"
    );

    // 2. What is not a grant.
    let lapsed = Uuid::new_v4();
    let pa = permission(&pool, "a:read").await;
    let pb = permission(&pool, "b:read").await;
    let pc = permission(&pool, "c:read").await;
    let pd = permission(&pool, "d:read").await;
    let pe = permission(&pool, "e:read").await;
    for (pid, extra) in [
        (pa, "status = 'suspended'"),
        (pb, "revoked_at = now()"),
        (pc, "expires_at = now() - interval '1 hour'"),
        (pd, "resource_id = gen_random_uuid()"),
        (pe, "metadata = '{\"deleted_at\":\"2020-01-01\"}'"),
    ] {
        sqlx::query("INSERT INTO public.user_permissions (user_id, permission_id) VALUES ($1, $2)")
            .bind(lapsed)
            .bind(pid)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!(
            "UPDATE public.user_permissions SET {extra} WHERE user_id = $1 AND permission_id = $2"
        ))
        .bind(lapsed)
        .bind(pid)
        .execute(&pool)
        .await
        .unwrap();
    }
    let pf = permission(&pool, "f:read").await;
    sqlx::query("INSERT INTO sapiens.direct_permission_grants (user_id, permission_id, scope) VALUES ($1, $2, '{\"org\":\"x\"}')").bind(lapsed).bind(pf).execute(&pool).await.unwrap();
    let (_, body) = ask(&router, lapsed, None, None).await;
    assert!(
        perms_of(&body).is_empty(),
        "revoked, expired, suspended, soft-deleted and narrowed grants hold nothing: {body}"
    );

    // 3. A tenant role created as data after the router was built grants at the next request.
    let (_, before) = ask(&router, lapsed, None, None).await;
    assert!(perms_of(&before).is_empty());
    let auditor = role(&pool, "AUDITOR", &["ticket:read", "report:*"]).await;
    sqlx::query("INSERT INTO public.user_roles (user_id, role_id) VALUES ($1, $2)")
        .bind(lapsed)
        .bind(auditor)
        .execute(&pool)
        .await
        .unwrap();
    let (_, after) = ask(&router, lapsed, None, None).await;
    assert_eq!(
        perms_of(&after),
        vec!["report:*", "ticket:read"],
        "a role added as rows grants with no restart: {after}"
    );
    // ...and revoking it removes the access just as fast.
    sqlx::query("UPDATE public.user_roles SET metadata = '{\"deleted_at\":\"2026-01-01\"}' WHERE user_id = $1").bind(lapsed).execute(&pool).await.unwrap();
    let (_, gone) = ask(&router, lapsed, None, None).await;
    assert!(perms_of(&gone).is_empty(), "{gone}");

    // 4. Authority by name, and a trusted role.
    let boss = Uuid::new_v4();
    let admin_role = role(&pool, "ADMIN", &[]).await;
    sqlx::query("INSERT INTO public.user_roles (user_id, role_id) VALUES ($1, $2)")
        .bind(boss)
        .bind(admin_role)
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = ask(&router, boss, None, None).await;
    assert_eq!(
        perms_of(&body),
        vec!["*:*"],
        "ADMIN holds everything by name, with no permission rows: {body}"
    );
    let books = Uuid::new_v4();
    let accountant = role(&pool, "ACCOUNTANT", &[]).await;
    sqlx::query("INSERT INTO public.user_roles (user_id, role_id) VALUES ($1, $2)")
        .bind(books)
        .bind(accountant)
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = ask(&router, books, None, None).await;
    assert_eq!(perms_of(&body), vec!["accounting:*"], "{body}");

    // 5. The tenant's own pool rides the request. A source built over a pool that cannot answer
    // still resolves when the request carries the tenant's pool.
    let dead = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy("postgresql://nobody:x@127.0.0.1:1/none")
        .unwrap();
    let router_dead = self::router(IdentityGrantSource::new(dead));
    let (status, _) = ask(&router_dead, by_role, None, None).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a source that cannot read its store refuses the request (fail closed)"
    );
    let (status, body) = ask(&router_dead, by_role, None, Some(pool.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        perms_of(&body),
        vec!["ticket:create", "ticket:read"],
        "the request's own pool answered"
    );
    assert!(
        !body.contains("nobody"),
        "the connection error is never sent: {body}"
    );
}

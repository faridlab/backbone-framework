//! Live proof that a request scope whose future is dropped mid-request (a client disconnect, a
//! timeout layer) never returns its connection to the pool still carrying the session variables
//! it set.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://postgres:postgres@localhost:5433/postgres`). When it is unset the tests skip, so a
//! checkout without a database still passes.
//!
//! The defect these guard against: the scope reset its variables only after the wrapped future
//! completed. A dropped future never reached that reset, so the next request to draw the same
//! pooled connection ran with the previous request's company, org fence and audit actor.

use backbone_orm::company_scope::with_request_scope;
use backbone_orm::org_scope::{with_org_request_scope_and_audit, OrgScope};
use backbone_orm::RequestAuditContext;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

/// One connection, so the read after the cancelled scope lands on the connection it used.
async fn single_connection_pool(dsn: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(dsn)
        .await
        .expect("connect")
}

/// Every scope variable as the next borrower of the pooled connection sees it.
async fn settings_on_next_borrow(pool: &PgPool) -> Vec<(String, String)> {
    let mut conn = pool
        .acquire()
        .await
        .expect("the cancelled scope's connection comes back");
    let mut out = Vec::new();
    for var in [
        "app.company_id",
        "app.scope_unit_ids",
        "app.acting_unit_id",
        "app.actor",
        "app.correlation_id",
    ] {
        let value: Option<String> = sqlx::query_scalar("SELECT current_setting($1, true)")
            .bind(var)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        out.push((var.to_string(), value.unwrap_or_default()));
    }
    out
}

fn assert_clean(settings: &[(String, String)]) {
    let dirty: Vec<_> = settings.iter().filter(|(_, v)| !v.is_empty()).collect();
    assert!(
        dirty.is_empty(),
        "a cancelled request scope returned its connection still scoped: {dirty:?}"
    );
}

#[tokio::test]
async fn a_cancelled_org_scope_returns_a_clean_connection() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;
    let unit = Uuid::new_v4();
    let mut audit = RequestAuditContext::new("cancelled-actor");
    audit.correlation_id = "corr-1".into();

    // The wrapped request never finishes; the timeout drops the whole scope future mid-flight,
    // exactly as a client disconnect drops an axum handler future.
    let outcome = tokio::time::timeout(
        Duration::from_millis(200),
        with_org_request_scope_and_audit(&pool, OrgScope::for_company_unit(unit), audit, async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }),
    )
    .await;
    assert!(outcome.is_err(), "the scope must have been cancelled");

    assert_clean(&settings_on_next_borrow(&pool).await);
}

#[tokio::test]
async fn a_cancelled_company_scope_returns_a_clean_connection() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;

    let outcome = tokio::time::timeout(
        Duration::from_millis(200),
        with_request_scope(&pool, Uuid::new_v4(), async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }),
    )
    .await;
    assert!(outcome.is_err(), "the scope must have been cancelled");

    assert_clean(&settings_on_next_borrow(&pool).await);
}

#[tokio::test]
async fn a_completed_scope_still_resets_before_returning() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;
    let unit = Uuid::new_v4();
    with_org_request_scope_and_audit(
        &pool,
        OrgScope::for_company_unit(unit),
        RequestAuditContext::new("done-actor"),
        async {},
    )
    .await
    .unwrap();
    assert_clean(&settings_on_next_borrow(&pool).await);
}

/// A branch under a subsidiary company under a parent company acts as the SUBSIDIARY: the
/// legacy company fence is the nearest company ancestor, whatever the ids sort to.
#[tokio::test]
async fn the_legacy_company_is_the_nearest_company_ancestor() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = single_connection_pool(&dsn).await;
    let mut conn = pool.acquire().await.unwrap();
    // A throwaway org spine in its own transaction, rolled back at the end — so whatever spine
    // the database already holds is back untouched afterwards.
    let mut tx = sqlx::Connection::begin(&mut *conn).await.unwrap();
    sqlx::raw_sql(
        "DROP SCHEMA IF EXISTS organization CASCADE; \
         CREATE SCHEMA organization; \
         CREATE TABLE organization.org_units ( \
            id uuid PRIMARY KEY, parent_id uuid, kind text NOT NULL); \
         CREATE FUNCTION organization.org_unit_subtree(roots uuid[]) \
           RETURNS SETOF uuid LANGUAGE sql AS $$ \
             WITH RECURSIVE d AS ( \
               SELECT id FROM organization.org_units WHERE id = ANY(roots) \
               UNION SELECT o.id FROM organization.org_units o JOIN d ON o.parent_id = d.id) \
             SELECT id FROM d $$; \
         CREATE FUNCTION organization.org_unit_root() RETURNS uuid LANGUAGE sql AS $$ \
             SELECT id FROM organization.org_units WHERE parent_id IS NULL LIMIT 1 $$;",
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    // Ids chosen so the PARENT company sorts lowest: an id-ordered pick lands on it.
    let root = Uuid::parse_str("00000000-0000-0000-0000-0000000000f0").unwrap();
    let parent = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let subsidiary = Uuid::parse_str("ffffffff-0000-0000-0000-000000000002").unwrap();
    let branch = Uuid::parse_str("00000000-0000-0000-0000-0000000000b0").unwrap();
    for (id, parent_id, kind) in [
        (root, None, "root"),
        (parent, Some(root), "company"),
        (subsidiary, Some(parent), "company"),
        (branch, Some(subsidiary), "branch"),
    ] {
        sqlx::query("INSERT INTO organization.org_units (id, parent_id, kind) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(parent_id)
            .bind(kind)
            .execute(&mut *tx)
            .await
            .unwrap();
    }

    let scope = backbone_orm::org_scope::resolve_org_scope(&mut tx, branch, &[])
        .await
        .unwrap();
    let subsidiary_scope = backbone_orm::org_scope::resolve_org_scope(&mut tx, subsidiary, &[])
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    assert_eq!(
        scope.legacy_company_id(),
        Some(subsidiary),
        "a branch acts as its own company"
    );
    assert_eq!(
        subsidiary_scope.legacy_company_id(),
        Some(subsidiary),
        "a company is its own"
    );
}

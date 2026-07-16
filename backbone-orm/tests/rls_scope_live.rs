//! Live proof that the company scope wrapper + Postgres RLS fence the ORM read/write path.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a **superuser** DSN, e.g.
//! `postgresql://postgres:postgres@localhost:5433/postgres`) — the test needs to create a role and
//! a policy. When it is unset the test skips, so a checkout without a database still passes.
//!
//! It sets up a company-fenced table exactly as ADR-0008's codegen would, connects a **non-superuser**
//! pool as the request role, and drives the real `PostgresRepository` through `with_company_scope`:
//! company A sees only A's rows, an unscoped read sees nothing (fail-closed), and a write forging
//! another company's id is rejected by `WITH CHECK`.

use backbone_orm::company_scope::with_company_scope;
use backbone_orm::repository::{DatabaseOperations, PostgresRepository};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct Widget {
    id: Uuid,
    company_id: Uuid,
    name: String,
}

const TABLE: &str = "rls_orm_test.widget";
const APP_DSN_ROLE: &str = "rls_orm_app";

async fn setup(admin: &PgPool, a: Uuid, b: Uuid) {
    // Clean slate.
    let _ = sqlx::raw_sql(
        "DROP SCHEMA IF EXISTS rls_orm_test CASCADE; \
         DROP ROLE IF EXISTS rls_orm_app;",
    )
    .execute(admin)
    .await;

    sqlx::raw_sql(&format!(
        "CREATE SCHEMA rls_orm_test; \
         CREATE TABLE {TABLE} ( \
            id uuid PRIMARY KEY DEFAULT gen_random_uuid(), \
            company_id uuid NOT NULL, \
            name text NOT NULL \
         ); \
         ALTER TABLE {TABLE} ENABLE ROW LEVEL SECURITY; \
         ALTER TABLE {TABLE} FORCE  ROW LEVEL SECURITY; \
         CREATE POLICY widget_company_isolation ON {TABLE} \
            FOR ALL \
            USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid) \
            WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid); \
         CREATE ROLE {APP_DSN_ROLE} LOGIN PASSWORD 'rlspw'; \
         GRANT USAGE ON SCHEMA rls_orm_test TO {APP_DSN_ROLE}; \
         GRANT SELECT, INSERT, UPDATE, DELETE ON {TABLE} TO {APP_DSN_ROLE};"
    ))
    .execute(admin)
    .await
    .expect("setup schema/role/policy");

    // Seed as the superuser (bypasses RLS): two rows for A, one for B.
    for (company, name) in [(a, "a-1"), (a, "a-2"), (b, "b-1")] {
        sqlx::query(&format!("INSERT INTO {TABLE} (id, company_id, name) VALUES ($1, $2, $3)"))
            .bind(Uuid::new_v4())
            .bind(company)
            .bind(name)
            .execute(admin)
            .await
            .expect("seed row");
    }
}

/// A pool that logs in as the non-superuser request role, derived from the admin DSN's host/port.
async fn app_pool(admin_dsn: &str) -> PgPool {
    // Swap credentials to the request role; keep host/port/db from the admin DSN.
    let after_at = admin_dsn.rsplit('@').next().expect("dsn has @host part");
    let dsn = format!("postgresql://{APP_DSN_ROLE}:rlspw@{after_at}");
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&dsn)
        .await
        .expect("connect as request role")
}

#[tokio::test]
async fn orm_reads_and_writes_are_company_fenced() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: set BACKBONE_ORM_RLS_DSN to run the live RLS scope test");
        return;
    };
    let admin = PgPool::connect(&dsn).await.expect("admin connect");
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    setup(&admin, a, b).await;

    // The repository runs as the non-superuser role, so RLS actually applies to it.
    let pool = app_pool(&dsn).await;
    let repo: PostgresRepository<Widget> = PostgresRepository::new(pool.clone(), TABLE);

    // Scoped to A → sees only A's two rows.
    let a_rows = with_company_scope(Some(a), repo.find_all()).await.expect("find_all A");
    assert_eq!(a_rows.len(), 2, "company A must see exactly its 2 rows, got {a_rows:?}");
    assert!(a_rows.iter().all(|w| w.company_id == a), "A must see only A rows");

    // Scoped to B → sees only B's row; A is invisible.
    let b_rows = with_company_scope(Some(b), repo.find_all()).await.expect("find_all B");
    assert_eq!(b_rows.len(), 1, "company B must see exactly its 1 row");
    assert_eq!(b_rows[0].company_id, b);

    // No scope set → fail-closed, zero rows (never a leak, never an error).
    let unscoped = repo.find_all().await.expect("unscoped find_all must not error");
    assert!(unscoped.is_empty(), "an unscoped read must see zero rows (fail-closed), got {unscoped:?}");

    // Write under scope A succeeds and is itself only visible to A.
    let new = Widget { id: Uuid::new_v4(), company_id: a, name: "a-3".into() };
    let created = with_company_scope(Some(a), repo.create(&new)).await.expect("create under A");
    assert_eq!(created.company_id, a);
    let a_after = with_company_scope(Some(a), repo.find_all()).await.unwrap();
    assert_eq!(a_after.len(), 3, "A now has 3 rows");

    // Forgery: scoped to A, try to create a B-owned row → WITH CHECK rejects it.
    let forged = Widget { id: Uuid::new_v4(), company_id: b, name: "forged".into() };
    let err = with_company_scope(Some(a), repo.create(&forged)).await;
    assert!(err.is_err(), "a caller scoped to A must not be able to write a B-owned row");

    // Cleanup.
    drop(pool);
    let _ = sqlx::raw_sql("DROP SCHEMA IF EXISTS rls_orm_test CASCADE; DROP ROLE IF EXISTS rls_orm_app;")
        .execute(&admin)
        .await;
}

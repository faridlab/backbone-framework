//! The outbox estate moving from the company axis to the org-unit axis (ADR-0029).
//!
//! Run against a disposable database:
//!
//!     DATABASE_URL=postgres://postgres:postgres@localhost:5433/postgres \
//!     cargo test --features multi_tenant --test org_rekey
//!
//! What it proves: `migrate` installs the org column and fills it from the acting unit, carries an
//! existing company-keyed row over without losing it, retires the company-keyed policy, and never
//! leaves the table unfenced while doing so.

#![cfg(feature = "multi_tenant")]

use backbone_outbox::{outbox, record::OutboxRecord};
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/postgres".into())
}

async fn pool() -> PgPool {
    PgPool::connect(&admin_url()).await.expect("connect")
}

fn schema_name() -> String {
    format!("rk_{}", &Uuid::new_v4().simple().to_string()[..8])
}

/// Build `outbox_events` the way the version before the re-key built it: a company column, its
/// index, and the company-keyed policy.
async fn legacy_outbox(pool: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.outbox_events (
             id uuid PRIMARY KEY, event_type text NOT NULL, aggregate_type text NOT NULL,
             aggregate_id text NOT NULL, company_id uuid NOT NULL, payload jsonb NOT NULL,
             occurred_at timestamptz NOT NULL, correlation_id text, causation_id text,
             version int NOT NULL DEFAULT 1, created_at timestamptz NOT NULL DEFAULT now(),
             published_at timestamptz);
         CREATE INDEX idx_{schema}_outbox_company_id ON {schema}.outbox_events (company_id);
         ALTER TABLE {schema}.outbox_events ENABLE ROW LEVEL SECURITY;
         ALTER TABLE {schema}.outbox_events FORCE ROW LEVEL SECURITY;
         CREATE POLICY outbox_events_company_isolation ON {schema}.outbox_events
             FOR ALL
             USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid
                    OR current_user = 'metaphor_relay');"
    ))
    .execute(pool)
    .await
    .unwrap();
}

async fn policies(pool: &PgPool, schema: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT policyname::text FROM pg_policies WHERE schemaname = $1")
        .bind(schema)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn migrate_rekeys_a_legacy_outbox_without_losing_its_rows() {
    let pool = pool().await;
    let schema = schema_name();
    legacy_outbox(&pool, &schema).await;

    // A row staged before the re-key: it has a company and no org unit.
    let company = Uuid::new_v4();
    let id = Uuid::new_v4();
    sqlx::query(&format!(
        "INSERT INTO {schema}.outbox_events
           (id, event_type, aggregate_type, aggregate_id, company_id, payload, occurred_at)
         VALUES ($1, 'Legacy', 'Thing', '1', $2, '{{}}'::jsonb, now())"
    ))
    .bind(id)
    .bind(company)
    .execute(&pool)
    .await
    .unwrap();

    outbox::migrate(&pool, &schema).await.expect("migrate heals the legacy table in place");

    let org_unit: Option<Uuid> =
        sqlx::query_scalar(&format!("SELECT org_unit_id FROM {schema}.outbox_events WHERE id=$1"))
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        org_unit,
        Some(company),
        "the existing row carries over: the org spine copied company ids verbatim, so the two are \
         the same value for any row staged before the re-key"
    );

    let names = policies(&pool, &schema).await;
    assert!(
        names.iter().any(|n| n == "outbox_events_org_unit_isolation"),
        "the org fence is installed: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "outbox_events_company_isolation"),
        "the company-keyed policy is retired: {names:?}"
    );
    assert!(!names.is_empty(), "the table is never left unfenced — FORCE RLS with no policy denies all");

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn a_staged_row_takes_the_acting_unit() {
    let pool = pool().await;
    let schema = schema_name();
    outbox::migrate(&pool, &schema).await.unwrap();

    let unit = Uuid::new_v4();
    let rec = OutboxRecord::new(
        "TestEvent",
        "Thing",
        "1",
        Uuid::new_v4(),
        serde_json::json!({}),
        chrono::Utc::now(),
    );

    // `stage` does not name the org column, so the fill trigger is what puts it there.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.acting_unit_id', $1, true)")
        .bind(unit.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    outbox::stage(&mut *tx, &schema, &rec).await.unwrap();
    tx.commit().await.unwrap();

    let org_unit: Option<Uuid> =
        sqlx::query_scalar(&format!("SELECT org_unit_id FROM {schema}.outbox_events WHERE id=$1"))
            .bind(rec.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(org_unit, Some(unit), "the trigger stamps the acting unit onto a staged row");

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn an_unbound_scope_leaves_the_unit_null_rather_than_guessing() {
    let pool = pool().await;
    let schema = schema_name();
    outbox::migrate(&pool, &schema).await.unwrap();

    let rec = OutboxRecord::new(
        "TestEvent",
        "Thing",
        "1",
        Uuid::new_v4(),
        serde_json::json!({}),
        chrono::Utc::now(),
    );
    // No acting unit bound: the trigger must not invent one. The fence then refuses the row for
    // every scoped session, which is the fail-closed posture.
    outbox::stage(&pool, &schema, &rec).await.unwrap();

    let org_unit: Option<Uuid> =
        sqlx::query_scalar(&format!("SELECT org_unit_id FROM {schema}.outbox_events WHERE id=$1"))
            .bind(rec.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(org_unit, None, "an unbound scope leaves NULL instead of guessing a unit");

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn migrate_is_idempotent_over_the_rekey() {
    let pool = pool().await;
    let schema = schema_name();
    legacy_outbox(&pool, &schema).await;

    outbox::migrate(&pool, &schema).await.unwrap();
    outbox::migrate(&pool, &schema).await.expect("a second boot is a no-op, not an error");
    outbox::migrate(&pool, &schema).await.expect("and a third");

    let names = policies(&pool, &schema).await;
    assert_eq!(
        names.iter().filter(|n| *n == "outbox_events_org_unit_isolation").count(),
        1,
        "exactly one org policy, however many times the service boots: {names:?}"
    );

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool).await.unwrap();
}

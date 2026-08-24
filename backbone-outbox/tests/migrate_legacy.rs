//! migrate() against a table created by an OLDER version of it — every current column except
//! `company_id` (the shape a database carries when the table predates that column). On such a
//! table `CREATE TABLE IF NOT EXISTS` is a no-op, so without an in-place heal the company
//! fence's index/policy DDL aborts on the missing column and migrate() — and any boot that runs
//! it — dies. These are the crate's migrate() regression tests: the heal itself, its
//! idempotency, and the posture of rows that predate the column.
//!
//! The legacy shape mirrors what such a database actually looks like: same 11 columns, the
//! primary key, and the unpublished-tail partial index — and NO company_id column, index, or
//! RLS policy.
//!
//! Follows the same live-Postgres conventions as mechanics.rs / relay_rls.rs (run with the
//! feature on, against a scratch database):
//!
//!     DATABASE_URL=postgres://user:pass@localhost:5432/scratch \
//!     cargo test --features multi_tenant --test migrate_legacy -- --nocapture
//!
//! What it proves:
//!   - migrate() SUCCEEDS on a legacy table (previously: error `column "company_id" does not
//!     exist` while building the fence index) and heals it: column + index + policy + RLS all
//!     present, new rows stage fine, and a second migrate() is a no-op;
//!   - the healed column is NULLABLE (a NOT NULL add would fail on any populated legacy table);
//!   - rows that predate the column keep `company_id IS NULL`: invisible to every scoped app
//!     role under the fence (fail-closed), still visible to `metaphor_relay` via the policy's
//!     per-table bypass.
//!
//! Unlike relay_rls.rs this suite never logs in as the roles and never touches their passwords
//! (safe against a shared cluster): it switches to them with `SET ROLE` from the admin
//! connection, which exercises the same RLS evaluation.

#![cfg(feature = "multi_tenant")]

use backbone_outbox::{outbox, OutboxRecord};
use chrono::Utc;
use sqlx::{Acquire, PgPool};
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".into());
    PgPool::connect(&url).await.expect("connect")
}

/// A schema whose `outbox_events` has the pre-`company_id` shape: created here by hand exactly
/// as an older migrate() would have left it (columns, primary key, unpublished-tail index — no
/// company_id column, no fence).
async fn legacy_schema(pool: &PgPool) -> String {
    let s = format!("t_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {s}")).execute(pool).await.unwrap();
    sqlx::query(&format!(
        r#"CREATE TABLE {s}.outbox_events (
             id             uuid PRIMARY KEY,
             event_type     text NOT NULL,
             aggregate_type text NOT NULL,
             aggregate_id   text NOT NULL,
             payload        jsonb NOT NULL,
             occurred_at    timestamptz NOT NULL,
             correlation_id text,
             causation_id   text,
             version        int NOT NULL DEFAULT 1,
             created_at     timestamptz NOT NULL DEFAULT now(),
             published_at   timestamptz
           )"#
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE INDEX idx_{s}_outbox_unpublished
           ON {s}.outbox_events (occurred_at) WHERE published_at IS NULL"
    ))
    .execute(pool)
    .await
    .unwrap();
    s
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(pool)
        .await
        .unwrap();
}

/// (exists, nullable) for `outbox_events.company_id` in `schema`.
async fn company_id_column(pool: &PgPool, schema: &str) -> (bool, bool) {
    let row: Option<(String,)> = sqlx::query_as(&format!(
        "SELECT is_nullable FROM information_schema.columns
          WHERE table_schema='{schema}' AND table_name='outbox_events' AND column_name='company_id'"
    ))
    .fetch_optional(pool)
    .await
    .unwrap();
    match row {
        Some((n,)) => (true, n == "YES"),
        None => (false, false),
    }
}

async fn company_index_exists(pool: &PgPool, schema: &str) -> bool {
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM pg_indexes
          WHERE schemaname='{schema}' AND indexname='idx_{schema}_outbox_company_id'"
    ))
    .fetch_one(pool)
    .await
    .unwrap();
    n == 1
}

async fn fence_policy_exists(pool: &PgPool, schema: &str) -> bool {
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM pg_policies
          WHERE schemaname='{schema}' AND tablename='outbox_events'
            AND policyname='outbox_events_company_isolation'"
    ))
    .fetch_one(pool)
    .await
    .unwrap();
    n == 1
}

/// RLS enabled AND forced on the table.
async fn rls_enabled_and_forced(pool: &PgPool, schema: &str) -> bool {
    let forced: bool = sqlx::query_scalar(&format!(
        "SELECT relrowsecurity AND relforcerowsecurity FROM pg_class
          WHERE oid = '{schema}.outbox_events'::regclass"
    ))
    .fetch_one(pool)
    .await
    .unwrap();
    forced
}

fn staged_record(company: Uuid) -> OutboxRecord {
    OutboxRecord::new("TestEvent", "Payment", "PAY-1", company, serde_json::json!({"k":"v"}), Utc::now())
}

/// An empty legacy table (pre-`company_id` shape) is healed in place by migrate(): the column,
/// the fence index, the policy, and RLS all appear; new events stage fine; running migrate()
/// again changes nothing. Before the heal this died at the fence index with
/// `column "company_id" does not exist`.
#[tokio::test]
async fn legacy_table_without_company_id_is_healed_in_place() {
    let pool = pool().await;
    let schema = legacy_schema(&pool).await;

    // The heal itself — previously the abort.
    outbox::migrate(&pool, &schema).await.unwrap();

    // Column added, and NULLABLE: a NOT NULL add would be rejected on a populated legacy table.
    let (exists, nullable) = company_id_column(&pool, &schema).await;
    assert!(exists, "company_id column is added to the legacy table");
    assert!(nullable, "the healed company_id column is nullable");

    assert!(company_index_exists(&pool, &schema).await, "fence index is created");
    assert!(fence_policy_exists(&pool, &schema).await, "fence policy is created");
    assert!(rls_enabled_and_forced(&pool, &schema).await, "RLS is enabled and forced");

    // New rows (which always carry a company) stage and count as pending.
    let company = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    outbox::stage(&mut *tx, &schema, &staged_record(company)).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 1);

    // Idempotent: a second migrate() is a no-op — no error, no shape change, data intact.
    outbox::migrate(&pool, &schema).await.unwrap();
    let (exists, nullable) = company_id_column(&pool, &schema).await;
    assert!(exists && nullable, "second migrate() leaves the column as-is");
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 1, "data survives the second migrate()");

    drop_schema(&pool, &schema).await;
}

/// A legacy table that already holds events migrates without error (why the healed column must
/// be nullable). The pre-existing rows keep `company_id IS NULL`, and the fence treats them
/// fail-closed: invisible to every scoped app role — and to an unscoped one — while
/// `metaphor_relay` still sees them through the policy's per-table bypass.
#[tokio::test]
async fn populated_legacy_table_migrates_and_null_company_rows_stay_fail_closed() {
    let pool = pool().await;
    let schema = legacy_schema(&pool).await;

    // Two events staged by the pre-company era: no company_id column to fill.
    for i in 0..2 {
        sqlx::query(&format!(
            r#"INSERT INTO {schema}.outbox_events
                 (id, event_type, aggregate_type, aggregate_id, payload, occurred_at, version)
               VALUES ($1, 'LegacyEvent', 'Payment', $2, $3, now(), 1)"#
        ))
        .bind(Uuid::new_v4())
        .bind(format!("LEG-{i}"))
        .bind(serde_json::json!({"legacy": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    // The heal must not fail on a populated table.
    outbox::migrate(&pool, &schema).await.unwrap();

    // The legacy rows survived, with NULL company_id.
    let null_rows: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.outbox_events WHERE company_id IS NULL"
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(null_rows, 2, "legacy rows survive the heal with company_id NULL");

    // One new-era event with a real company, staged through the normal path.
    let owner = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    outbox::stage(&mut *tx, &schema, &staged_record(owner)).await.unwrap();
    tx.commit().await.unwrap();

    // A throwaway NOBYPASSRLS probe role (dropped at the end) + the fixed relay role, both able
    // to reach the table. No logins and no password changes — SET ROLE below evaluates the same
    // RLS paths.
    let probe = format!("outbox_fence_probe_{}", &Uuid::new_v4().simple().to_string()[..16]);
    sqlx::query(&format!(
        "CREATE ROLE {probe} NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOLOGIN"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let _ = sqlx::query(
        "CREATE ROLE metaphor_relay NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOLOGIN",
    )
    .execute(&pool)
    .await; // may already exist — only its name matters for the policy's bypass
    for role in [&probe, "metaphor_relay"] {
        sqlx::query(&format!("GRANT USAGE ON SCHEMA {schema} TO {role}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!("GRANT SELECT ON {schema}.outbox_events TO {role}"))
            .execute(&pool)
            .await
            .unwrap();
    }

    let total_unpublished: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.outbox_events WHERE published_at IS NULL"
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total_unpublished, 3, "2 legacy NULL-company rows + 1 new scoped row");

    // Scoped app role: sees ONLY its company's row — the NULL legacy rows are invisible.
    // Unscoped: sees nothing (fail-closed). All on one connection — SET ROLE is per-connection.
    let mut conn = pool.acquire().await.unwrap();
    let n_owner: i64 = {
        sqlx::query(&format!("SET ROLE {probe}")).execute(&mut *conn).await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SELECT set_config('app.company_id', $1, true)")
            .bind(owner.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        let n: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {schema}.outbox_events"
        ))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
        n
    };
    assert_eq!(n_owner, 1, "scoped app role sees its own row, not the NULL legacy rows");

    let n_other: i64 = {
        let other = Uuid::new_v4();
        sqlx::query(&format!("SET ROLE {probe}")).execute(&mut *conn).await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SELECT set_config('app.company_id', $1, true)")
            .bind(other.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {schema}.outbox_events"))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
        n
    };
    assert_eq!(n_other, 0, "scoped to another company sees nothing");

    let n_unscoped: i64 = {
        sqlx::query(&format!("SET ROLE {probe}")).execute(&mut *conn).await.unwrap();
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {schema}.outbox_events"))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
        n
    };
    assert_eq!(n_unscoped, 0, "unscoped app role sees zero rows (fail-closed)");

    // The relay role, with NO company scope, sees all unpublished rows — including the NULL
    // legacy ones — so delivering them is not blocked by the fence.
    let n_relay: i64 = {
        sqlx::query("SET ROLE metaphor_relay").execute(&mut *conn).await.unwrap();
        let n: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {schema}.outbox_events WHERE published_at IS NULL"
        ))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
        n
    };
    assert_eq!(n_relay, 3, "relay role sees every unpublished row, NULL-company ones included");
    drop(conn);

    // Idempotent on the populated shape too.
    outbox::migrate(&pool, &schema).await.unwrap();
    let null_rows: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.outbox_events WHERE company_id IS NULL"
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(null_rows, 2, "second migrate() leaves the legacy rows untouched");

    drop_schema(&pool, &schema).await;
    sqlx::query(&format!("DROP OWNED BY {probe}")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP ROLE {probe}")).execute(&pool).await.unwrap();
}

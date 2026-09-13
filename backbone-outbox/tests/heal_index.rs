//! `migrate()` retires the partial index that predates the dead-letter state.
//!
//! The predecessor's predicate was `WHERE published_at IS NULL`, which still admits a row the
//! transport gave up on. Leaving it in place would cover the drain's tail with two partial indexes,
//! one of them wrong. This builds a table the way the older `migrate()` did, heals it, and checks the
//! swap happened.

use backbone_outbox::outbox;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_outbox".into());
    PgPool::connect(&url).await.expect("connect")
}

/// Build `schema.outbox_events` as the version before the dead-letter state created it.
async fn legacy_schema(pool: &PgPool) -> String {
    // Short on purpose: a realistic schema name, well inside Postgres's 63-byte identifier cap,
    // so the index names here are the ones a real deployment gets.
    let schema = format!("li_{}", &Uuid::new_v4().simple().to_string()[..8]);
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        r#"CREATE TABLE {schema}.outbox_events (
             id uuid PRIMARY KEY, event_type text NOT NULL, aggregate_type text NOT NULL,
             aggregate_id text NOT NULL, company_id uuid NOT NULL, payload jsonb NOT NULL,
             occurred_at timestamptz NOT NULL, correlation_id text, causation_id text,
             version int NOT NULL DEFAULT 1, created_at timestamptz NOT NULL DEFAULT now(),
             published_at timestamptz)"#
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE INDEX idx_{schema}_outbox_unpublished
           ON {schema}.outbox_events (occurred_at) WHERE published_at IS NULL"
    ))
    .execute(pool)
    .await
    .unwrap();
    schema
}

async fn index_names(pool: &PgPool, schema: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relkind = 'i'",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn migrate_retires_the_old_partial_index() {
    let pool = pool().await;
    let schema = legacy_schema(&pool).await;

    let before = index_names(&pool, &schema).await;
    assert!(
        before
            .iter()
            .any(|n| n == &format!("idx_{schema}_outbox_unpublished")),
        "the fixture starts with the predecessor index: {before:?}"
    );

    outbox::migrate(&pool, &schema)
        .await
        .expect("migrate heals the table in place");

    let after = index_names(&pool, &schema).await;
    assert!(
        after
            .iter()
            .any(|n| n == &format!("idx_{schema}_outbox_pending")),
        "the pending index, which excludes dead rows, is created: {after:?}"
    );
    assert!(
        !after
            .iter()
            .any(|n| n == &format!("idx_{schema}_outbox_unpublished")),
        "the predecessor is retired: {after:?}"
    );

    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await
        .unwrap();
}

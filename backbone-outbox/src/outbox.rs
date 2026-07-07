//! The producer side: schema setup + staging an event **inside the producer's transaction**.

use sqlx::{PgExecutor, PgPool};

use crate::error::{validate_schema, Result};
use crate::record::OutboxRecord;

/// Create the `outbox_events` + `inbox_consumed` tables in `schema` (idempotent). A module is both a
/// producer (its outbox) and a consumer (its inbox), so both are created together. Safe to run on every
/// boot.
pub async fn migrate(pool: &PgPool, schema: &str) -> Result<()> {
    validate_schema(schema)?;
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema}")).execute(pool).await?;
    sqlx::query(&format!(
        r#"CREATE TABLE IF NOT EXISTS {schema}.outbox_events (
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
    .await?;
    // Partial index over just the un-drained tail — keeps the relay's poll cheap as the table grows.
    sqlx::query(&format!(
        "CREATE INDEX IF NOT EXISTS idx_{schema}_outbox_unpublished
           ON {schema}.outbox_events (occurred_at) WHERE published_at IS NULL"
    ))
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        r#"CREATE TABLE IF NOT EXISTS {schema}.inbox_consumed (
             consumer    text NOT NULL,
             event_id    uuid NOT NULL,
             consumed_at timestamptz NOT NULL DEFAULT now(),
             PRIMARY KEY (consumer, event_id)
           )"#
    ))
    .execute(pool)
    .await?;
    Ok(())
}

/// Stage an event into `schema.outbox_events`. Pass the producer's **transaction** as the executor
/// (`&mut *tx`) so the event and the state change commit atomically — no lost or phantom events.
///
/// Idempotent on the event `id` (`ON CONFLICT DO NOTHING`): a producer-level retry re-stages harmlessly.
/// Returns `true` if the row was newly staged, `false` if this id was already present.
pub async fn stage<'c, E>(executor: E, schema: &str, rec: &OutboxRecord) -> Result<bool>
where
    E: PgExecutor<'c>,
{
    validate_schema(schema)?;
    let done = sqlx::query(&format!(
        r#"INSERT INTO {schema}.outbox_events
             (id, event_type, aggregate_type, aggregate_id, payload, occurred_at,
              correlation_id, causation_id, version)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
           ON CONFLICT (id) DO NOTHING"#
    ))
    .bind(rec.id)
    .bind(&rec.event_type)
    .bind(&rec.aggregate_type)
    .bind(&rec.aggregate_id)
    .bind(&rec.payload)
    .bind(rec.occurred_at)
    .bind(&rec.correlation_id)
    .bind(&rec.causation_id)
    .bind(rec.version)
    .execute(executor)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Count of un-drained events in `schema.outbox_events` (for monitoring / tests).
pub async fn pending_count(pool: &PgPool, schema: &str) -> Result<i64> {
    validate_schema(schema)?;
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.outbox_events WHERE published_at IS NULL"
    ))
    .fetch_one(pool)
    .await?;
    Ok(n)
}

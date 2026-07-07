//! The consumer side: at-least-once delivery → exactly-once effect via a dedup table.

use sqlx::PgExecutor;

use crate::error::{validate_schema, Result};

/// Claim an event for a `consumer` exactly once. Run this **inside the consumer's transaction**, before
/// (and committed together with) the effect, so "marked consumed" and "effect applied" are atomic.
///
/// Returns `true` the first time this `(consumer, event_id)` is seen (the caller should apply the
/// effect), and `false` on any redelivery (the caller should skip — the effect already happened). This
/// is what turns the relay's at-least-once delivery into an exactly-once effect. The dedup is scoped by
/// `consumer`, so N independent consumers each process the same event once.
pub async fn once<'c, E>(executor: E, schema: &str, consumer: &str, event_id: uuid::Uuid) -> Result<bool>
where
    E: PgExecutor<'c>,
{
    validate_schema(schema)?;
    let done = sqlx::query(&format!(
        r#"INSERT INTO {schema}.inbox_consumed (consumer, event_id)
           VALUES ($1,$2) ON CONFLICT (consumer, event_id) DO NOTHING"#
    ))
    .bind(consumer)
    .bind(event_id)
    .execute(executor)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Whether a `(consumer, event_id)` has already been consumed (for monitoring / tests).
pub async fn was_consumed(
    pool: &sqlx::PgPool,
    schema: &str,
    consumer: &str,
    event_id: uuid::Uuid,
) -> Result<bool> {
    validate_schema(schema)?;
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.inbox_consumed WHERE consumer=$1 AND event_id=$2"
    ))
    .bind(consumer)
    .bind(event_id)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

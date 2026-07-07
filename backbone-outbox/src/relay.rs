//! The relay: drain unpublished outbox rows onto the bus, at-least-once.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{validate_schema, OutboxError, Result};
use crate::record::OutboxRecord;

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: Uuid,
    event_type: String,
    aggregate_type: String,
    aggregate_id: String,
    payload: serde_json::Value,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
    causation_id: Option<String>,
    version: i32,
}
impl OutboxRow {
    fn into_record(self) -> OutboxRecord {
        OutboxRecord {
            id: self.id,
            event_type: self.event_type,
            aggregate_type: self.aggregate_type,
            aggregate_id: self.aggregate_id,
            payload: self.payload,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            version: self.version,
        }
    }
}

/// Drain up to `batch` un-published events from `schema.outbox_events`, hand each to `publish`, and mark
/// the successfully-published ones. Returns the number published this pass.
///
/// **The relay never holds a transaction across `publish`.** The batch is read with a short-lived
/// connection that is returned before any `publish` runs; each record is then published while holding
/// *no* relay connection, and marked with an idempotent per-row UPDATE (`… WHERE id=$1 AND published_at
/// IS NULL`). This is deliberate: the shipped consumers reborrow the *same* pool inside `publish` (their
/// own tx), so a relay that held its connection + the outbox row locks across `publish` would need two
/// pool connections per in-flight event and self-deadlock on a bounded pool (maturity council
/// 2026-07-07). By not spanning `publish`, each DB step borrows and returns a connection independently —
/// safe even at `max_connections = 1`.
///
/// **At-least-once**: `published_at` is set only after `publish` returns `Ok`; a crash between publish
/// and mark redelivers the row (the consumer's inbox dedups it). Concurrent relay workers are not row-
/// locked, so they may double-*deliver* a row — harmless (the inbox makes the *effect* exactly-once);
/// a lease-based claim to trim duplicate deliveries is a future optimization. A `publish` error leaves
/// the row for the next pass.
pub async fn drain_once<F, Fut>(pool: &PgPool, schema: &str, batch: i64, publish: F) -> Result<usize>
where
    F: Fn(OutboxRecord) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), OutboxError>>,
{
    validate_schema(schema)?;
    // Short-lived read: borrows a connection only for the SELECT, then returns it to the pool.
    let rows: Vec<OutboxRow> = sqlx::query_as(&format!(
        r#"SELECT id, event_type, aggregate_type, aggregate_id, payload, occurred_at,
                  correlation_id, causation_id, version
           FROM {schema}.outbox_events
           WHERE published_at IS NULL
           ORDER BY occurred_at, id
           LIMIT $1"#
    ))
    .bind(batch)
    .fetch_all(pool)
    .await?;

    let mut published = 0usize;
    for row in rows {
        let rec = row.into_record();
        let id = rec.id;
        match publish(rec).await {
            // No relay connection is held here — the consumer may freely reborrow the pool.
            Ok(()) => {
                sqlx::query(&format!(
                    "UPDATE {schema}.outbox_events SET published_at=now() WHERE id=$1 AND published_at IS NULL"
                ))
                .bind(id)
                .execute(pool)
                .await?;
                published += 1;
            }
            // Leave the row un-published; the next pass retries it.
            Err(OutboxError::Publish(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(published)
}

/// Drain repeatedly until the outbox is empty (bounded by `max_passes`). Convenience for tests and
/// one-shot flushes; a production relay loops `drain_once` on a `backbone-jobs` schedule instead.
pub async fn drain_all<F, Fut>(
    pool: &PgPool,
    schema: &str,
    batch: i64,
    max_passes: usize,
    publish: F,
) -> Result<usize>
where
    F: Fn(OutboxRecord) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), OutboxError>>,
{
    let mut total = 0;
    for _ in 0..max_passes {
        let n = drain_once(pool, schema, batch, &publish).await?;
        total += n;
        if n == 0 {
            break;
        }
    }
    Ok(total)
}

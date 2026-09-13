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
    company_id: Uuid,
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
            company_id: self.company_id,
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
        r#"SELECT id, event_type, aggregate_type, aggregate_id, company_id, payload, occurred_at,
                  correlation_id, causation_id, version
           FROM {schema}.outbox_events
           WHERE published_at IS NULL AND failed_at IS NULL
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
            // The transport gave up on this record. Marking it published would report a delivery
            // that never happened and erase the only trace that work was dropped; leaving it pending
            // would re-hand the same doomed record on every pass forever. Mark it dead instead:
            // visible in the table, excluded from the drain, and re-emittable once the cause is fixed.
            Err(OutboxError::Exhausted(reason)) => {
                sqlx::query(&format!(
                    "UPDATE {schema}.outbox_events SET failed_at=now(), failure_reason=$2
                     WHERE id=$1 AND published_at IS NULL"
                ))
                .bind(id)
                .bind(&reason)
                .execute(pool)
                .await?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(published)
}

/// A record the transport gave up on, as an operator sees it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeadRecord {
    /// The outbox row id — pass it to [`reemit`].
    pub id: Uuid,
    /// The event that was never delivered.
    pub event_type: String,
    /// What it was about.
    pub aggregate_type: String,
    /// Which one.
    pub aggregate_id: String,
    /// When the transport gave up.
    pub failed_at: DateTime<Utc>,
    /// Why, as the transport reported it.
    pub failure_reason: Option<String>,
}

/// List the records the transport gave up on, oldest first.
///
/// These are cross-module effects that did not happen. Nothing retries them on its own — that is the
/// point of a dead letter — so an operator reads this, fixes the cause, and calls [`reemit`].
pub async fn list_dead(pool: &PgPool, schema: &str, limit: i64) -> Result<Vec<DeadRecord>> {
    validate_schema(schema)?;
    let rows = sqlx::query_as::<_, DeadRecord>(&format!(
        r#"SELECT id, event_type, aggregate_type, aggregate_id, failed_at, failure_reason
           FROM {schema}.outbox_events
           WHERE failed_at IS NOT NULL
           ORDER BY failed_at, id
           LIMIT $1"#
    ))
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Hand a dead record back to the relay. Returns whether a row was revived.
///
/// Clears the dead mark so the next drain offers the record to the transport again. Deliberately
/// manual: an event is dead because delivery kept failing, and re-emitting before the cause is fixed
/// just re-kills it. A row that was published in the meantime is left alone.
pub async fn reemit(pool: &PgPool, schema: &str, id: Uuid) -> Result<bool> {
    validate_schema(schema)?;
    let done = sqlx::query(&format!(
        "UPDATE {schema}.outbox_events SET failed_at=NULL, failure_reason=NULL
         WHERE id=$1 AND failed_at IS NOT NULL AND published_at IS NULL"
    ))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Hand every dead record back to the relay. Returns how many were revived.
///
/// For the case where one shared cause killed a batch — the consumer was misconfigured, a downstream
/// service was down — and the fix clears all of them at once.
pub async fn reemit_all(pool: &PgPool, schema: &str) -> Result<u64> {
    validate_schema(schema)?;
    let done = sqlx::query(&format!(
        "UPDATE {schema}.outbox_events SET failed_at=NULL, failure_reason=NULL
         WHERE failed_at IS NOT NULL AND published_at IS NULL"
    ))
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
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

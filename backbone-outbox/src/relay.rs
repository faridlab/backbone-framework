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
/// the successfully-published ones. **At-least-once**: a locked row is marked `published_at` only if
/// `publish` returned `Ok`; if the relay crashes after `publish` succeeds but before the mark commits,
/// the row is redelivered on the next drain (the consumer's inbox dedups it). Rows are locked
/// `FOR UPDATE SKIP LOCKED`, so multiple relay workers scale without contending. Returns the number
/// published this pass.
///
/// `publish` is the transport seam — wire it to the in-proc `backbone-messaging` bus today, or a broker
/// later, without touching producers or consumers. A `publish` error leaves that row for the next pass.
pub async fn drain_once<F, Fut>(pool: &PgPool, schema: &str, batch: i64, publish: F) -> Result<usize>
where
    F: Fn(OutboxRecord) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), OutboxError>>,
{
    validate_schema(schema)?;
    let mut tx = pool.begin().await?;
    let rows: Vec<OutboxRow> = sqlx::query_as(&format!(
        r#"SELECT id, event_type, aggregate_type, aggregate_id, payload, occurred_at,
                  correlation_id, causation_id, version
           FROM {schema}.outbox_events
           WHERE published_at IS NULL
           ORDER BY occurred_at, id
           LIMIT $1
           FOR UPDATE SKIP LOCKED"#
    ))
    .bind(batch)
    .fetch_all(&mut *tx)
    .await?;

    let mut published = 0usize;
    for row in rows {
        let rec = row.into_record();
        let id = rec.id;
        match publish(rec).await {
            Ok(()) => {
                sqlx::query(&format!(
                    "UPDATE {schema}.outbox_events SET published_at=now() WHERE id=$1"
                ))
                .bind(id)
                .execute(&mut *tx)
                .await?;
                published += 1;
            }
            // Leave the row un-published; the next pass retries it.
            Err(OutboxError::Publish(_)) => {}
            Err(e) => return Err(e),
        }
    }
    tx.commit().await?;
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

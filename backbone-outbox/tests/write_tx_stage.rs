//! Staging on the write transaction of the current task: the event commits or rolls back with the
//! write that raised it, and carries the correlation and causation ids it was given.
//!
//! Like the other outbox suites it connects to `DATABASE_URL` and creates its own schema.

use backbone_orm::in_write_tx;
use backbone_outbox::{outbox, OutboxError, OutboxRecord};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_outbox".into());
    PgPool::connect(&url).await.expect("connect")
}

async fn fresh_schema(pool: &PgPool) -> String {
    let s = format!("t_{}", Uuid::new_v4().simple());
    outbox::migrate(pool, &s).await.unwrap();
    s
}

fn rec(kind: &str) -> OutboxRecord {
    OutboxRecord::new(
        kind,
        "Ticket",
        "t-1",
        Uuid::new_v4(),
        serde_json::json!({"k": kind}),
        Utc::now(),
    )
    .with_correlation_id("corr-1")
    .with_causation_id("cause-1")
    .with_version(3)
}

#[tokio::test]
async fn a_staged_event_commits_with_the_write_transaction_and_keeps_its_ids() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    let record = rec("TicketOpened");

    let staged: Result<bool, OutboxError> = in_write_tx(&pool, async {
        let staged = outbox::stage_in_write_tx(&schema, &record).await?;
        // Staged on the transaction: not visible to another connection yet.
        assert_eq!(outbox::pending_count(&pool, &schema).await?, 0);
        Ok(staged)
    })
    .await;
    assert!(staged.unwrap());
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 1);

    let (corr, cause, version): (Option<String>, Option<String>, i32) = sqlx::query_as(&format!(
        "SELECT correlation_id, causation_id, version FROM {schema}.outbox_events WHERE id = $1"
    ))
    .bind(record.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(corr.as_deref(), Some("corr-1"));
    assert_eq!(cause.as_deref(), Some("cause-1"));
    assert_eq!(version, 3);
}

#[tokio::test]
async fn a_failure_after_staging_leaves_no_event() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;

    let outcome: Result<(), OutboxError> = in_write_tx(&pool, async {
        outbox::stage_in_write_tx(&schema, &rec("TicketOpened")).await?;
        Err(OutboxError::Publish("a later step failed".into()))
    })
    .await;
    assert!(outcome.is_err());
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 0);
}

#[tokio::test]
async fn staging_outside_a_write_transaction_is_refused() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    let err = outbox::stage_in_write_tx(&schema, &rec("TicketOpened"))
        .await
        .unwrap_err();
    assert!(matches!(err, OutboxError::Db(_)), "{err}");
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 0);
}

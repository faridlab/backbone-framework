//! Mechanics of the durable outbox/inbox/relay on real Postgres:
//! stage-in-tx atomicity, idempotent stage, relay at-least-once, inbox exactly-once dedup, and the
//! end-to-end crash-window: a producer commits state + stages an event atomically; a later relay
//! delivers it; a REDELIVERY is deduped at the consumer's inbox so a settlement-shaped draw happens once.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use backbone_outbox::{inbox, outbox, relay, OutboxError, OutboxRecord};
use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_outbox".into());
    PgPool::connect(&url).await.expect("connect")
}
/// Each test gets its own schema so they don't collide.
async fn fresh_schema(pool: &PgPool) -> String {
    let s = format!("t_{}", Uuid::new_v4().simple());
    outbox::migrate(pool, &s).await.unwrap();
    s
}
fn rec(kind: &str, agg: &str) -> OutboxRecord {
    OutboxRecord::new(kind, "Payment", agg, Uuid::new_v4(), serde_json::json!({"k": kind}), Utc::now())
}
/// A no-op publish sink (success).
async fn ok_publish(_r: OutboxRecord) -> Result<(), OutboxError> {
    Ok(())
}

/// M1 — stage is atomic with the producer's tx: a rollback leaves no event; a commit persists it.
#[tokio::test]
async fn m1_stage_atomic_with_tx() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;

    // Rollback → nothing staged.
    let mut tx = pool.begin().await.unwrap();
    outbox::stage(&mut *tx, &schema, &rec("A", "1")).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 0, "rolled-back event is not staged");

    // Commit → staged.
    let mut tx = pool.begin().await.unwrap();
    outbox::stage(&mut *tx, &schema, &rec("A", "1")).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 1, "committed event is staged");
}

/// M2 — stage is idempotent on the event id: re-staging the same id is a no-op (producer retry safe).
#[tokio::test]
async fn m2_stage_idempotent() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    let r = rec("A", "1");

    let first = outbox::stage(&pool, &schema, &r).await.unwrap();
    let second = outbox::stage(&pool, &schema, &r).await.unwrap();
    assert!(first, "first stage inserts");
    assert!(!second, "re-stage of the same id is a no-op");
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 1);
}

/// M3 — the relay drains at-least-once and marks published; a failing publish leaves the row for retry.
#[tokio::test]
async fn m3_relay_drains_and_retries() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    for i in 0..3 {
        outbox::stage(&pool, &schema, &rec("A", &i.to_string())).await.unwrap();
    }

    // First pass: publish FAILS → nothing marked, all 3 still pending.
    let n = relay::drain_once(&pool, &schema, 100, |_r| async {
        Err(OutboxError::Publish("bus down".into()))
    }).await.unwrap();
    assert_eq!(n, 0, "failed publish marks nothing");
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 3, "rows stay for retry");

    // Second pass: publish succeeds → all 3 delivered + marked.
    let delivered = Arc::new(AtomicUsize::new(0));
    let d = delivered.clone();
    let n = relay::drain_once(&pool, &schema, 100, move |_r| {
        let d = d.clone();
        async move { d.fetch_add(1, Ordering::SeqCst); Ok(()) }
    }).await.unwrap();
    assert_eq!(n, 3);
    assert_eq!(delivered.load(Ordering::SeqCst), 3);
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 0, "all published");

    // Third pass: nothing left.
    assert_eq!(relay::drain_once(&pool, &schema, 100, ok_publish).await.unwrap(), 0);
}

/// M4 — inbox `once` dedups per consumer: true the first time, false on redelivery; independent
/// consumers each get their own first-time.
#[tokio::test]
async fn m4_inbox_once_dedups_per_consumer() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    let id = Uuid::new_v4();

    let mut tx = pool.begin().await.unwrap();
    assert!(inbox::once(&mut *tx, &schema, "billing", id).await.unwrap(), "first delivery processes");
    assert!(!inbox::once(&mut *tx, &schema, "billing", id).await.unwrap(), "redelivery is skipped");
    // A different consumer still gets its own first-time for the same event.
    assert!(inbox::once(&mut *tx, &schema, "analytics", id).await.unwrap(), "independent consumer processes once");
    tx.commit().await.unwrap();
}

/// M6 (maturity council 2026-07-07) — the relay must NOT hold a connection across `publish`. A consumer
/// that reborrows the SAME pool inside `publish` (as the shipped rollout does) would need two pool
/// connections per in-flight event; the old relay held its tx + row locks across publish and
/// self-deadlocked on a bounded pool. On a `max_connections = 1` pool with a short acquire timeout, a
/// drain whose consumer does `pool.begin()` must SUCCEED (not block/timeout).
#[tokio::test]
async fn m6_relay_does_not_deadlock_a_reborrowing_consumer() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_outbox".into());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect(&url)
        .await
        .expect("connect");
    let schema = fresh_schema(&pool).await;
    sqlx::query(&format!("CREATE TABLE {schema}.marks (id text PRIMARY KEY)")).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    outbox::stage(&mut *tx, &schema, &rec("A", "1")).await.unwrap();
    tx.commit().await.unwrap();

    // The consumer reborrows the SAME single-connection pool inside publish (the shipped shape).
    let cpool = pool.clone();
    let cschema = schema.clone();
    let published = relay::drain_once(&pool, &schema, 100, move |r: OutboxRecord| {
        let (pool, schema) = (cpool.clone(), cschema.clone());
        async move {
            let mut tx = pool.begin().await.map_err(OutboxError::from)?; // <- would deadlock if relay held the conn
            sqlx::query(&format!("INSERT INTO {schema}.marks VALUES ($1) ON CONFLICT DO NOTHING"))
                .bind(r.id.to_string()).execute(&mut *tx).await.map_err(OutboxError::from)?;
            tx.commit().await.map_err(OutboxError::from)?;
            Ok(())
        }
    }).await.unwrap();
    assert_eq!(published, 1, "the reborrowing consumer ran without a pool deadlock");
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 0, "and the row was marked published");
}

/// M5 — END-TO-END crash-window exactly-once (settlement-shaped): a producer commits state + stages the
/// event atomically; the relay (running LATER, after a simulated crash) delivers it; the event is then
/// REDELIVERED; the consumer's inbox dedups so the invoice is drawn down exactly once.
#[tokio::test]
async fn m5_crash_window_exactly_once_draw() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    // A settlement-shaped consumer state: one invoice with an outstanding balance.
    sqlx::query(&format!(
        "CREATE TABLE {schema}.invoices (id text PRIMARY KEY, outstanding double precision NOT NULL)"
    )).execute(&pool).await.unwrap();
    sqlx::query(&format!("INSERT INTO {schema}.invoices VALUES ('INV-1', 100)"))
        .execute(&pool).await.unwrap();

    // PRODUCER: settle a payment — commit the payment's state AND stage PaymentSettled atomically.
    // (Here the "state" is trivial; the point is stage() rides the same tx, so a crash right after
    // commit cannot lose the event — it is already durable in the outbox.)
    let event = OutboxRecord::new(
        "PaymentSettled", "Payment", "PAY-1", Uuid::new_v4(),
        serde_json::json!({"invoice": "INV-1", "amount": "40"}), Utc::now());
    let mut tx = pool.begin().await.unwrap();
    outbox::stage(&mut *tx, &schema, &event).await.unwrap();
    tx.commit().await.unwrap();
    // <-- simulate a crash HERE: the old code would have published after commit and lost the event.
    //     With the outbox it is durably staged; the relay picks it up whenever it next runs.

    // CONSUMER: applies a settlement draw, deduped + committed atomically via the inbox.
    let draw = |pool: PgPool, schema: String| {
        move |r: OutboxRecord| {
            let (pool, schema) = (pool.clone(), schema.clone());
            async move {
                let mut tx = pool.begin().await.map_err(OutboxError::from)?;
                // Dedup + effect in ONE tx.
                if inbox::once(&mut *tx, &schema, "billing", r.id).await? {
                    let amount: f64 = r.payload["amount"].as_str().unwrap().parse().unwrap();
                    sqlx::query(&format!(
                        "UPDATE {schema}.invoices SET outstanding = outstanding - $1 WHERE id=$2"))
                        .bind(amount).bind(r.payload["invoice"].as_str().unwrap())
                        .execute(&mut *tx).await.map_err(OutboxError::from)?;
                }
                tx.commit().await.map_err(OutboxError::from)?;
                Ok(())
            }
        }
    };

    // Relay runs (post-crash) and delivers the event → invoice drawn 100 → 60.
    relay::drain_once(&pool, &schema, 100, draw(pool.clone(), schema.clone())).await.unwrap();
    let outstanding: f64 = sqlx::query_scalar(&format!("SELECT outstanding FROM {schema}.invoices WHERE id='INV-1'"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(outstanding, 60.0, "settlement applied once");

    // REDELIVERY: the same event is handed to the consumer again (at-least-once). The inbox dedups it →
    // the invoice is NOT drawn a second time.
    draw(pool.clone(), schema.clone())(event.clone()).await.unwrap();
    let outstanding: f64 = sqlx::query_scalar(&format!("SELECT outstanding FROM {schema}.invoices WHERE id='INV-1'"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(outstanding, 60.0, "redelivery did NOT double-draw (exactly-once effect)");
}

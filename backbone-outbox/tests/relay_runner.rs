//! Composition test for the relay RUNNER (feature = "runner"): a producer stages events; the runner
//! loop drains them onto the "bus" (here, a consumer handler that dedups + applies, settlement-shaped);
//! a forced redelivery is deduped; graceful shutdown stops the loop. Proves the standalone relay worker
//! delivers at-least-once and the consumer makes it exactly-once — end-to-end, over real Postgres.
#![cfg(feature = "runner")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use backbone_outbox::runner::{run, RelayConfig};
use backbone_outbox::{inbox, outbox, OutboxError, OutboxRecord};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_outbox".into());
    PgPool::connect(&url).await.expect("connect")
}
async fn fresh_schema(pool: &PgPool) -> String {
    let s = format!("r_{}", Uuid::new_v4().simple());
    outbox::migrate(pool, &s).await.unwrap();
    s
}

/// R1 — the runner drains staged events to the consumer (exactly-once), survives a forced redelivery,
/// and stops on shutdown.
#[tokio::test]
async fn runner_delivers_then_stops() {
    let pool = pool().await;
    let schema = fresh_schema(&pool).await;
    // settlement-shaped consumer state
    sqlx::query(&format!("CREATE TABLE {schema}.invoices (id text PRIMARY KEY, outstanding double precision NOT NULL)"))
        .execute(&pool).await.unwrap();
    sqlx::query(&format!("INSERT INTO {schema}.invoices VALUES ('INV-1', 100), ('INV-2', 50)"))
        .execute(&pool).await.unwrap();

    // The "bus → consumer handler": dedup at the inbox, then draw — exactly-once. Counts deliveries seen.
    let deliveries = Arc::new(AtomicUsize::new(0));
    let (hpool, hschema, hcount) = (pool.clone(), schema.clone(), deliveries.clone());
    let publish = move |rec: OutboxRecord| {
        let (pool, schema, count) = (hpool.clone(), hschema.clone(), hcount.clone());
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            let mut tx = pool.begin().await.map_err(OutboxError::from)?;
            if inbox::once(&mut *tx, &schema, "drawer", rec.id).await? {
                let inv = rec.payload["invoice"].as_str().unwrap();
                let amt: f64 = rec.payload["amount"].as_str().unwrap().parse().unwrap();
                sqlx::query(&format!("UPDATE {schema}.invoices SET outstanding = outstanding - $1 WHERE id=$2"))
                    .bind(amt).bind(inv).execute(&mut *tx).await.map_err(OutboxError::from)?;
            }
            tx.commit().await.map_err(OutboxError::from)?;
            Ok(())
        }
    };

    // Start the runner (fast poll) with a shutdown signal.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let cfg = RelayConfig { schema: schema.clone(), batch: 10, poll_interval: Duration::from_millis(20) };
    let runner_pool = pool.clone();
    let handle = tokio::spawn(async move {
        run(runner_pool, cfg, publish, async { let _ = shutdown_rx.await; }).await
    });

    // Producer stages two settlement events (in-tx).
    for (inv, amt, ev) in [("INV-1", "40", "e1"), ("INV-2", "20", "e2")] {
        let rec = OutboxRecord::new("Settled", "Payment", ev,
            serde_json::json!({"invoice": inv, "amount": amt}), Utc::now());
        let mut tx = pool.begin().await.unwrap();
        outbox::stage(&mut *tx, &schema, &rec).await.unwrap();
        tx.commit().await.unwrap();
    }

    // Wait for the runner to drain the outbox (bounded).
    for _ in 0..100 {
        if outbox::pending_count(&pool, &schema).await.unwrap() == 0 { break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(outbox::pending_count(&pool, &schema).await.unwrap(), 0, "runner drained the outbox");
    let out1: f64 = sqlx::query_scalar(&format!("SELECT outstanding FROM {schema}.invoices WHERE id='INV-1'")).fetch_one(&pool).await.unwrap();
    let out2: f64 = sqlx::query_scalar(&format!("SELECT outstanding FROM {schema}.invoices WHERE id='INV-2'")).fetch_one(&pool).await.unwrap();
    assert_eq!((out1, out2), (60.0, 30.0), "both settlements applied once");

    // Force a redelivery of e1 (as if the relay crashed after publish before marking).
    sqlx::query(&format!("UPDATE {schema}.outbox_events SET published_at=NULL WHERE aggregate_id='e1'"))
        .execute(&pool).await.unwrap();
    for _ in 0..100 {
        if outbox::pending_count(&pool, &schema).await.unwrap() == 0 { break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let out1_again: f64 = sqlx::query_scalar(&format!("SELECT outstanding FROM {schema}.invoices WHERE id='INV-1'")).fetch_one(&pool).await.unwrap();
    assert_eq!(out1_again, 60.0, "redelivery deduped — no double draw");
    assert!(deliveries.load(Ordering::SeqCst) >= 3, "the runner re-handed the redelivered event");

    // Graceful shutdown: the loop returns Ok.
    shutdown_tx.send(()).unwrap();
    let res = tokio::time::timeout(Duration::from_secs(2), handle).await.expect("runner joined").unwrap();
    assert!(res.is_ok(), "runner exited cleanly on shutdown");
}

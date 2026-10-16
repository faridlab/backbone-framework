//! The relay RUNNER — a poll loop that drains the outbox on an interval until shutdown.
//!
//! Feature-gated (`runner`) so the outbox/inbox primitives stay tokio-free. A composition/producer
//! service starts one runner per producer schema; on a `backbone-jobs` deployment the same
//! [`relay::drain_once`] is instead invoked on a scheduled job — this loop is the standalone equivalent.

use std::future::Future;
use std::time::Duration;

use sqlx::PgPool;

use crate::error::{OutboxError, Result};
use crate::record::OutboxRecord;
use crate::relay;

/// How the runner paces itself.
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// The producer schema whose `outbox_events` to drain (e.g. `"payment"`).
    pub schema: String,
    /// Max rows delivered per pass.
    pub batch: i64,
    /// Wait between passes when the outbox was empty (or under-full).
    pub poll_interval: Duration,
}

impl RelayConfig {
    /// A sane default: drain 100 at a time, poll every second.
    pub fn new(schema: impl Into<String>) -> Self {
        Self { schema: schema.into(), batch: 100, poll_interval: Duration::from_secs(1) }
    }
}

/// Run the relay loop until `shutdown` resolves, then return. Each pass drains up to `batch` events via
/// [`relay::drain_once`]; if it delivered a **full** batch there may be more, so it polls again
/// immediately, otherwise it waits `poll_interval` (interruptible by `shutdown`). A drain error
/// (non-publish, e.g. the DB is down) is returned and stops the loop — the supervisor restarts it.
///
/// `publish` is the transport seam (wire it to the in-proc `backbone-messaging` bus or a broker); a
/// per-record publish failure leaves that row for the next pass (at-least-once), it does not stop the
/// loop.
pub async fn run<F, Fut, S>(pool: PgPool, cfg: RelayConfig, publish: F, shutdown: S) -> Result<()>
where
    F: Fn(OutboxRecord) -> Fut,
    Fut: Future<Output = std::result::Result<(), OutboxError>>,
    S: Future<Output = ()>,
{
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => return Ok(()),
            res = relay::drain_once(&pool, &cfg.schema, cfg.batch, &publish) => {
                let delivered = res?;
                if (delivered as i64) < cfg.batch {
                    // Under-full pass → nothing more waiting; back off (but wake on shutdown).
                    tokio::select! {
                        biased;
                        _ = &mut shutdown => return Ok(()),
                        _ = tokio::time::sleep(cfg.poll_interval) => {}
                    }
                }
                // A full batch → loop immediately; there may be more to drain.
            }
        }
    }
}

/// The longest wait between restarts of a failed relay.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// [`run`] under supervision, until `shutdown` resolves. A drain error (a database outage, a missing
/// relay grant on a newly composed schema) used to end the runner's task for good, with no line in
/// the log, while events kept staging behind it. Supervised, each error is logged with its schema and
/// the relay restarts after a wait that doubles from one second up to a minute, resetting once a run
/// has lasted that long. `publish` is cloned for each restart.
pub async fn supervise<F, Fut, S>(pool: PgPool, cfg: RelayConfig, publish: F, shutdown: S)
where
    F: Fn(OutboxRecord) -> Fut + Clone,
    Fut: Future<Output = std::result::Result<(), OutboxError>>,
    S: Future<Output = ()>,
{
    let schema = cfg.schema.clone();
    restart_loop(
        &schema,
        || run(pool.clone(), cfg.clone(), publish.clone(), std::future::pending::<()>()),
        shutdown,
    )
    .await
}

/// Run `attempt` again after each failure, waiting longer each time, until `shutdown` resolves or an
/// attempt ends cleanly.
async fn restart_loop<A, AFut, S>(schema: &str, mut attempt: A, shutdown: S)
where
    A: FnMut() -> AFut,
    AFut: Future<Output = Result<()>>,
    S: Future<Output = ()>,
{
    tokio::pin!(shutdown);
    let mut backoff = Duration::from_secs(1);
    loop {
        let started = tokio::time::Instant::now();
        tokio::select! {
            biased;
            _ = &mut shutdown => return,
            res = attempt() => match res {
                Ok(()) => return,
                Err(e) => {
                    if started.elapsed() >= MAX_BACKOFF {
                        backoff = Duration::from_secs(1);
                    }
                    tracing::error!(schema, error = %e, retry_in_secs = backoff.as_secs(), "outbox relay stopped; restarting");
                }
            },
        }
        tokio::select! {
            biased;
            _ = &mut shutdown => return,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod supervise_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A relay that fails is restarted, with a doubling wait, until an attempt ends cleanly.
    #[tokio::test(start_paused = true)]
    async fn a_failed_relay_restarts_with_backoff_until_it_runs() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let started = tokio::time::Instant::now();
        restart_loop(
            "payment",
            move || {
                let n = seen.fetch_add(1, Ordering::SeqCst);
                async move { if n < 3 { Err(OutboxError::Publish("db down".into())) } else { Ok(()) } }
            },
            std::future::pending::<()>(),
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 4, "three failures, then a clean run");
        // Waits of 1 + 2 + 4 seconds between the four attempts.
        assert_eq!(started.elapsed().as_secs(), 7);
    }

    /// Shutdown ends supervision even while a failed relay is waiting to restart.
    #[tokio::test(start_paused = true)]
    async fn shutdown_ends_supervision_while_it_waits() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        restart_loop(
            "payment",
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async { Err(OutboxError::Publish("db down".into())) }
            },
            tokio::time::sleep(Duration::from_millis(1500)),
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "the first wait ends, the second is cut short");
    }
}

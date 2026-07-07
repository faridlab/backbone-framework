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

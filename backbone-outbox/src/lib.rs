//! # backbone-outbox
//!
//! Durable **transactional-outbox + relay + inbox** primitives that make cross-module event delivery
//! go-live safe over Postgres — without a broker. It composes the in-process `backbone-messaging` bus;
//! it does not replace it.
//!
//! The three moving parts (see `docs/erp/event-bus-contract.md`):
//!
//! 1. **Outbox** — [`outbox::stage`] writes a serialized event into `<schema>.outbox_events` **inside
//!    the producer's own transaction**, so the state change and the event commit atomically. No event is
//!    lost on a crash between commit and publish, and none is emitted for a rolled-back change.
//! 2. **Relay** — [`relay::drain_once`] drains un-published rows (`FOR UPDATE SKIP LOCKED`) onto a
//!    caller-supplied `publish` sink (the in-proc bus today, a broker later) and marks them published.
//!    **At-least-once** delivery.
//! 3. **Inbox** — [`inbox::once`] dedups a `(consumer, event_id)` in the consumer's own transaction, so
//!    at-least-once delivery becomes an **exactly-once effect**.
//!
//! `backbone-outbox` is framework plumbing: it depends on `sqlx` + `serde` only, never on a domain
//! module or the bus. The relay's transport is a closure the caller wires.
//!
//! ```no_run
//! # async fn ex(pool: &sqlx::PgPool) -> Result<(), backbone_outbox::OutboxError> {
//! use backbone_outbox::{outbox, inbox, relay, OutboxRecord};
//! use chrono::Utc;
//!
//! // Producer: stage in the same tx as the state change.
//! let mut tx = pool.begin().await?;
//! // ... mutate state on &mut *tx ...
//! let rec = OutboxRecord::new("PaymentSettled", "Payment", "pay-1",
//!     serde_json::json!({"invoice": "INV-1", "amount": "100.00"}), Utc::now());
//! outbox::stage(&mut *tx, "payment", &rec).await?;
//! tx.commit().await?;
//!
//! // Relay: drain onto the bus (here: a no-op sink).
//! relay::drain_once(pool, "payment", 100, |_rec: OutboxRecord| async { Ok(()) }).await?;
//!
//! // Consumer: dedup then apply, atomically.
//! let mut ctx = pool.begin().await?;
//! if inbox::once(&mut *ctx, "billing", "billing-settler", rec.id).await? {
//!     // ... apply_settlement on &mut *ctx ...
//! }
//! ctx.commit().await?;
//! # Ok(()) }
//! ```

pub mod error;
pub mod inbox;
pub mod outbox;
pub mod record;
pub mod relay;

pub use error::{OutboxError, Result};
pub use record::OutboxRecord;

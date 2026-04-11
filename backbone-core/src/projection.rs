//! Projector base trait for CQRS read models.
//!
//! Phase 0 generic base for the `projection.rs` generator (Category C).
//!
//! Each generated `{Name}Projector` struct applies domain events to its
//! projection (read model). The structural contract — applying an event and
//! rebuilding from scratch — is identical for all entities. This trait
//! captures that contract so generated projectors can implement rather than
//! independently describe it.
//!
//! ## Generated output (after Phase 1):
//! ```rust,ignore
//! use backbone_core::projection::Projector;
//!
//! #[async_trait]
//! impl<R: OrderProjectionRepository> Projector<Order, OrderEvent> for OrderProjector<R> {
//!     async fn project(&self, event: OrderEvent, sequence: i64) -> anyhow::Result<()> {
//!         // dispatch to on_created / on_updated / on_deleted
//!     }
//!     async fn rebuild(&self) -> anyhow::Result<u64> {
//!         self.repository.rebuild_all().await
//!     }
//! }
//! ```

use async_trait::async_trait;

/// Applies domain events to a CQRS read model (projection).
///
/// Type parameters:
/// - `E`   — entity type the projection is built from
/// - `Evt` — domain event type (e.g. `OrderEvent` / `CrudEvent<Order>`)
///
/// The entity-specific fields of the projection struct, the event handler
/// methods, and the repository trait remain in the generated module.
#[async_trait]
pub trait Projector<E, Evt>: Send + Sync {
    /// Apply a single domain event to update the read model.
    ///
    /// `sequence` is the monotonically-increasing position of the event in
    /// the event store, used for ordering and idempotency tracking.
    async fn project(&self, event: Evt, sequence: i64) -> anyhow::Result<()>;

    /// Rebuild all projections from the full event history.
    ///
    /// Returns the number of projections rebuilt.
    async fn rebuild(&self) -> anyhow::Result<u64>;
}

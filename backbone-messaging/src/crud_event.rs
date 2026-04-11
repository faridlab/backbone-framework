//! CRUD domain events — generic event types for all entity operations.
//!
//! Every generated entity gets a type alias over these generics:
//!
//! ```rust,ignore
//! // Generated (not hand-written):
//! pub type StoredFileCrudEvent = CrudEvent<StoredFile>;
//! pub type StoredFileCrudEventPublisher = Arc<dyn CrudEventPublisher<StoredFile>>;
//! ```
//!
//! Custom services can inject `Arc<dyn CrudEventPublisher<E>>` directly
//! or use `NoOpCrudEventPublisher<E>` when no external subscribers exist.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

use crate::error::EventError;
use crate::event::DomainEvent;

// ─── EventMetadata ────────────────────────────────────────────────────────────

/// Infrastructure metadata attached to every CRUD event.
///
/// Carries correlation tracking, timestamps, and entity identification
/// without polluting the entity type itself.
#[derive(Debug, Clone)]
pub struct EventMetadata {
    /// Unique ID for this event instance (UUID v4).
    pub event_id: String,
    /// When the event was created (UTC).
    pub timestamp: DateTime<Utc>,
    /// Correlation ID for distributed tracing across services.
    pub correlation_id: Option<String>,
    /// The static entity type name (e.g. `"order"`, `"stored_file"`).
    pub entity_type: &'static str,
    /// The entity's aggregate root ID at the time of the event.
    pub aggregate_id: String,
}

impl EventMetadata {
    /// Construct metadata for an event on `aggregate_id` of type `entity_type`.
    pub fn new(aggregate_id: impl Into<String>, entity_type: &'static str) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            correlation_id: None,
            entity_type,
            aggregate_id: aggregate_id.into(),
        }
    }

    /// Attach a correlation ID (builder).
    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

// ─── CrudEvent ────────────────────────────────────────────────────────────────

/// The standard set of CRUD domain events for any entity type `E`.
///
/// All variants carry the full entity snapshot so subscribers never
/// need to fetch back to the database.
#[derive(Debug, Clone)]
pub enum CrudEvent<E: Clone + Send + Sync + 'static> {
    /// Entity was created for the first time.
    Created { entity: E, metadata: EventMetadata },
    /// Entity fields were updated (full replacement).
    Updated { before: E, after: E, metadata: EventMetadata },
    /// Entity was partially patched.
    Patched { before: E, after: E, metadata: EventMetadata },
    /// Entity was soft-deleted (moved to trash).
    SoftDeleted { entity: E, metadata: EventMetadata },
    /// Soft-deleted entity was restored.
    Restored { entity: E, metadata: EventMetadata },
    /// Entity was permanently deleted.
    HardDeleted { entity_id: String, metadata: EventMetadata },
    /// Bulk create completed.
    BulkCreated { entities: Vec<E>, metadata: EventMetadata },
}

impl<E: Clone + Send + Sync + 'static> CrudEvent<E> {
    /// Access the event metadata from any variant.
    pub fn metadata(&self) -> &EventMetadata {
        match self {
            CrudEvent::Created { metadata, .. } => metadata,
            CrudEvent::Updated { metadata, .. } => metadata,
            CrudEvent::Patched { metadata, .. } => metadata,
            CrudEvent::SoftDeleted { metadata, .. } => metadata,
            CrudEvent::Restored { metadata, .. } => metadata,
            CrudEvent::HardDeleted { metadata, .. } => metadata,
            CrudEvent::BulkCreated { metadata, .. } => metadata,
        }
    }
}

// ─── DomainEvent impl ─────────────────────────────────────────────────────────

impl<E: Clone + Send + Sync + 'static> DomainEvent for CrudEvent<E> {
    fn event_type(&self) -> &'static str {
        match self {
            CrudEvent::Created { .. } => "entity.created",
            CrudEvent::Updated { .. } => "entity.updated",
            CrudEvent::Patched { .. } => "entity.patched",
            CrudEvent::SoftDeleted { .. } => "entity.soft_deleted",
            CrudEvent::Restored { .. } => "entity.restored",
            CrudEvent::HardDeleted { .. } => "entity.hard_deleted",
            CrudEvent::BulkCreated { .. } => "entity.bulk_created",
        }
    }

    fn aggregate_id(&self) -> &str {
        &self.metadata().aggregate_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.metadata().timestamp
    }

    fn aggregate_type(&self) -> &'static str {
        self.metadata().entity_type
    }
}

// ─── CrudEventPublisher ───────────────────────────────────────────────────────

/// Contract for publishing `CrudEvent<E>` instances.
///
/// Implemented by:
/// - `NoOpCrudEventPublisher<E>` — in tests and modules without subscribers
/// - Real bus adapters (Kafka, in-memory, etc.) provided by infrastructure
///
/// Services always require `Arc<dyn CrudEventPublisher<E>>` — never `Option<...>`.
#[async_trait]
pub trait CrudEventPublisher<E: Clone + Send + Sync + 'static>: Send + Sync {
    async fn publish(&self, event: CrudEvent<E>) -> Result<(), EventError>;

    async fn publish_many(&self, events: Vec<CrudEvent<E>>) -> Result<(), EventError> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }

    /// Convenience method: publish a Created event.
    async fn publish_created(&self, entity: E, _user_id: Option<String>) -> Result<(), EventError> {
        let meta = EventMetadata::new("", std::any::type_name::<E>());
        self.publish(CrudEvent::Created { entity, metadata: meta }).await
    }

    /// Convenience method: publish an Updated event.
    async fn publish_updated(&self, entity: E, _user_id: Option<String>) -> Result<(), EventError> {
        let meta = EventMetadata::new("", std::any::type_name::<E>());
        self.publish(CrudEvent::Updated { before: entity.clone(), after: entity, metadata: meta }).await
    }

    /// Convenience method: publish a soft-deleted (or hard-deleted) event.
    async fn publish_deleted(&self, entity_id: String, _user_id: Option<String>) -> Result<(), EventError> {
        let meta = EventMetadata::new(entity_id.clone(), std::any::type_name::<E>());
        self.publish(CrudEvent::HardDeleted { entity_id, metadata: meta }).await
    }
}

/// No-op implementation — discards all events.  Default for services
/// that don't need cross-module integration.
pub struct NoOpCrudEventPublisher<E: Clone + Send + Sync + 'static> {
    _phantom: std::marker::PhantomData<E>,
}

impl<E: Clone + Send + Sync + 'static> NoOpCrudEventPublisher<E> {
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }

    /// Convenience constructor that returns the publisher wrapped in an `Arc`,
    /// ready to inject into a service.
    pub fn arc() -> Arc<dyn CrudEventPublisher<E>> {
        Arc::new(Self::new())
    }
}

impl<E: Clone + Send + Sync + 'static> Default for NoOpCrudEventPublisher<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: Clone + Send + Sync + 'static> CrudEventPublisher<E> for NoOpCrudEventPublisher<E> {
    async fn publish(&self, _event: CrudEvent<E>) -> Result<(), EventError> {
        Ok(())
    }

    async fn publish_many(&self, _events: Vec<CrudEvent<E>>) -> Result<(), EventError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct FakeEntity {
        id: String,
    }

    fn meta(id: &str) -> EventMetadata {
        EventMetadata::new(id.to_string(), "fake_entity")
    }

    #[tokio::test]
    async fn noop_publisher_returns_ok_for_all_variants() {
        let publisher = NoOpCrudEventPublisher::<FakeEntity>::new();

        let events = vec![
            CrudEvent::Created {
                entity: FakeEntity { id: "1".into() },
                metadata: meta("1"),
            },
            CrudEvent::SoftDeleted {
                entity: FakeEntity { id: "1".into() },
                metadata: meta("1"),
            },
            CrudEvent::HardDeleted {
                entity_id: "1".into(),
                metadata: meta("1"),
            },
        ];

        for event in events {
            assert!(publisher.publish(event).await.is_ok());
        }
    }

    #[test]
    fn event_type_names_are_stable() {
        let created: CrudEvent<FakeEntity> = CrudEvent::Created {
            entity: FakeEntity { id: "1".into() },
            metadata: meta("1"),
        };
        assert_eq!(created.event_type(), "entity.created");

        let deleted: CrudEvent<FakeEntity> = CrudEvent::HardDeleted {
            entity_id: "1".into(),
            metadata: meta("1"),
        };
        assert_eq!(deleted.event_type(), "entity.hard_deleted");
    }

    #[test]
    fn aggregate_id_roundtrips() {
        let event: CrudEvent<FakeEntity> = CrudEvent::Created {
            entity: FakeEntity { id: "abc".into() },
            metadata: meta("abc"),
        };
        assert_eq!(event.aggregate_id(), "abc");
    }

    #[test]
    fn metadata_correlation_id() {
        let m = EventMetadata::new("e1", "entity").with_correlation("corr-42");
        assert_eq!(m.correlation_id.as_deref(), Some("corr-42"));
        assert_eq!(m.aggregate_id, "e1");
        assert_eq!(m.entity_type, "entity");
    }
}

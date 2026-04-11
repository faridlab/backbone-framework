//! DDD Aggregate Root Pattern
//!
//! Provides traits for implementing Aggregate Roots in Domain-Driven Design.
//! Aggregate Roots are the entry point to a cluster of domain objects
//! and are responsible for maintaining invariants.
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::{AggregateRoot, PersistentEntity};
//!
//! #[derive(Clone)]
//! pub struct Order {
//!     id: String,
//!     items: Vec<OrderItem>,
//!     status: OrderStatus,
//!     events: Vec<OrderEvent>,
//!     // ... other fields
//! }
//!
//! impl AggregateRoot for Order {
//!     type Event = OrderEvent;
//!
//!     fn uncommitted_events(&self) -> &[Self::Event] {
//!         &self.events
//!     }
//!
//!     fn clear_events(&mut self) {
//!         self.events.clear();
//!     }
//!
//!     fn apply_event(&mut self, event: Self::Event) {
//!         match event {
//!             OrderEvent::ItemAdded { item } => self.items.push(item),
//!             OrderEvent::Confirmed => self.status = OrderStatus::Confirmed,
//!             // ...
//!         }
//!         self.events.push(event);
//!     }
//! }
//! ```

use crate::persistence::PersistentEntity;

/// DDD Aggregate Root trait.
///
/// Aggregate Roots are characterized by:
/// - Being the only entry point to the aggregate
/// - Maintaining consistency boundaries
/// - Publishing domain events for state changes
/// - Having a unique identity (via `PersistentEntity`)
pub trait AggregateRoot: PersistentEntity {
    /// The type of domain events this aggregate can produce.
    type Event: Send + Sync + Clone;

    /// Get all uncommitted domain events.
    ///
    /// These events should be persisted/published after the aggregate
    /// is saved to the repository.
    fn uncommitted_events(&self) -> &[Self::Event];

    /// Clear all uncommitted events.
    ///
    /// This should be called after events have been persisted/published.
    fn clear_events(&mut self);

    /// Apply a domain event to update the aggregate state.
    ///
    /// This method should:
    /// 1. Update the aggregate's internal state based on the event
    /// 2. Store the event in the uncommitted events list
    fn apply_event(&mut self, event: Self::Event);

    /// Get the aggregate version for optimistic concurrency.
    ///
    /// Returns `None` if versioning is not supported.
    fn version(&self) -> Option<u64> {
        None
    }

    /// Increment the aggregate version.
    ///
    /// Called after successful persistence.
    fn increment_version(&mut self) {
        // Default implementation does nothing
    }

    /// Check if there are any uncommitted events.
    fn has_uncommitted_events(&self) -> bool {
        !self.uncommitted_events().is_empty()
    }

    /// Get the number of uncommitted events.
    fn uncommitted_event_count(&self) -> usize {
        self.uncommitted_events().len()
    }
}

/// Extension trait for aggregates that support event sourcing.
pub trait EventSourcedAggregate: AggregateRoot {
    /// Reconstruct the aggregate from a stream of events.
    fn from_events(id: String, events: impl IntoIterator<Item = Self::Event>) -> Self
    where
        Self: Sized;

    /// Get all events (including committed ones) for event sourcing.
    fn all_events(&self) -> Vec<Self::Event>;
}

/// Trait for aggregates that enforce invariants.
pub trait InvariantAggregate: AggregateRoot {
    /// Error type for invariant violations.
    type InvariantError: std::error::Error + Send + Sync;

    /// Check all aggregate invariants.
    ///
    /// This should be called before persisting the aggregate.
    fn check_invariants(&self) -> Result<(), Self::InvariantError>;
}

/// Helper struct for tracking aggregate metadata.
#[derive(Debug, Clone, Default)]
pub struct AggregateMetadata {
    /// Current version of the aggregate.
    pub version: u64,
    /// Timestamp of last modification.
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    /// ID of the user who last modified the aggregate.
    pub last_modified_by: Option<String>,
}

impl AggregateMetadata {
    /// Create new metadata with version 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create metadata with a specific version.
    pub fn with_version(version: u64) -> Self {
        Self {
            version,
            ..Default::default()
        }
    }

    /// Increment the version.
    pub fn increment(&mut self) {
        self.version += 1;
        self.last_modified = Some(chrono::Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestEvent {
        data: String,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestAggregate {
        id: String,
        data: String,
        events: Vec<TestEvent>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        deleted_at: Option<DateTime<Utc>>,
    }

    impl PersistentEntity for TestAggregate {
        fn entity_id(&self) -> String {
            self.id.clone()
        }

        fn set_entity_id(&mut self, id: String) {
            self.id = id;
        }

        fn created_at(&self) -> DateTime<Utc> {
            self.created_at
        }

        fn set_created_at(&mut self, at: DateTime<Utc>) {
            self.created_at = at;
        }

        fn updated_at(&self) -> DateTime<Utc> {
            self.updated_at
        }

        fn set_updated_at(&mut self, at: DateTime<Utc>) {
            self.updated_at = at;
        }

        fn deleted_at(&self) -> Option<DateTime<Utc>> {
            self.deleted_at
        }

        fn set_deleted_at(&mut self, at: Option<DateTime<Utc>>) {
            self.deleted_at = at;
        }
    }

    impl AggregateRoot for TestAggregate {
        type Event = TestEvent;

        fn uncommitted_events(&self) -> &[Self::Event] {
            &self.events
        }

        fn clear_events(&mut self) {
            self.events.clear();
        }

        fn apply_event(&mut self, event: Self::Event) {
            self.data = event.data.clone();
            self.events.push(event);
        }
    }

    #[test]
    fn test_aggregate_events() {
        let mut aggregate = TestAggregate {
            id: "1".to_string(),
            data: "initial".to_string(),
            events: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        };

        assert!(!aggregate.has_uncommitted_events());

        aggregate.apply_event(TestEvent {
            data: "updated".to_string(),
        });

        assert!(aggregate.has_uncommitted_events());
        assert_eq!(aggregate.uncommitted_event_count(), 1);
        assert_eq!(aggregate.data, "updated");

        aggregate.clear_events();
        assert!(!aggregate.has_uncommitted_events());
    }

    #[test]
    fn test_aggregate_metadata() {
        let mut metadata = AggregateMetadata::new();
        assert_eq!(metadata.version, 0);

        metadata.increment();
        assert_eq!(metadata.version, 1);
        assert!(metadata.last_modified.is_some());
    }
}

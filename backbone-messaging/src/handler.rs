//! Event Handler trait definition

use async_trait::async_trait;

use crate::{DomainEvent, EventEnvelope, EventError};

/// Trait for event handlers
///
/// Event handlers process domain events asynchronously. They can be used
/// for side effects like sending notifications, updating read models,
/// or triggering workflows.
///
/// # Example
///
/// ```rust,ignore
/// use backbone_messaging::{EventHandler, EventEnvelope, EventError, DomainEvent};
/// use async_trait::async_trait;
///
/// struct EmailNotificationHandler;
///
/// #[async_trait]
/// impl<E: DomainEvent> EventHandler<E> for EmailNotificationHandler {
///     async fn handle(&self, envelope: EventEnvelope<E>) -> Result<(), EventError> {
///         // Send email notification
///         println!("Event {} occurred for {}", envelope.event_type, envelope.aggregate_id);
///         Ok(())
///     }
///
///     fn event_types(&self) -> Vec<&'static str> {
///         vec!["UserCreated", "OrderPlaced"]
///     }
/// }
/// ```
#[async_trait]
pub trait EventHandler<E: DomainEvent>: Send + Sync {
    /// Handle a domain event
    ///
    /// This method is called when an event matching one of the types
    /// returned by `event_types()` is published.
    async fn handle(&self, envelope: EventEnvelope<E>) -> Result<(), EventError>;

    /// Event types this handler is interested in
    ///
    /// Return an empty slice to receive all events.
    fn event_types(&self) -> Vec<&'static str>;

    /// Handler name for logging and debugging
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Whether this handler should be retried on failure
    fn should_retry(&self) -> bool {
        true
    }

    /// Maximum retry attempts
    fn max_retries(&self) -> u32 {
        3
    }
}

/// A handler that logs all events for debugging
pub struct LoggingHandler {
    event_types: Vec<&'static str>,
}

impl LoggingHandler {
    /// Create a handler that logs specific event types
    pub fn new(event_types: Vec<&'static str>) -> Self {
        Self { event_types }
    }

    /// Create a handler that logs all events
    pub fn all() -> Self {
        Self { event_types: vec![] }
    }
}

#[async_trait]
impl<E: DomainEvent> EventHandler<E> for LoggingHandler {
    async fn handle(&self, envelope: EventEnvelope<E>) -> Result<(), EventError> {
        tracing::info!(
            event_type = %envelope.event_type,
            aggregate_id = %envelope.aggregate_id,
            event_id = %envelope.id,
            correlation_id = ?envelope.correlation_id,
            "Domain event received"
        );
        Ok(())
    }

    fn event_types(&self) -> Vec<&'static str> {
        self.event_types.clone()
    }

    fn name(&self) -> &'static str {
        "LoggingHandler"
    }
}

/// A handler that collects events for testing
#[derive(Default)]
pub struct CollectingHandler<E: DomainEvent> {
    events: std::sync::Arc<tokio::sync::RwLock<Vec<EventEnvelope<E>>>>,
}

impl<E: DomainEvent> CollectingHandler<E> {
    /// Create a new collecting handler
    pub fn new() -> Self {
        Self {
            events: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Get all collected events
    pub async fn events(&self) -> Vec<EventEnvelope<E>> {
        self.events.read().await.clone()
    }

    /// Clear collected events
    pub async fn clear(&self) {
        self.events.write().await.clear();
    }

    /// Get event count
    pub async fn count(&self) -> usize {
        self.events.read().await.len()
    }
}

#[async_trait]
impl<E: DomainEvent> EventHandler<E> for CollectingHandler<E> {
    async fn handle(&self, envelope: EventEnvelope<E>) -> Result<(), EventError> {
        self.events.write().await.push(envelope);
        Ok(())
    }

    fn event_types(&self) -> Vec<&'static str> {
        vec![] // Collect all events
    }

    fn name(&self) -> &'static str {
        "CollectingHandler"
    }
}

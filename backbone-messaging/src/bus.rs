//! Event Bus implementation

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use chrono::{DateTime, Utc};

use crate::{DomainEvent, EventEnvelope, EventError, EventHandler};

/// Type alias for handler map
type HandlerMap<E> = Arc<RwLock<HashMap<String, Vec<Arc<dyn EventHandler<E>>>>>>;

/// Configuration for the event bus
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Maximum number of events to buffer in the broadcast channel
    pub buffer_size: usize,
    /// Enable event persistence for replay
    pub persist_events: bool,
    /// Event retention period in seconds (for persistence)
    pub retention_seconds: u64,
    /// Maximum events to keep in history
    pub max_history_size: usize,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            persist_events: false,
            retention_seconds: 86400, // 24 hours
            max_history_size: 10000,
        }
    }
}

impl EventBusConfig {
    /// Create config with persistence enabled
    pub fn with_persistence() -> Self {
        Self {
            persist_events: true,
            ..Default::default()
        }
    }

    /// Set buffer size
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Set retention period
    pub fn retention_seconds(mut self, seconds: u64) -> Self {
        self.retention_seconds = seconds;
        self
    }
}

/// Generic event bus for publishing and subscribing to domain events
///
/// The EventBus provides:
/// - Type-safe publish/subscribe
/// - Event envelope with metadata
/// - Handler registration by event type
/// - Optional event history for replay
///
/// # Type Parameters
///
/// - `E`: The domain event type (must implement `DomainEvent`)
///
/// # Example
///
/// ```rust,ignore
/// use backbone_messaging::{EventBus, EventBusConfig, DomainEvent};
///
/// #[derive(Clone, Debug)]
/// struct UserCreated { user_id: String }
///
/// impl DomainEvent for UserCreated {
///     fn event_type(&self) -> &'static str { "UserCreated" }
///     fn aggregate_id(&self) -> &str { &self.user_id }
/// }
///
/// let bus = EventBus::<UserCreated>::new();
/// bus.publish(UserCreated { user_id: "123".into() }).await?;
/// ```
pub struct EventBus<E: DomainEvent> {
    /// Broadcast channel sender
    sender: broadcast::Sender<EventEnvelope<E>>,
    /// Configuration
    config: EventBusConfig,
    /// Registered handlers by event type
    handlers: HandlerMap<E>,
    /// Event history (if persistence enabled)
    history: Arc<RwLock<Vec<EventEnvelope<E>>>>,
}

impl<E: DomainEvent> EventBus<E> {
    /// Create a new event bus with default configuration
    pub fn new() -> Self {
        Self::with_config(EventBusConfig::default())
    }

    /// Create a new event bus with custom configuration
    pub fn with_config(config: EventBusConfig) -> Self {
        let (sender, _) = broadcast::channel(config.buffer_size);
        Self {
            sender,
            config,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Publish a domain event
    pub async fn publish(&self, event: E) -> Result<(), EventError> {
        let envelope = EventEnvelope::new(event);
        self.publish_envelope(envelope).await
    }

    /// Publish multiple domain events
    pub async fn publish_all(&self, events: Vec<E>) -> Result<(), EventError> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }

    /// Publish an event envelope (with metadata)
    pub async fn publish_envelope(&self, envelope: EventEnvelope<E>) -> Result<(), EventError> {
        // Store in history if persistence enabled
        if self.config.persist_events {
            self.store_event(&envelope).await;
        }

        // Broadcast to all subscribers
        let _ = self.sender.send(envelope.clone());

        // Dispatch to registered handlers
        self.dispatch(envelope).await?;

        Ok(())
    }

    /// Register an event handler
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler<E>>) {
        let event_types = handler.event_types();
        let mut handlers = self.handlers.write().await;

        if event_types.is_empty() {
            // Handler wants all events - register under wildcard
            handlers
                .entry("*".to_string())
                .or_default()
                .push(Arc::clone(&handler));
        } else {
            for event_type in event_types {
                handlers
                    .entry(event_type.to_string())
                    .or_default()
                    .push(Arc::clone(&handler));
            }
        }
    }

    /// Subscribe to all events (returns a broadcast receiver)
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope<E>> {
        self.sender.subscribe()
    }

    /// Get event history (if persistence enabled)
    pub async fn history(&self) -> Vec<EventEnvelope<E>> {
        self.history.read().await.clone()
    }

    /// Get events for a specific aggregate
    pub async fn events_for_aggregate(&self, aggregate_id: &str) -> Vec<EventEnvelope<E>> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id)
            .cloned()
            .collect()
    }

    /// Get events by type
    pub async fn events_by_type(&self, event_type: &str) -> Vec<EventEnvelope<E>> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Get events in a time range
    pub async fn events_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<EventEnvelope<E>> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.occurred_at >= start && e.occurred_at <= end)
            .cloned()
            .collect()
    }

    /// Clear event history
    pub async fn clear_history(&self) {
        self.history.write().await.clear();
    }

    /// Get handler count
    pub async fn handler_count(&self) -> usize {
        self.handlers
            .read()
            .await
            .values()
            .map(|v| v.len())
            .sum()
    }

    // ========================================
    // Private Methods
    // ========================================

    async fn store_event(&self, envelope: &EventEnvelope<E>) {
        let mut history = self.history.write().await;
        history.push(envelope.clone());

        // Trim old events by retention period
        let cutoff = Utc::now() - chrono::Duration::seconds(self.config.retention_seconds as i64);
        history.retain(|e| e.published_at > cutoff);

        // Also trim by max size
        while history.len() > self.config.max_history_size {
            history.remove(0);
        }
    }

    async fn dispatch(&self, envelope: EventEnvelope<E>) -> Result<(), EventError> {
        let handlers = self.handlers.read().await;

        // Get handlers for this specific event type
        let mut handlers_to_call = Vec::new();

        if let Some(type_handlers) = handlers.get(envelope.event_type) {
            handlers_to_call.extend(type_handlers.iter().cloned());
        }

        // Get wildcard handlers (registered for all events)
        if let Some(wildcard_handlers) = handlers.get("*") {
            handlers_to_call.extend(wildcard_handlers.iter().cloned());
        }

        // Call all handlers
        for handler in handlers_to_call {
            if let Err(e) = handler.handle(envelope.clone()).await {
                tracing::error!(
                    handler = %handler.name(),
                    event_type = %envelope.event_type,
                    error = ?e,
                    "Event handler error"
                );
                // Continue dispatching to other handlers
            }
        }

        Ok(())
    }
}

impl<E: DomainEvent> Default for EventBus<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: DomainEvent> Clone for EventBus<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            config: self.config.clone(),
            handlers: Arc::clone(&self.handlers),
            history: Arc::clone(&self.history),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::CollectingHandler;

    #[derive(Clone, Debug)]
    struct TestEvent {
        id: String,
        message: String,
    }

    impl DomainEvent for TestEvent {
        fn event_type(&self) -> &'static str {
            "TestEvent"
        }

        fn aggregate_id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn test_event_bus_publish() {
        let bus = EventBus::<TestEvent>::with_config(EventBusConfig::with_persistence());

        let event = TestEvent {
            id: "test-123".to_string(),
            message: "Hello".to_string(),
        };

        bus.publish(event).await.unwrap();

        let history = bus.history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_type, "TestEvent");
    }

    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let bus = EventBus::<TestEvent>::new();
        let mut rx = bus.subscribe();

        let event = TestEvent {
            id: "test-123".to_string(),
            message: "Hello".to_string(),
        };

        bus.publish(event).await.unwrap();

        let envelope = rx.recv().await.unwrap();
        assert_eq!(envelope.event_type, "TestEvent");
        assert_eq!(envelope.aggregate_id, "test-123");
    }

    #[tokio::test]
    async fn test_event_bus_handler() {
        let bus = EventBus::<TestEvent>::new();
        let handler = Arc::new(CollectingHandler::<TestEvent>::new());

        bus.register_handler(handler.clone()).await;

        let event = TestEvent {
            id: "test-123".to_string(),
            message: "Hello".to_string(),
        };

        bus.publish(event).await.unwrap();

        // Give handler time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(handler.count().await, 1);
    }

    #[tokio::test]
    async fn test_events_for_aggregate() {
        let bus = EventBus::<TestEvent>::with_config(EventBusConfig::with_persistence());

        bus.publish(TestEvent {
            id: "agg-1".to_string(),
            message: "First".to_string(),
        })
        .await
        .unwrap();

        bus.publish(TestEvent {
            id: "agg-1".to_string(),
            message: "Second".to_string(),
        })
        .await
        .unwrap();

        bus.publish(TestEvent {
            id: "agg-2".to_string(),
            message: "Other".to_string(),
        })
        .await
        .unwrap();

        let events = bus.events_for_aggregate("agg-1").await;
        assert_eq!(events.len(), 2);
    }
}

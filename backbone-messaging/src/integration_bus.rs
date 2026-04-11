//! Integration Event Bus - Central event dispatcher for cross-module communication
//!
//! The `IntegrationEventBus` provides a type-erased event bus for publishing and
//! subscribing to integration events across bounded contexts.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────────────┐     ┌─────────────┐
//! │   Sapiens   │────▶│ IntegrationEventBus │────▶│   Postman   │
//! │   Module    │     │  (Type-erased)      │     │   Module    │
//! └─────────────┘     └─────────────────────┘     └─────────────┘
//!                              │
//!                              ▼
//!                     ┌─────────────┐
//!                     │  Bucket   │
//!                     │   Module    │
//!                     └─────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use backbone_messaging::{IntegrationEventBus, IntegrationEventHandler};
//!
//! // Create the bus
//! let bus = IntegrationEventBus::new();
//!
//! // Register a handler for user events
//! bus.register_handler(Arc::new(UserEventHandler)).await;
//!
//! // Publish an event
//! bus.publish(UserCreatedIntegrationEvent { ... }).await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::integration::{IntegrationEvent, IntegrationEventEnvelope};
use crate::EventError;

/// Handler for integration events (type-erased)
///
/// Implement this trait to create handlers that react to integration events
/// from other bounded contexts.
///
/// Type alias for the handler map to reduce type complexity
type HandlerMap = HashMap<String, Vec<Arc<dyn IntegrationEventHandler>>>;
type HandlerMapRef = Arc<RwLock<HandlerMap>>;
///
/// # Pattern Matching
///
/// Handlers specify which events they're interested in via `event_patterns()`.
/// Patterns support:
/// - Exact match: `"sapiens.user.created"`
/// - Wildcard suffix: `"sapiens.user.*"` matches all user events
/// - Global wildcard: `"*"` matches all events
///
/// # Example
///
/// ```rust,ignore
/// struct EmailNotificationHandler {
///     email_service: Arc<EmailService>,
/// }
///
/// #[async_trait]
/// impl IntegrationEventHandler for EmailNotificationHandler {
///     async fn handle(&self, envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
///         match envelope.event_type.as_str() {
///             "sapiens.user.created" => {
///                 // Deserialize and send welcome email
///                 let event: UserCreatedEvent = envelope.deserialize()?;
///                 self.email_service.send_welcome(&event.email).await?;
///             }
///             _ => {}
///         }
///         Ok(())
///     }
///
///     fn event_patterns(&self) -> Vec<&'static str> {
///         vec!["sapiens.user.*"]
///     }
///
///     fn name(&self) -> &'static str {
///         "EmailNotificationHandler"
///     }
/// }
/// ```
#[async_trait]
pub trait IntegrationEventHandler: Send + Sync {
    /// Handle an integration event envelope
    ///
    /// Called when an event matching one of the patterns from `event_patterns()`
    /// is published to the bus.
    async fn handle(&self, envelope: IntegrationEventEnvelope) -> Result<(), EventError>;

    /// Event patterns this handler is interested in
    ///
    /// Patterns support wildcards:
    /// - `"sapiens.user.created"` - exact match
    /// - `"sapiens.user.*"` - matches all sapiens.user.* events
    /// - `"*"` - matches all events
    fn event_patterns(&self) -> Vec<&'static str>;

    /// Handler name for logging and debugging
    fn name(&self) -> &'static str;

    /// Whether this handler should be retried on failure
    fn should_retry(&self) -> bool {
        true
    }

    /// Maximum retry attempts
    fn max_retries(&self) -> u32 {
        3
    }
}

/// Configuration for the integration event bus
#[derive(Clone, Debug)]
pub struct IntegrationBusConfig {
    /// Maximum number of events to buffer in the broadcast channel
    pub buffer_size: usize,
    /// Enable event persistence for replay/audit
    pub persist_events: bool,
    /// Maximum events to keep in history
    pub max_history_size: usize,
    /// Enable dead letter queue for failed handlers
    pub enable_dead_letter_queue: bool,
}

impl Default for IntegrationBusConfig {
    fn default() -> Self {
        Self {
            buffer_size: 10000,
            persist_events: true,
            max_history_size: 100000,
            enable_dead_letter_queue: true,
        }
    }
}

impl IntegrationBusConfig {
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

    /// Set max history size
    pub fn max_history_size(mut self, size: usize) -> Self {
        self.max_history_size = size;
        self
    }
}

/// Dead letter entry for failed event handling
#[derive(Clone, Debug)]
pub struct DeadLetterEntry {
    /// The envelope that failed
    pub envelope: IntegrationEventEnvelope,
    /// Handler that failed
    pub handler_name: String,
    /// Error message
    pub error: String,
    /// Number of retry attempts
    pub retry_count: u32,
    /// When the failure occurred
    pub failed_at: chrono::DateTime<chrono::Utc>,
}

/// Central event bus for integration events across all bounded contexts
///
/// The `IntegrationEventBus` is the heart of cross-module communication.
/// It receives events from publishers and dispatches them to registered handlers
/// based on pattern matching.
///
/// # Features
///
/// - **Type-erased**: Events are serialized to JSON, allowing loose coupling
/// - **Pattern matching**: Handlers subscribe to event patterns with wildcards
/// - **Persistence**: Optional event history for replay/audit
/// - **Dead letter queue**: Failed events are captured for debugging
///
/// # Thread Safety
///
/// The bus is fully thread-safe and can be cloned and shared across tasks.
pub struct IntegrationEventBus {
    /// Broadcast channel sender
    sender: broadcast::Sender<IntegrationEventEnvelope>,
    /// Registered handlers by pattern
    handlers: HandlerMapRef,
    /// Event history (if persistence enabled)
    history: Arc<RwLock<Vec<IntegrationEventEnvelope>>>,
    /// Dead letter queue for failed handlers
    dead_letter_queue: Arc<RwLock<Vec<DeadLetterEntry>>>,
    /// Configuration
    config: IntegrationBusConfig,
}

impl IntegrationEventBus {
    /// Create a new integration event bus with default configuration
    pub fn new() -> Self {
        Self::with_config(IntegrationBusConfig::default())
    }

    /// Create a new integration event bus with custom configuration
    pub fn with_config(config: IntegrationBusConfig) -> Self {
        let (sender, _) = broadcast::channel(config.buffer_size);
        Self {
            sender,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            dead_letter_queue: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }

    /// Publish an integration event
    ///
    /// The event is serialized to JSON and wrapped in an envelope.
    /// It's then broadcast to all subscribers and dispatched to matching handlers.
    ///
    /// # Errors
    ///
    /// Returns `EventError::SerializationError` if the event cannot be serialized.
    pub async fn publish<E: IntegrationEvent>(&self, event: E) -> Result<(), EventError> {
        let envelope = IntegrationEventEnvelope::from_event(&event)
            .map_err(|e| EventError::SerializationError(e.to_string()))?;
        self.publish_envelope(envelope).await
    }

    /// Publish a pre-built envelope
    ///
    /// Use this when you already have an envelope (e.g., forwarding from another bus).
    pub async fn publish_envelope(&self, envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
        debug!(
            event_type = %envelope.event_type,
            source = %envelope.source_context,
            aggregate_id = %envelope.aggregate_id,
            "Publishing integration event"
        );

        // Store in history if persistence enabled
        if self.config.persist_events {
            self.store_event(&envelope).await;
        }

        // Broadcast to subscribers
        let _ = self.sender.send(envelope.clone());

        // Dispatch to handlers
        self.dispatch(envelope).await
    }

    /// Register an integration event handler
    ///
    /// The handler will be called for events matching any of its patterns.
    pub async fn register_handler(&self, handler: Arc<dyn IntegrationEventHandler>) {
        let patterns = handler.event_patterns();
        let handler_name = handler.name();
        let mut handlers = self.handlers.write().await;

        for pattern in patterns {
            info!(
                handler = %handler_name,
                pattern = %pattern,
                "Registering integration event handler"
            );
            handlers
                .entry(pattern.to_string())
                .or_default()
                .push(Arc::clone(&handler));
        }
    }

    /// Subscribe to all integration events (returns a broadcast receiver)
    ///
    /// Use this for monitoring, logging, or custom event processing.
    pub fn subscribe(&self) -> broadcast::Receiver<IntegrationEventEnvelope> {
        self.sender.subscribe()
    }

    /// Get event history (if persistence enabled)
    pub async fn history(&self) -> Vec<IntegrationEventEnvelope> {
        self.history.read().await.clone()
    }

    /// Get events by type pattern
    pub async fn events_by_pattern(&self, pattern: &str) -> Vec<IntegrationEventEnvelope> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.matches_pattern(pattern))
            .cloned()
            .collect()
    }

    /// Get events for a specific aggregate
    pub async fn events_for_aggregate(&self, aggregate_id: &str) -> Vec<IntegrationEventEnvelope> {
        self.history
            .read()
            .await
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id)
            .cloned()
            .collect()
    }

    /// Get dead letter queue entries
    pub async fn dead_letters(&self) -> Vec<DeadLetterEntry> {
        self.dead_letter_queue.read().await.clone()
    }

    /// Clear dead letter queue
    pub async fn clear_dead_letters(&self) {
        self.dead_letter_queue.write().await.clear();
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

    /// Get registered patterns
    pub async fn registered_patterns(&self) -> Vec<String> {
        self.handlers.read().await.keys().cloned().collect()
    }

    // ========================================
    // Private Methods
    // ========================================

    async fn store_event(&self, envelope: &IntegrationEventEnvelope) {
        let mut history = self.history.write().await;
        history.push(envelope.clone());

        // Trim by max size
        while history.len() > self.config.max_history_size {
            history.remove(0);
        }
    }

    async fn dispatch(&self, envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
        let handlers = self.handlers.read().await;
        let mut handlers_to_call = Vec::new();

        // Find handlers matching the event type
        for (pattern, pattern_handlers) in handlers.iter() {
            if Self::matches_pattern(pattern, &envelope.event_type) {
                handlers_to_call.extend(pattern_handlers.iter().cloned());
            }
        }

        drop(handlers); // Release lock before calling handlers

        // Deduplicate handlers (same handler might match multiple patterns)
        let mut seen = std::collections::HashSet::new();
        handlers_to_call.retain(|h| seen.insert(h.name()));

        debug!(
            event_type = %envelope.event_type,
            handler_count = handlers_to_call.len(),
            "Dispatching integration event"
        );

        // Call all matching handlers
        for handler in handlers_to_call {
            if let Err(e) = self.call_handler_with_retry(&handler, &envelope).await {
                error!(
                    handler = %handler.name(),
                    event_type = %envelope.event_type,
                    error = ?e,
                    "Integration event handler failed after retries"
                );

                // Add to dead letter queue
                if self.config.enable_dead_letter_queue {
                    self.add_to_dead_letter(&envelope, handler.name(), &e.to_string()).await;
                }
            }
        }

        Ok(())
    }

    async fn call_handler_with_retry(
        &self,
        handler: &Arc<dyn IntegrationEventHandler>,
        envelope: &IntegrationEventEnvelope,
    ) -> Result<(), EventError> {
        let max_retries = if handler.should_retry() {
            handler.max_retries()
        } else {
            1
        };

        let mut last_error = None;

        for attempt in 0..max_retries {
            match handler.handle(envelope.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if attempt < max_retries - 1 {
                        warn!(
                            handler = %handler.name(),
                            attempt = attempt + 1,
                            max_retries = max_retries,
                            error = ?e,
                            "Handler failed, retrying"
                        );
                        // Simple backoff
                        tokio::time::sleep(tokio::time::Duration::from_millis(100 * (attempt as u64 + 1))).await;
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| EventError::HandlerError {
            handler: handler.name().to_string(),
            message: "Unknown error".to_string(),
        }))
    }

    async fn add_to_dead_letter(&self, envelope: &IntegrationEventEnvelope, handler_name: &str, error: &str) {
        let entry = DeadLetterEntry {
            envelope: envelope.clone(),
            handler_name: handler_name.to_string(),
            error: error.to_string(),
            retry_count: 3, // Already exhausted retries
            failed_at: chrono::Utc::now(),
        };

        self.dead_letter_queue.write().await.push(entry);
    }

    /// Check if event type matches pattern
    fn matches_pattern(pattern: &str, event_type: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix(".*") {
            return event_type.starts_with(prefix);
        }
        pattern == event_type
    }
}

impl Default for IntegrationEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for IntegrationEventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            handlers: Arc::clone(&self.handlers),
            history: Arc::clone(&self.history),
            dead_letter_queue: Arc::clone(&self.dead_letter_queue),
            config: self.config.clone(),
        }
    }
}

/// A handler that logs all integration events
pub struct IntegrationLoggingHandler {
    patterns: Vec<&'static str>,
}

impl IntegrationLoggingHandler {
    /// Create a handler that logs specific patterns
    pub fn new(patterns: Vec<&'static str>) -> Self {
        Self { patterns }
    }

    /// Create a handler that logs all events
    pub fn all() -> Self {
        Self { patterns: vec!["*"] }
    }
}

#[async_trait]
impl IntegrationEventHandler for IntegrationLoggingHandler {
    async fn handle(&self, envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
        info!(
            event_type = %envelope.event_type,
            source = %envelope.source_context,
            aggregate_id = %envelope.aggregate_id,
            correlation_id = ?envelope.correlation_id,
            "Integration event received"
        );
        Ok(())
    }

    fn event_patterns(&self) -> Vec<&'static str> {
        self.patterns.clone()
    }

    fn name(&self) -> &'static str {
        "IntegrationLoggingHandler"
    }

    fn should_retry(&self) -> bool {
        false // Logging shouldn't retry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestEvent {
        id: String,
        data: String,
        occurred_at: chrono::DateTime<Utc>,
    }

    impl IntegrationEvent for TestEvent {
        fn event_type(&self) -> &'static str {
            "test.entity.created"
        }

        fn source_context(&self) -> &'static str {
            "test"
        }

        fn aggregate_id(&self) -> &str {
            &self.id
        }

        fn occurred_at(&self) -> chrono::DateTime<Utc> {
            self.occurred_at
        }
    }

    struct CountingHandler {
        count: Arc<AtomicUsize>,
        patterns: Vec<&'static str>,
    }

    impl CountingHandler {
        fn new(patterns: Vec<&'static str>) -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
                patterns,
            }
        }

        fn count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl IntegrationEventHandler for CountingHandler {
        async fn handle(&self, _envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn event_patterns(&self) -> Vec<&'static str> {
            self.patterns.clone()
        }

        fn name(&self) -> &'static str {
            "CountingHandler"
        }
    }

    #[tokio::test]
    async fn test_bus_publish_and_handle() {
        let bus = IntegrationEventBus::new();
        let handler = Arc::new(CountingHandler::new(vec!["test.entity.created"]));

        bus.register_handler(handler.clone()).await;

        let event = TestEvent {
            id: "test-123".to_string(),
            data: "Hello".to_string(),
            occurred_at: Utc::now(),
        };

        bus.publish(event).await.unwrap();

        // Give handler time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(handler.count(), 1);
    }

    #[tokio::test]
    async fn test_bus_wildcard_pattern() {
        let bus = IntegrationEventBus::new();
        let handler = Arc::new(CountingHandler::new(vec!["test.*"]));

        bus.register_handler(handler.clone()).await;

        let event = TestEvent {
            id: "test-123".to_string(),
            data: "Hello".to_string(),
            occurred_at: Utc::now(),
        };

        bus.publish(event).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(handler.count(), 1);
    }

    #[tokio::test]
    async fn test_bus_global_wildcard() {
        let bus = IntegrationEventBus::new();
        let handler = Arc::new(CountingHandler::new(vec!["*"]));

        bus.register_handler(handler.clone()).await;

        let event = TestEvent {
            id: "test-123".to_string(),
            data: "Hello".to_string(),
            occurred_at: Utc::now(),
        };

        bus.publish(event).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(handler.count(), 1);
    }

    #[tokio::test]
    async fn test_bus_history() {
        let bus = IntegrationEventBus::with_config(IntegrationBusConfig::with_persistence());

        let event = TestEvent {
            id: "test-123".to_string(),
            data: "Hello".to_string(),
            occurred_at: Utc::now(),
        };

        bus.publish(event).await.unwrap();

        let history = bus.history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_type, "test.entity.created");
    }

    #[tokio::test]
    async fn test_bus_subscribe() {
        let bus = IntegrationEventBus::new();
        let mut rx = bus.subscribe();

        let event = TestEvent {
            id: "test-123".to_string(),
            data: "Hello".to_string(),
            occurred_at: Utc::now(),
        };

        bus.publish(event).await.unwrap();

        let envelope = rx.recv().await.unwrap();
        assert_eq!(envelope.event_type, "test.entity.created");
    }

    #[test]
    fn test_pattern_matching() {
        // Exact match
        assert!(IntegrationEventBus::matches_pattern("test.user.created", "test.user.created"));
        assert!(!IntegrationEventBus::matches_pattern("test.user.created", "test.user.deleted"));

        // Wildcard suffix
        assert!(IntegrationEventBus::matches_pattern("test.user.*", "test.user.created"));
        assert!(IntegrationEventBus::matches_pattern("test.user.*", "test.user.deleted"));
        assert!(!IntegrationEventBus::matches_pattern("test.user.*", "test.role.created"));

        // Multi-level wildcard
        assert!(IntegrationEventBus::matches_pattern("test.*", "test.user.created"));
        assert!(IntegrationEventBus::matches_pattern("test.*", "test.role.deleted"));

        // Global wildcard
        assert!(IntegrationEventBus::matches_pattern("*", "anything.here"));
    }
}

//! Generic event subscriber — reusable subscription wiring for any entity+event pair.
//!
//! Eliminates the need for per-entity subscriber boilerplate.  Modules
//! register a `GenericEventSubscriber` with one or more `Arc<dyn EventHandler<Event>>`
//! instances instead of implementing handler wiring from scratch for every entity.
//!
//! # Example
//!
//! ```rust,ignore
//! use backbone_messaging::subscriber::GenericEventSubscriber;
//! use backbone_messaging::crud_event::CrudEvent;
//!
//! // Listen to StoredFile CRUD events and update a search index.
//! let subscriber = GenericEventSubscriber::<CrudEvent<StoredFile>>::new(
//!     vec!["entity.created", "entity.updated"],
//!     vec![Arc::new(SearchIndexHandler::new())],
//! );
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::envelope::EventEnvelope;
use crate::error::EventError;
use crate::event::DomainEvent;
use crate::handler::EventHandler;

// ─── SubscriberCallback (kept for SubscriberRegistry convenience) ─────────────

/// Callback type for closure-based event subscribers.
pub type SubscriberCallback<Event> =
    Arc<dyn Fn(Event) -> Pin<Box<dyn Future<Output = Result<(), EventError>> + Send>> + Send + Sync>;

// ─── GenericEventSubscriber ───────────────────────────────────────────────────

/// A generic event subscriber backed by a list of `EventHandler` implementations.
///
/// `Event` — the concrete event type (must implement `DomainEvent`)
///
/// Handlers are called in registration order.  If an event type filter is set,
/// only events whose `event_type()` matches one of the registered types are dispatched.
pub struct GenericEventSubscriber<Event: DomainEvent> {
    /// Event type discriminators this subscriber cares about.
    /// Empty vec = subscribe to all.
    subscribed_types: Vec<&'static str>,

    /// The handlers that process dispatched events.
    handlers: Vec<Arc<dyn EventHandler<Event>>>,

    /// Optional human-readable name for logging.
    name: &'static str,
}

impl<Event: DomainEvent + Clone> GenericEventSubscriber<Event> {
    /// Create a subscriber with explicit type filters and handler list.
    ///
    /// `subscribed_types` — filter to only these `event_type()` strings; empty = all.
    /// `handlers` — ordered list of handlers to invoke.
    pub fn new(
        subscribed_types: Vec<&'static str>,
        handlers: Vec<Arc<dyn EventHandler<Event>>>,
    ) -> Self {
        Self {
            subscribed_types,
            handlers,
            name: std::any::type_name::<Event>(),
        }
    }

    /// Create a subscriber that receives **all** event types.
    pub fn all(handlers: Vec<Arc<dyn EventHandler<Event>>>) -> Self {
        Self::new(vec![], handlers)
    }

    /// Override the subscriber name (used in logs).
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    /// Add a handler to the subscriber (builder pattern).
    pub fn with_handler(mut self, handler: Arc<dyn EventHandler<Event>>) -> Self {
        self.handlers.push(handler);
        self
    }

    /// Returns true if this subscriber should receive the given event type.
    pub fn is_interested(&self, event_type: &str) -> bool {
        self.subscribed_types.is_empty() || self.subscribed_types.contains(&event_type)
    }

    /// Dispatch an event to all registered handlers in order.
    ///
    /// The raw event is wrapped in an `EventEnvelope` before dispatch.
    /// Returns on the first handler error.
    pub async fn dispatch(&self, event: Event) -> Result<(), EventError> {
        if !self.is_interested(event.event_type()) {
            return Ok(());
        }
        let envelope = EventEnvelope::new(event);
        for handler in &self.handlers {
            handler.handle(envelope.clone()).await?;
        }
        Ok(())
    }

    /// The subscriber's display name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The event types this subscriber handles (empty = all).
    pub fn subscribed_types(&self) -> &[&'static str] {
        &self.subscribed_types
    }

    /// Number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

// ─── SubscriberRegistry ───────────────────────────────────────────────────────

/// A registry that holds multiple closure-based subscribers for the same event type.
///
/// Dispatch an event to all registered subscribers in registration order.
///
/// For `EventHandler`-based subscriptions prefer `GenericEventSubscriber`.
pub struct SubscriberRegistry<Event: Clone + Send + Sync + 'static> {
    subscribers: Vec<SubscriberCallback<Event>>,
}

impl<Event: Clone + Send + Sync + 'static> SubscriberRegistry<Event> {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// Register a closure subscriber.
    pub fn register<F, Fut>(&mut self, handler: F)
    where
        F: Fn(Event) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), EventError>> + Send + 'static,
    {
        self.subscribers
            .push(Arc::new(move |event| Box::pin(handler(event))));
    }

    /// Dispatch to all registered subscribers, collecting errors.
    pub async fn dispatch_all(&self, event: Event) -> Vec<EventError> {
        let mut errors = Vec::new();
        for subscriber in &self.subscribers {
            if let Err(e) = subscriber(event.clone()).await {
                errors.push(e);
            }
        }
        errors
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl<Event: Clone + Send + Sync + 'static> Default for SubscriberRegistry<Event> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::DomainEvent;

    // A minimal DomainEvent for testing
    #[derive(Clone, Debug)]
    struct FakeEvent {
        event_type: &'static str,
        id: String,
    }

    impl DomainEvent for FakeEvent {
        fn event_type(&self) -> &'static str {
            self.event_type
        }
        fn aggregate_id(&self) -> &str {
            &self.id
        }
    }

    // A simple counting handler for tests
    struct CountingHandler {
        count: Arc<tokio::sync::Mutex<u32>>,
    }

    impl CountingHandler {
        fn new(count: Arc<tokio::sync::Mutex<u32>>) -> Self {
            Self { count }
        }
    }

    #[async_trait::async_trait]
    impl EventHandler<FakeEvent> for CountingHandler {
        async fn handle(&self, _envelope: EventEnvelope<FakeEvent>) -> Result<(), EventError> {
            *self.count.lock().await += 1;
            Ok(())
        }
        fn event_types(&self) -> Vec<&'static str> {
            vec![]
        }
    }

    #[tokio::test]
    async fn subscriber_fires_for_matching_type() {
        let count = Arc::new(tokio::sync::Mutex::new(0u32));
        let handler = Arc::new(CountingHandler::new(count.clone()));

        let subscriber = GenericEventSubscriber::new(
            vec!["created"],
            vec![handler as Arc<dyn EventHandler<FakeEvent>>],
        );

        assert!(subscriber.is_interested("created"));
        assert!(!subscriber.is_interested("deleted"));

        let event = FakeEvent { event_type: "created", id: "e1".into() };
        subscriber.dispatch(event).await.unwrap();
        assert_eq!(*count.lock().await, 1);
    }

    #[tokio::test]
    async fn subscriber_skips_non_matching_type() {
        let count = Arc::new(tokio::sync::Mutex::new(0u32));
        let handler = Arc::new(CountingHandler::new(count.clone()));

        let subscriber = GenericEventSubscriber::new(
            vec!["created"],
            vec![handler as Arc<dyn EventHandler<FakeEvent>>],
        );

        let event = FakeEvent { event_type: "deleted", id: "e1".into() };
        subscriber.dispatch(event).await.unwrap();
        // Not interested in "deleted" → handler NOT called
        assert_eq!(*count.lock().await, 0);
    }

    #[tokio::test]
    async fn registry_dispatches_to_all_subscribers() {
        let mut registry = SubscriberRegistry::<FakeEvent>::new();
        let counter = Arc::new(tokio::sync::Mutex::new(0u32));

        for _ in 0..3 {
            let c = counter.clone();
            registry.register(move |_event: FakeEvent| {
                let cc = c.clone();
                async move {
                    *cc.lock().await += 1;
                    Ok(())
                }
            });
        }

        let errors = registry
            .dispatch_all(FakeEvent { event_type: "created", id: "1".into() })
            .await;
        assert!(errors.is_empty());
        assert_eq!(*counter.lock().await, 3);
    }
}

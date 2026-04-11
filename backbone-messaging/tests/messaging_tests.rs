//! Comprehensive tests for backbone-messaging
//!
//! Covers domain EventBus, IntegrationEventBus, EventEnvelope,
//! dead letter queue, retry logic, and error handling.

use backbone_messaging::{
    DomainEvent, EventBus, EventBusConfig, EventEnvelope, EventEnvelopeBuilder,
    EventError, CollectingHandler,
    IntegrationEvent, IntegrationEventBus, IntegrationBusConfig,
    IntegrationEventEnvelope, IntegrationEventHandler,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// Test Event Types
// ============================================================================

#[derive(Clone, Debug)]
struct OrderCreated {
    order_id: String,
    #[allow(dead_code)]
    customer: String,
}

impl DomainEvent for OrderCreated {
    fn event_type(&self) -> &'static str { "OrderCreated" }
    fn aggregate_id(&self) -> &str { &self.order_id }
}

/// A domain event that uses custom version
#[derive(Clone, Debug)]
struct CustomVersionEvent {
    id: String,
}

impl DomainEvent for CustomVersionEvent {
    fn event_type(&self) -> &'static str { "CustomVersionEvent" }
    fn aggregate_id(&self) -> &str { &self.id }
    fn version(&self) -> u32 { 3 }
}

// Integration event
#[derive(Clone, Debug, Serialize, Deserialize)]
struct UserRegistered {
    user_id: String,
    email: String,
    occurred_at: DateTime<Utc>,
}

impl IntegrationEvent for UserRegistered {
    fn event_type(&self) -> &'static str { "sapiens.user.registered" }
    fn source_context(&self) -> &'static str { "sapiens" }
    fn aggregate_id(&self) -> &str { &self.user_id }
    fn occurred_at(&self) -> DateTime<Utc> { self.occurred_at }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UserDeleted {
    user_id: String,
    occurred_at: DateTime<Utc>,
}

impl IntegrationEvent for UserDeleted {
    fn event_type(&self) -> &'static str { "sapiens.user.deleted" }
    fn source_context(&self) -> &'static str { "sapiens" }
    fn aggregate_id(&self) -> &str { &self.user_id }
    fn occurred_at(&self) -> DateTime<Utc> { self.occurred_at }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrderPaid {
    order_id: String,
    occurred_at: DateTime<Utc>,
}

impl IntegrationEvent for OrderPaid {
    fn event_type(&self) -> &'static str { "billing.order.paid" }
    fn source_context(&self) -> &'static str { "billing" }
    fn aggregate_id(&self) -> &str { &self.order_id }
    fn occurred_at(&self) -> DateTime<Utc> { self.occurred_at }
}

// ============================================================================
// Test Handlers
// ============================================================================

/// A handler that always fails
struct FailingIntegrationHandler {
    attempt_count: Arc<AtomicU32>,
    patterns: Vec<&'static str>,
    retry: bool,
    retries: u32,
}

impl FailingIntegrationHandler {
    fn new(patterns: Vec<&'static str>) -> Self {
        Self {
            attempt_count: Arc::new(AtomicU32::new(0)),
            patterns,
            retry: true,
            retries: 3,
        }
    }

    fn no_retry(patterns: Vec<&'static str>) -> Self {
        Self {
            attempt_count: Arc::new(AtomicU32::new(0)),
            patterns,
            retry: false,
            retries: 1,
        }
    }

}

#[async_trait]
impl IntegrationEventHandler for FailingIntegrationHandler {
    async fn handle(&self, _envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
        self.attempt_count.fetch_add(1, Ordering::SeqCst);
        Err(EventError::handler("FailingHandler", "intentional failure"))
    }

    fn event_patterns(&self) -> Vec<&'static str> {
        self.patterns.clone()
    }

    fn name(&self) -> &'static str {
        "FailingHandler"
    }

    fn should_retry(&self) -> bool {
        self.retry
    }

    fn max_retries(&self) -> u32 {
        self.retries
    }
}

/// A collecting handler for integration events
struct CollectingIntegrationHandler {
    count: Arc<AtomicU32>,
    patterns: Vec<&'static str>,
}

impl CollectingIntegrationHandler {
    fn new(patterns: Vec<&'static str>) -> Self {
        Self {
            count: Arc::new(AtomicU32::new(0)),
            patterns,
        }
    }

    fn count(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl IntegrationEventHandler for CollectingIntegrationHandler {
    async fn handle(&self, _envelope: IntegrationEventEnvelope) -> Result<(), EventError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn event_patterns(&self) -> Vec<&'static str> {
        self.patterns.clone()
    }

    fn name(&self) -> &'static str {
        "CollectingIntegrationHandler"
    }
}

// ============================================================================
// Domain EventBus Tests
// ============================================================================

// We need a single event type enum for the bus since EventBus<E> is generic
#[derive(Clone, Debug)]
enum TestDomainEvent {
    Created { id: String, #[allow(dead_code)] customer: String },
    Shipped { id: String },
}

impl DomainEvent for TestDomainEvent {
    fn event_type(&self) -> &'static str {
        match self {
            TestDomainEvent::Created { .. } => "OrderCreated",
            TestDomainEvent::Shipped { .. } => "OrderShipped",
        }
    }

    fn aggregate_id(&self) -> &str {
        match self {
            TestDomainEvent::Created { id, .. } => id,
            TestDomainEvent::Shipped { id } => id,
        }
    }
}

#[tokio::test]
async fn test_events_by_type() {
    let bus = EventBus::<TestDomainEvent>::with_config(EventBusConfig::with_persistence());

    bus.publish(TestDomainEvent::Created { id: "o1".into(), customer: "c1".into() }).await.unwrap();
    bus.publish(TestDomainEvent::Shipped { id: "o2".into() }).await.unwrap();
    bus.publish(TestDomainEvent::Created { id: "o3".into(), customer: "c2".into() }).await.unwrap();

    let created = bus.events_by_type("OrderCreated").await;
    assert_eq!(created.len(), 2, "Should have 2 OrderCreated events");

    let shipped = bus.events_by_type("OrderShipped").await;
    assert_eq!(shipped.len(), 1, "Should have 1 OrderShipped event");

    let none = bus.events_by_type("NonExistent").await;
    assert_eq!(none.len(), 0);
}

#[tokio::test]
async fn test_events_in_range() {
    let bus = EventBus::<OrderCreated>::with_config(EventBusConfig::with_persistence());

    let before = Utc::now();
    sleep(Duration::from_millis(50)).await;

    bus.publish(OrderCreated { order_id: "o1".into(), customer: "c1".into() }).await.unwrap();

    sleep(Duration::from_millis(50)).await;
    let middle = Utc::now();
    sleep(Duration::from_millis(50)).await;

    bus.publish(OrderCreated { order_id: "o2".into(), customer: "c2".into() }).await.unwrap();

    sleep(Duration::from_millis(50)).await;
    let after = Utc::now();

    // Full range
    let all = bus.events_in_range(before, after).await;
    assert_eq!(all.len(), 2, "Full range should contain all events");

    // Partial range (only first event)
    let first_half = bus.events_in_range(before, middle).await;
    assert_eq!(first_half.len(), 1, "First half should contain 1 event");
}

#[tokio::test]
async fn test_publish_all() {
    let bus = EventBus::<OrderCreated>::with_config(EventBusConfig::with_persistence());

    let events = vec![
        OrderCreated { order_id: "o1".into(), customer: "c1".into() },
        OrderCreated { order_id: "o2".into(), customer: "c2".into() },
        OrderCreated { order_id: "o3".into(), customer: "c3".into() },
    ];

    bus.publish_all(events).await.unwrap();

    let history = bus.history().await;
    assert_eq!(history.len(), 3, "publish_all should store all 3 events");
}

#[tokio::test]
async fn test_multiple_handlers_same_event() {
    let bus = EventBus::<OrderCreated>::new();
    let handler_a = Arc::new(CollectingHandler::<OrderCreated>::new());
    let handler_b = Arc::new(CollectingHandler::<OrderCreated>::new());

    bus.register_handler(handler_a.clone()).await;
    bus.register_handler(handler_b.clone()).await;

    bus.publish(OrderCreated { order_id: "o1".into(), customer: "c1".into() }).await.unwrap();

    sleep(Duration::from_millis(50)).await;

    assert_eq!(handler_a.count().await, 1, "Handler A should receive event");
    assert_eq!(handler_b.count().await, 1, "Handler B should receive event");
}

#[tokio::test]
async fn test_handler_count() {
    let bus = EventBus::<OrderCreated>::new();

    assert_eq!(bus.handler_count().await, 0);

    bus.register_handler(Arc::new(CollectingHandler::<OrderCreated>::new())).await;
    assert_eq!(bus.handler_count().await, 1);

    bus.register_handler(Arc::new(CollectingHandler::<OrderCreated>::new())).await;
    assert_eq!(bus.handler_count().await, 2);
}

#[tokio::test]
async fn test_clear_history() {
    let bus = EventBus::<OrderCreated>::with_config(EventBusConfig::with_persistence());

    bus.publish(OrderCreated { order_id: "o1".into(), customer: "c1".into() }).await.unwrap();
    bus.publish(OrderCreated { order_id: "o2".into(), customer: "c2".into() }).await.unwrap();

    assert_eq!(bus.history().await.len(), 2);

    bus.clear_history().await;
    assert_eq!(bus.history().await.len(), 0, "History should be empty after clear");
}

#[tokio::test]
async fn test_config_max_history_size() {
    let config = EventBusConfig {
        persist_events: true,
        max_history_size: 3,
        ..EventBusConfig::default()
    };
    let bus = EventBus::<OrderCreated>::with_config(config);

    for i in 0..5 {
        bus.publish(OrderCreated {
            order_id: format!("o{}", i),
            customer: format!("c{}", i),
        }).await.unwrap();
    }

    let history = bus.history().await;
    assert!(history.len() <= 3, "History should be trimmed to max_history_size");
}

// ============================================================================
// EventEnvelope Tests
// ============================================================================

#[tokio::test]
async fn test_envelope_builder() {
    let event = OrderCreated { order_id: "o1".into(), customer: "c1".into() };

    let envelope = EventEnvelopeBuilder::new(event)
        .correlation_id("corr-123")
        .causation_id("cause-456")
        .metadata("source", "test")
        .metadata("env", "dev")
        .build();

    assert_eq!(envelope.correlation_id, Some("corr-123".to_string()));
    assert_eq!(envelope.causation_id, Some("cause-456".to_string()));
    assert_eq!(envelope.metadata.get("source"), Some(&"test".to_string()));
    assert_eq!(envelope.metadata.get("env"), Some(&"dev".to_string()));
    assert_eq!(envelope.event_type, "OrderCreated");
    assert_eq!(envelope.aggregate_id, "o1");
}

#[tokio::test]
async fn test_envelope_correlation_tracking() {
    let event_a = OrderCreated { order_id: "o1".into(), customer: "c1".into() };
    let event_b = OrderCreated { order_id: "o2".into(), customer: "c2".into() };
    let event_c = OrderCreated { order_id: "o3".into(), customer: "c3".into() };

    let envelope_a = EventEnvelope::new(event_a)
        .with_correlation_id("corr-1");
    let envelope_b = EventEnvelope::new(event_b)
        .with_correlation_id("corr-1");
    let envelope_c = EventEnvelope::new(event_c)
        .with_correlation_id("corr-2");

    assert!(envelope_a.is_correlated_with(&envelope_b), "Same correlation ID");
    assert!(!envelope_a.is_correlated_with(&envelope_c), "Different correlation ID");

    // Causation tracking
    let parent = EventEnvelope::new(OrderCreated { order_id: "p1".into(), customer: "c".into() });
    let parent_id = parent.id.clone();
    let child = EventEnvelope::new(OrderCreated { order_id: "c1".into(), customer: "c".into() })
        .with_causation_id(parent_id);

    assert!(child.was_caused_by(&parent), "Child should be caused by parent");
}

#[tokio::test]
async fn test_event_error_constructors() {
    let handler_err = EventError::handler("MyHandler", "something failed");
    assert!(format!("{}", handler_err).contains("MyHandler"));
    assert!(format!("{}", handler_err).contains("something failed"));

    let publish_err = EventError::publish("channel full");
    assert!(format!("{}", publish_err).contains("channel full"));

    let ser_err = EventError::serialization("invalid JSON");
    assert!(format!("{}", ser_err).contains("invalid JSON"));
}

#[tokio::test]
async fn test_domain_event_defaults() {
    let event = OrderCreated { order_id: "o1".into(), customer: "c1".into() };

    // Default version is 1
    assert_eq!(event.version(), 1);

    // aggregate_type() defaults to event_type for events ending with known suffixes
    let agg_type = event.aggregate_type();
    assert_eq!(agg_type, "OrderCreated");

    // Custom version
    let custom = CustomVersionEvent { id: "x".into() };
    assert_eq!(custom.version(), 3);
}

// ============================================================================
// IntegrationEventBus Tests
// ============================================================================

#[tokio::test]
async fn test_dead_letter_queue() {
    let bus = IntegrationEventBus::new(); // default config has dead letter enabled

    let handler = Arc::new(FailingIntegrationHandler::new(vec!["sapiens.*"]));
    bus.register_handler(handler.clone()).await;

    let event = UserRegistered {
        user_id: "u1".into(),
        email: "test@test.com".into(),
        occurred_at: Utc::now(),
    };
    bus.publish(event).await.unwrap();

    // Wait for retry backoff to complete
    sleep(Duration::from_millis(1000)).await;

    let dead_letters = bus.dead_letters().await;
    assert_eq!(dead_letters.len(), 1, "Should have 1 dead letter entry");
    assert_eq!(dead_letters[0].handler_name, "FailingHandler");
    assert!(dead_letters[0].error.contains("intentional failure"));

    // Clear dead letters
    bus.clear_dead_letters().await;
    assert_eq!(bus.dead_letters().await.len(), 0, "Dead letters should be cleared");
}

#[tokio::test]
async fn test_handler_retry_exhausted() {
    let bus = IntegrationEventBus::new();

    let handler = Arc::new(FailingIntegrationHandler::new(vec!["sapiens.*"]));
    let attempt_count = handler.attempt_count.clone();
    bus.register_handler(handler).await;

    let event = UserRegistered {
        user_id: "u2".into(),
        email: "retry@test.com".into(),
        occurred_at: Utc::now(),
    };
    bus.publish(event).await.unwrap();

    sleep(Duration::from_millis(1000)).await;

    // Default max_retries is 3, so should have 3 attempts
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3, "Should retry 3 times");
}

#[tokio::test]
async fn test_handler_no_retry() {
    let bus = IntegrationEventBus::new();

    let handler = Arc::new(FailingIntegrationHandler::no_retry(vec!["sapiens.*"]));
    let attempt_count = handler.attempt_count.clone();
    bus.register_handler(handler).await;

    let event = UserRegistered {
        user_id: "u3".into(),
        email: "noretry@test.com".into(),
        occurred_at: Utc::now(),
    };
    bus.publish(event).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    assert_eq!(attempt_count.load(Ordering::SeqCst), 1, "Should only attempt once (no retry)");
}

#[tokio::test]
async fn test_handler_deduplication() {
    let bus = IntegrationEventBus::new();

    // Handler matches both patterns, but should be called only once per event
    let handler = Arc::new(CollectingIntegrationHandler::new(vec!["sapiens.*", "sapiens.user.*"]));
    bus.register_handler(handler.clone()).await;

    let event = UserRegistered {
        user_id: "u4".into(),
        email: "dedup@test.com".into(),
        occurred_at: Utc::now(),
    };
    bus.publish(event).await.unwrap();

    sleep(Duration::from_millis(50)).await;

    assert_eq!(handler.count(), 1, "Handler should be called exactly once despite matching multiple patterns");
}

#[tokio::test]
async fn test_events_by_pattern() {
    let bus = IntegrationEventBus::new();

    bus.publish(UserRegistered {
        user_id: "u1".into(), email: "a@test.com".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    bus.publish(UserDeleted {
        user_id: "u2".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    bus.publish(OrderPaid {
        order_id: "o1".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    let user_events = bus.events_by_pattern("sapiens.user.*").await;
    assert_eq!(user_events.len(), 2, "Should match both sapiens.user.* events");

    let billing_events = bus.events_by_pattern("billing.*").await;
    assert_eq!(billing_events.len(), 1, "Should match 1 billing event");

    let all_events = bus.events_by_pattern("*").await;
    assert_eq!(all_events.len(), 3, "Wildcard should match all events");
}

#[tokio::test]
async fn test_events_for_aggregate() {
    let bus = IntegrationEventBus::new();

    bus.publish(UserRegistered {
        user_id: "user-1".into(), email: "a@test.com".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    bus.publish(UserDeleted {
        user_id: "user-1".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    bus.publish(UserRegistered {
        user_id: "user-2".into(), email: "b@test.com".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    let events = bus.events_for_aggregate("user-1").await;
    assert_eq!(events.len(), 2, "Should find 2 events for user-1");

    let events = bus.events_for_aggregate("user-2").await;
    assert_eq!(events.len(), 1, "Should find 1 event for user-2");
}

#[tokio::test]
async fn test_concurrent_publishing() {
    let bus = Arc::new(IntegrationEventBus::new());
    let mut handles = vec![];

    for i in 0..10 {
        let bus = bus.clone();
        let handle = tokio::spawn(async move {
            bus.publish(UserRegistered {
                user_id: format!("concurrent-{}", i),
                email: format!("user{}@test.com", i),
                occurred_at: Utc::now(),
            }).await.unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let history = bus.history().await;
    assert_eq!(history.len(), 10, "All 10 concurrent events should be in history");
}

#[tokio::test]
async fn test_config_disables_persistence() {
    let config = IntegrationBusConfig {
        persist_events: false,
        ..IntegrationBusConfig::default()
    };
    let bus = IntegrationEventBus::with_config(config);

    bus.publish(UserRegistered {
        user_id: "u1".into(), email: "a@test.com".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    let history = bus.history().await;
    assert_eq!(history.len(), 0, "History should be empty when persistence is disabled");
}

#[tokio::test]
async fn test_config_disables_dead_letter() {
    let config = IntegrationBusConfig {
        enable_dead_letter_queue: false,
        ..IntegrationBusConfig::default()
    };
    let bus = IntegrationEventBus::with_config(config);

    let handler = Arc::new(FailingIntegrationHandler::new(vec!["sapiens.*"]));
    bus.register_handler(handler).await;

    bus.publish(UserRegistered {
        user_id: "u1".into(), email: "a@test.com".into(), occurred_at: Utc::now(),
    }).await.unwrap();

    sleep(Duration::from_millis(1000)).await;

    let dead_letters = bus.dead_letters().await;
    assert_eq!(dead_letters.len(), 0, "No dead letters when dead letter queue is disabled");
}

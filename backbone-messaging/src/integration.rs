//! Integration Events - Cross-Bounded Context Communication
//!
//! Integration events differ from domain events:
//! - **Domain events**: Internal to a bounded context, may contain domain-specific types
//! - **Integration events**: Published for other bounded contexts to consume, contain only primitive/common types
//!
//! # Key Differences
//!
//! | Aspect | Domain Event | Integration Event |
//! |--------|--------------|-------------------|
//! | Scope | Single bounded context | Cross-context |
//! | Types | Can use domain types | Primitives only |
//! | Coupling | Internal | Loose coupling |
//! | Serialization | Optional | Required (JSON) |
//!
//! # Example
//!
//! ```rust,ignore
//! use backbone_messaging::IntegrationEvent;
//! use chrono::{DateTime, Utc};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! pub struct UserCreatedIntegrationEvent {
//!     pub user_id: String,
//!     pub email: String,
//!     pub occurred_at: DateTime<Utc>,
//! }
//!
//! impl IntegrationEvent for UserCreatedIntegrationEvent {
//!     fn event_type(&self) -> &'static str { "sapiens.user.created" }
//!     fn source_context(&self) -> &'static str { "sapiens" }
//!     fn aggregate_id(&self) -> &str { &self.user_id }
//!     fn occurred_at(&self) -> DateTime<Utc> { self.occurred_at }
//! }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trait for integration events that cross bounded context boundaries
///
/// Integration events are serializable and contain only primitive/common types
/// to avoid coupling between modules. They follow a naming convention:
/// `{source_context}.{aggregate}.{action}` (e.g., "sapiens.user.created")
///
/// # Implementation Requirements
///
/// - Must be `Clone + Send + Sync + 'static` for async handling
/// - Must implement `Serialize + Deserialize` for JSON transport
/// - Should only use primitive types (String, numbers, booleans)
/// - Event type should follow dot-notation naming
///
/// # Example
///
/// ```rust,ignore
/// impl IntegrationEvent for UserCreatedEvent {
///     fn event_type(&self) -> &'static str { "sapiens.user.created" }
///     fn source_context(&self) -> &'static str { "sapiens" }
///     fn aggregate_id(&self) -> &str { &self.user_id }
///     fn occurred_at(&self) -> DateTime<Utc> { self.occurred_at }
/// }
/// ```
pub trait IntegrationEvent: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static {
    /// Event type identifier using dot notation
    ///
    /// Format: `{source_context}.{aggregate}.{action}`
    /// Examples: "sapiens.user.created", "postman.email.sent"
    fn event_type(&self) -> &'static str;

    /// Source bounded context that publishes this event
    ///
    /// Examples: "sapiens", "postman", "bucket"
    fn source_context(&self) -> &'static str;

    /// Aggregate ID in the source context
    ///
    /// This identifies which aggregate instance the event belongs to
    fn aggregate_id(&self) -> &str;

    /// When the event occurred in the domain
    fn occurred_at(&self) -> DateTime<Utc>;

    /// Event schema version for evolution
    ///
    /// Increment when making breaking changes to the event structure.
    /// Consumers can use this to handle multiple versions.
    fn version(&self) -> u32 {
        1
    }

    /// Correlation ID for distributed tracing
    ///
    /// Used to trace a request across multiple bounded contexts.
    /// Returns `None` by default; override if your event carries correlation info.
    fn correlation_id(&self) -> Option<&str> {
        None
    }
}

/// Type-erased integration event envelope for cross-module transport
///
/// The envelope wraps any integration event with metadata and serializes
/// the event payload to JSON for transport between bounded contexts.
///
/// # Fields
///
/// - `id`: Unique envelope ID (UUID)
/// - `event_type`: Dot-notation event type
/// - `source_context`: Origin bounded context
/// - `payload`: JSON-serialized event data
///
/// # Example
///
/// ```rust,ignore
/// let event = UserCreatedIntegrationEvent { ... };
/// let envelope = IntegrationEventEnvelope::from_event(&event)?;
///
/// // Later, deserialize back to the typed event
/// let restored: UserCreatedIntegrationEvent = envelope.deserialize()?;
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntegrationEventEnvelope {
    /// Unique envelope ID
    pub id: String,
    /// Event type (e.g., "sapiens.user.created")
    pub event_type: String,
    /// Source bounded context
    pub source_context: String,
    /// Aggregate ID in source context
    pub aggregate_id: String,
    /// When the event occurred
    pub occurred_at: DateTime<Utc>,
    /// When the envelope was created/published
    pub published_at: DateTime<Utc>,
    /// Event schema version
    pub version: u32,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// Causation ID (parent event/envelope that caused this one)
    pub causation_id: Option<String>,
    /// JSON-serialized event payload
    pub payload: serde_json::Value,
}

impl IntegrationEventEnvelope {
    /// Create an envelope from an integration event
    ///
    /// Serializes the event to JSON and wraps it with metadata.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if the event cannot be serialized.
    pub fn from_event<E: IntegrationEvent>(event: &E) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            event_type: event.event_type().to_string(),
            source_context: event.source_context().to_string(),
            aggregate_id: event.aggregate_id().to_string(),
            occurred_at: event.occurred_at(),
            published_at: Utc::now(),
            version: event.version(),
            correlation_id: event.correlation_id().map(String::from),
            causation_id: None,
            payload: serde_json::to_value(event)?,
        })
    }

    /// Deserialize the payload back to a typed event
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if the payload doesn't match the expected type.
    pub fn deserialize<E: IntegrationEvent>(&self) -> Result<E, serde_json::Error> {
        serde_json::from_value(self.payload.clone())
    }

    /// Set the causation ID (for event chaining)
    pub fn with_causation_id(mut self, causation_id: impl Into<String>) -> Self {
        self.causation_id = Some(causation_id.into());
        self
    }

    /// Set the correlation ID (for distributed tracing)
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    /// Check if this envelope matches a pattern
    ///
    /// Patterns support:
    /// - Exact match: "sapiens.user.created"
    /// - Wildcard suffix: "sapiens.user.*" matches "sapiens.user.created", "sapiens.user.deleted"
    /// - Global wildcard: "*" matches everything
    pub fn matches_pattern(&self, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix(".*") {
            return self.event_type.starts_with(prefix);
        }
        pattern == self.event_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestIntegrationEvent {
        user_id: String,
        email: String,
        occurred_at: DateTime<Utc>,
    }

    impl IntegrationEvent for TestIntegrationEvent {
        fn event_type(&self) -> &'static str {
            "test.user.created"
        }

        fn source_context(&self) -> &'static str {
            "test"
        }

        fn aggregate_id(&self) -> &str {
            &self.user_id
        }

        fn occurred_at(&self) -> DateTime<Utc> {
            self.occurred_at
        }
    }

    #[test]
    fn test_envelope_from_event() {
        let event = TestIntegrationEvent {
            user_id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            occurred_at: Utc::now(),
        };

        let envelope = IntegrationEventEnvelope::from_event(&event).unwrap();

        assert_eq!(envelope.event_type, "test.user.created");
        assert_eq!(envelope.source_context, "test");
        assert_eq!(envelope.aggregate_id, "user-123");
        assert_eq!(envelope.version, 1);
        assert!(!envelope.id.is_empty());
    }

    #[test]
    fn test_envelope_deserialize() {
        let event = TestIntegrationEvent {
            user_id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            occurred_at: Utc::now(),
        };

        let envelope = IntegrationEventEnvelope::from_event(&event).unwrap();
        let restored: TestIntegrationEvent = envelope.deserialize().unwrap();

        assert_eq!(restored.user_id, "user-123");
        assert_eq!(restored.email, "test@example.com");
    }

    #[test]
    fn test_pattern_matching() {
        let event = TestIntegrationEvent {
            user_id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            occurred_at: Utc::now(),
        };

        let envelope = IntegrationEventEnvelope::from_event(&event).unwrap();

        // Exact match
        assert!(envelope.matches_pattern("test.user.created"));
        assert!(!envelope.matches_pattern("test.user.deleted"));

        // Wildcard suffix
        assert!(envelope.matches_pattern("test.user.*"));
        assert!(envelope.matches_pattern("test.*"));
        assert!(!envelope.matches_pattern("other.*"));

        // Global wildcard
        assert!(envelope.matches_pattern("*"));
    }

    #[test]
    fn test_envelope_with_causation() {
        let event = TestIntegrationEvent {
            user_id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            occurred_at: Utc::now(),
        };

        let envelope = IntegrationEventEnvelope::from_event(&event)
            .unwrap()
            .with_causation_id("parent-event-id")
            .with_correlation_id("trace-123");

        assert_eq!(envelope.causation_id, Some("parent-event-id".to_string()));
        assert_eq!(envelope.correlation_id, Some("trace-123".to_string()));
    }
}

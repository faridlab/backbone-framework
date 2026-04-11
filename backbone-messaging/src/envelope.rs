//! Event Envelope - Metadata wrapper for domain events

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DomainEvent;

/// Wrapper for domain events with metadata
///
/// EventEnvelope adds infrastructure concerns (IDs, timestamps, tracing)
/// around domain events without polluting the domain model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<E: DomainEvent> {
    /// Unique event ID
    pub id: String,
    /// Event type name
    pub event_type: &'static str,
    /// Aggregate ID this event belongs to
    pub aggregate_id: String,
    /// Aggregate type name
    pub aggregate_type: &'static str,
    /// The domain event payload
    #[serde(skip)]
    event: Option<E>,
    /// When the event occurred in the domain
    pub occurred_at: DateTime<Utc>,
    /// When the event was published to the bus
    pub published_at: DateTime<Utc>,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// Causation ID (parent event that caused this one)
    pub causation_id: Option<String>,
    /// Event schema version
    pub version: u32,
    /// Additional metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl<E: DomainEvent> EventEnvelope<E> {
    /// Create a new event envelope from a domain event
    pub fn new(event: E) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: event.event_type(),
            aggregate_id: event.aggregate_id().to_string(),
            aggregate_type: event.aggregate_type(),
            occurred_at: event.occurred_at(),
            published_at: Utc::now(),
            version: event.version(),
            event: Some(event),
            correlation_id: None,
            causation_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Get the wrapped event
    pub fn event(&self) -> Option<&E> {
        self.event.as_ref()
    }

    /// Take ownership of the wrapped event
    pub fn into_event(self) -> Option<E> {
        self.event
    }

    /// Set correlation ID for distributed tracing
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set causation ID (parent event)
    pub fn with_causation_id(mut self, id: impl Into<String>) -> Self {
        self.causation_id = Some(id.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if this event is correlated with another
    pub fn is_correlated_with(&self, other: &Self) -> bool {
        match (&self.correlation_id, &other.correlation_id) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Check if this event was caused by another
    pub fn was_caused_by(&self, other: &Self) -> bool {
        match &self.causation_id {
            Some(causation) => causation == &other.id,
            None => false,
        }
    }
}

/// Builder for creating event envelopes with metadata
pub struct EventEnvelopeBuilder<E: DomainEvent> {
    envelope: EventEnvelope<E>,
}

impl<E: DomainEvent> EventEnvelopeBuilder<E> {
    /// Start building an envelope from an event
    pub fn new(event: E) -> Self {
        Self {
            envelope: EventEnvelope::new(event),
        }
    }

    /// Set correlation ID
    pub fn correlation_id(mut self, id: impl Into<String>) -> Self {
        self.envelope.correlation_id = Some(id.into());
        self
    }

    /// Set causation ID
    pub fn causation_id(mut self, id: impl Into<String>) -> Self {
        self.envelope.causation_id = Some(id.into());
        self
    }

    /// Add metadata entry
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.envelope.metadata.insert(key.into(), value.into());
        self
    }

    /// Build the envelope
    pub fn build(self) -> EventEnvelope<E> {
        self.envelope
    }
}

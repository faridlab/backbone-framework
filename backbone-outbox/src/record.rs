//! The durable event record — the wire shape carried from a producer's outbox to a consumer's inbox.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A serialized domain event staged in the outbox and carried to consumers.
///
/// The `id` is the single end-to-end dedup key (the `backbone-messaging` envelope id): the producer
/// stages it, the relay carries it, the consumer dedups on it via [`crate::inbox::once`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxRecord {
    /// The event id — the dedup key. Take it from the messaging `EventEnvelope::id`.
    pub id: Uuid,
    /// The event type name, e.g. `"PaymentSettled"`.
    pub event_type: String,
    /// The aggregate type the event belongs to, e.g. `"Payment"`.
    pub aggregate_type: String,
    /// The aggregate id (as a string — aggregates key on uuid, code, or number).
    pub aggregate_id: String,
    /// The owning tenant — the outbox_events table is fenced by this (ADR-0011). Every staged event
    /// carries the company it belongs to so the RLS policy can isolate tenant event streams.
    pub company_id: Uuid,
    /// The serialized domain event payload.
    pub payload: serde_json::Value,
    /// When the event occurred in the domain.
    pub occurred_at: DateTime<Utc>,
    /// Correlation id for tracing a business flow across events.
    pub correlation_id: Option<String>,
    /// Causation id (the parent event that caused this one).
    pub causation_id: Option<String>,
    /// Event schema version (see `extension-contract.md` §4).
    pub version: i32,
}

impl OutboxRecord {
    /// Build a record with a fresh id, no correlation/causation, version 1, `occurred_at = now`.
    /// (`now` is passed in so callers stay deterministic under test.) `company_id` is the owning
    /// tenant — required, since the outbox_events table is fenced by it (ADR-0011).
    pub fn new(
        event_type: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        company_id: Uuid,
        payload: serde_json::Value,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type: event_type.into(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            company_id,
            payload,
            occurred_at,
            correlation_id: None,
            causation_id: None,
            version: 1,
        }
    }

    /// Set the event id explicitly (e.g. reuse the messaging envelope id).
    pub fn with_id(mut self, id: Uuid) -> Self {
        self.id = id;
        self
    }

    /// Set the correlation id.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}

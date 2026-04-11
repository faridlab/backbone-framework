//! Backbone Messaging - Event-driven messaging infrastructure
//!
//! This crate provides a generic, type-safe event bus system for domain events
//! following the Event-Driven Architecture and CQRS patterns.
//!
//! # Features
//!
//! - **Generic Event Bus**: Type-safe publish/subscribe for any event type
//! - **Event Envelope**: Metadata wrapper with correlation/causation IDs
//! - **Event Handlers**: Async trait-based event handling
//! - **Event History**: Optional event persistence for replay
//! - **Composite Events**: Support for aggregate event streams
//! - **Integration Events**: Cross-bounded context communication (Phase 6)
//!
//! # Domain Events vs Integration Events
//!
//! | Aspect | Domain Event | Integration Event |
//! |--------|--------------|-------------------|
//! | Scope | Single bounded context | Cross-context |
//! | Types | Can use domain types | Primitives only (JSON) |
//! | Coupling | Internal | Loose coupling |
//! | Bus | `EventBus<E>` (typed) | `IntegrationEventBus` (type-erased) |
//!
//! # Example - Domain Events
//!
//! ```rust,ignore
//! use backbone_messaging::{EventBus, EventHandler, DomainEvent};
//! use async_trait::async_trait;
//!
//! // Define your domain event
//! #[derive(Clone, Debug)]
//! struct UserCreated {
//!     user_id: String,
//!     email: String,
//! }
//!
//! impl DomainEvent for UserCreated {
//!     fn event_type(&self) -> &'static str { "UserCreated" }
//!     fn aggregate_id(&self) -> &str { &self.user_id }
//! }
//!
//! // Create and use the event bus
//! let bus = EventBus::<UserCreated>::new();
//! bus.publish(UserCreated { user_id: "123".into(), email: "test@example.com".into() }).await?;
//! ```
//!
//! # Example - Integration Events (Cross-Module)
//!
//! ```rust,ignore
//! use backbone_messaging::{IntegrationEventBus, IntegrationEvent, IntegrationEventHandler};
//! use chrono::{DateTime, Utc};
//! use serde::{Deserialize, Serialize};
//!
//! // Define an integration event (must be serializable)
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct UserCreatedIntegrationEvent {
//!     user_id: String,
//!     email: String,
//!     occurred_at: DateTime<Utc>,
//! }
//!
//! impl IntegrationEvent for UserCreatedIntegrationEvent {
//!     fn event_type(&self) -> &'static str { "sapiens.user.created" }
//!     fn source_context(&self) -> &'static str { "sapiens" }
//!     fn aggregate_id(&self) -> &str { &self.user_id }
//!     fn occurred_at(&self) -> DateTime<Utc> { self.occurred_at }
//! }
//!
//! // Publish from Sapiens module
//! let bus = IntegrationEventBus::new();
//! bus.publish(UserCreatedIntegrationEvent { ... }).await?;
//!
//! // Subscribe in Postman module (using wildcard pattern)
//! bus.register_handler(Arc::new(EmailHandler)).await; // patterns: ["sapiens.user.*"]
//! ```

// Domain Events (internal to bounded context)
mod event;
mod handler;
mod bus;
mod error;
mod envelope;

// Integration Events (cross-bounded context)
mod integration;
mod integration_bus;

// Generic CRUD event infrastructure (Phase 0)
pub mod crud_event;
pub mod noop;
pub mod subscriber;

// Domain Event exports
pub use event::DomainEvent;
pub use handler::{EventHandler, LoggingHandler, CollectingHandler};
pub use bus::{EventBus, EventBusConfig};
pub use error::EventError;
pub use envelope::{EventEnvelope, EventEnvelopeBuilder};

// Integration Event exports
pub use integration::{IntegrationEvent, IntegrationEventEnvelope};
pub use integration_bus::{
    IntegrationEventBus,
    IntegrationBusConfig,
    IntegrationEventHandler,
    IntegrationLoggingHandler,
    DeadLetterEntry,
};

// Generic CRUD event re-exports
pub use crud_event::{CrudEvent, CrudEventPublisher, NoOpCrudEventPublisher};
pub use noop::{NoOpPublisher, NoOpEventBus, Publisher};
pub use subscriber::{GenericEventSubscriber, SubscriberCallback, SubscriberRegistry};

/// Prelude module for convenient imports
pub mod prelude {
    // Domain events
    pub use super::{
        DomainEvent,
        EventHandler,
        EventBus,
        EventBusConfig,
        EventError,
        EventEnvelope,
        EventEnvelopeBuilder,
        LoggingHandler,
        CollectingHandler,
    };

    // Integration events
    pub use super::{
        IntegrationEvent,
        IntegrationEventEnvelope,
        IntegrationEventBus,
        IntegrationBusConfig,
        IntegrationEventHandler,
        IntegrationLoggingHandler,
        DeadLetterEntry,
    };
}

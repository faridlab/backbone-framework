//! No-op event publisher implementations.
//!
//! Use these in tests and simple contexts where event publishing
//! is wired but should produce no side effects.

use async_trait::async_trait;
use std::marker::PhantomData;

use crate::error::EventError;

/// A no-op implementation of any event publishing contract.
///
/// Accepts any event type, discards it silently, and returns `Ok(())`.
/// Use this as the default publisher in generated services so that
/// `Option<Publisher>` is never needed.
///
/// # Example
///
/// ```rust,ignore
/// use backbone_messaging::NoOpPublisher;
///
/// let publisher: NoOpPublisher<MyEvent> = NoOpPublisher::new();
/// publisher.publish(my_event).await?; // does nothing
/// ```
pub struct NoOpPublisher<E> {
    _phantom: PhantomData<E>,
}

impl<E> NoOpPublisher<E> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E> Default for NoOpPublisher<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Synchronous publish trait — the minimal contract for a publisher.
/// Both real and no-op publishers implement this.
#[async_trait]
pub trait Publisher<E: Send + Sync + 'static>: Send + Sync {
    async fn publish(&self, event: E) -> Result<(), EventError>;

    async fn publish_many(&self, events: Vec<E>) -> Result<(), EventError> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<E: Send + Sync + 'static> Publisher<E> for NoOpPublisher<E> {
    async fn publish(&self, _event: E) -> Result<(), EventError> {
        Ok(())
    }

    async fn publish_many(&self, _events: Vec<E>) -> Result<(), EventError> {
        Ok(())
    }
}

// ─── NoOpEventBus ─────────────────────────────────────────────────────────────

/// A non-generic, type-erased no-op event bus.
///
/// Unlike `NoOpPublisher<E>`, this single struct can publish **any** event type
/// without being parameterized.  Use it in contexts where a concrete bus
/// instance is required but no event forwarding is desired.
///
/// This eliminates the `Option<event_bus>` anti-pattern in services — a service
/// can always hold an `Arc<NoOpEventBus>` and call `publish()` unconditionally.
///
/// # Example
///
/// ```rust,ignore
/// use backbone_messaging::NoOpEventBus;
///
/// let bus = NoOpEventBus::new();
/// bus.publish(my_event).await?; // silently discarded
/// ```
pub struct NoOpEventBus;

impl NoOpEventBus {
    pub fn new() -> Self {
        Self
    }

    /// Publish any event — always succeeds, does nothing.
    pub async fn publish<E: Send + 'static>(&self, _event: E) -> Result<(), EventError> {
        Ok(())
    }
}

impl Default for NoOpEventBus {
    fn default() -> Self {
        Self::new()
    }
}

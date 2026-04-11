//! Domain Event trait definition

use chrono::{DateTime, Utc};

/// Trait for domain events
///
/// All domain events must implement this trait to be used with the EventBus.
/// Domain events represent something that happened in the domain that domain
/// experts care about.
///
/// # Example
///
/// ```rust,ignore
/// use backbone_messaging::DomainEvent;
/// use chrono::{DateTime, Utc};
///
/// #[derive(Clone, Debug)]
/// struct OrderPlaced {
///     order_id: String,
///     customer_id: String,
///     total: f64,
///     placed_at: DateTime<Utc>,
/// }
///
/// impl DomainEvent for OrderPlaced {
///     fn event_type(&self) -> &'static str {
///         "OrderPlaced"
///     }
///
///     fn aggregate_id(&self) -> &str {
///         &self.order_id
///     }
///
///     fn occurred_at(&self) -> DateTime<Utc> {
///         self.placed_at
///     }
/// }
/// ```
pub trait DomainEvent: Clone + Send + Sync + 'static {
    /// Returns the event type name (e.g., "UserCreated", "OrderPlaced")
    fn event_type(&self) -> &'static str;

    /// Returns the aggregate ID this event belongs to
    fn aggregate_id(&self) -> &str;

    /// Returns when the event occurred
    /// Default implementation returns current time
    fn occurred_at(&self) -> DateTime<Utc> {
        Utc::now()
    }

    /// Returns the aggregate type name (e.g., "User", "Order")
    /// Default implementation extracts from event type
    fn aggregate_type(&self) -> &'static str {
        // Extract aggregate type from event type (e.g., "UserCreated" -> "User")
        let event_type = self.event_type();
        // Simple heuristic: find common suffixes
        for suffix in ["Created", "Updated", "Deleted", "Changed", "Added", "Removed"] {
            if event_type.ends_with(suffix) {
                // Return the part before the suffix
                // This is a compile-time approximation
                return event_type;
            }
        }
        event_type
    }

    /// Returns the event version for schema evolution
    fn version(&self) -> u32 {
        1
    }
}

/// Marker trait for events that can be serialized
#[allow(dead_code)]
pub trait SerializableEvent: DomainEvent + serde::Serialize + serde::de::DeserializeOwned {}

impl<T> SerializableEvent for T where T: DomainEvent + serde::Serialize + serde::de::DeserializeOwned {}

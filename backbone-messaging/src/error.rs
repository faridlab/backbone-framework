//! Event processing errors

use thiserror::Error;

/// Errors that can occur during event processing
#[derive(Debug, Error)]
pub enum EventError {
    /// Failed to publish an event
    #[error("Failed to publish event: {0}")]
    PublishError(String),

    /// Event handler returned an error
    #[error("Handler '{handler}' failed: {message}")]
    HandlerError {
        handler: String,
        message: String,
    },

    /// Failed to serialize/deserialize event
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Event not found
    #[error("Event not found: {0}")]
    NotFound(String),

    /// Event bus is closed
    #[error("Event bus is closed")]
    BusClosed,

    /// Channel send error
    #[error("Channel send error: {0}")]
    ChannelError(String),

    /// Timeout waiting for event
    #[error("Timeout waiting for event")]
    Timeout,

    /// Invalid event type
    #[error("Invalid event type: {0}")]
    InvalidEventType(String),

    /// Duplicate handler registration
    #[error("Handler already registered: {0}")]
    DuplicateHandler(String),
}

impl EventError {
    /// Create a handler error
    pub fn handler(handler: impl Into<String>, message: impl Into<String>) -> Self {
        Self::HandlerError {
            handler: handler.into(),
            message: message.into(),
        }
    }

    /// Create a publish error
    pub fn publish(message: impl Into<String>) -> Self {
        Self::PublishError(message.into())
    }

    /// Create a serialization error
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::SerializationError(message.into())
    }
}

impl From<tokio::sync::broadcast::error::SendError<String>> for EventError {
    fn from(e: tokio::sync::broadcast::error::SendError<String>) -> Self {
        Self::ChannelError(e.to_string())
    }
}

impl From<serde_json::Error> for EventError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError(e.to_string())
    }
}

//! Queue types and structures

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;

/// Queue message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[derive(Default)]
pub enum QueuePriority {
    /// Low priority (value: 1)
    Low = 1,

    /// Normal priority (value: 5)
    #[default]
    Normal = 5,

    /// High priority (value: 10)
    High = 10,

    /// Critical priority (value: 20)
    Critical = 20,
}


impl std::fmt::Display for QueuePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueuePriority::Low => write!(f, "Low"),
            QueuePriority::Normal => write!(f, "Normal"),
            QueuePriority::High => write!(f, "High"),
            QueuePriority::Critical => write!(f, "Critical"),
        }
    }
}

impl From<i32> for QueuePriority {
    fn from(value: i32) -> Self {
        match value {
            1 => Self::Low,
            5 => Self::Normal,
            10 => Self::High,
            20 => Self::Critical,
            _ => Self::Normal, // Default for unknown values
        }
    }
}

/// Queue message status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    /// Pending to be processed
    Pending,

    /// Currently being processed (invisible)
    Processing,

    /// Successfully processed and acknowledged
    Acknowledged,

    /// Failed processing and returned to queue
    Failed,

    /// Sent to dead letter queue
    DeadLettered,

    /// Deleted without processing
    Deleted,
}

/// Queue message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    /// Unique message identifier
    pub id: String,

    /// Message payload (JSON-serializable data)
    pub payload: serde_json::Value,

    /// Message priority
    pub priority: QueuePriority,

    /// Number of times this message has been received
    pub receive_count: u32,

    /// Maximum receive count before dead letter queue
    pub max_receive_count: u32,

    /// Initial enqueue timestamp
    pub enqueued_at: DateTime<Utc>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Timestamp when message becomes visible
    pub visible_at: DateTime<Utc>,

    /// Timestamp when message expires (optional)
    pub expires_at: Option<DateTime<Utc>>,

    /// Visibility timeout in seconds
    pub visibility_timeout: u64,

    /// Current message status
    pub status: MessageStatus,

    /// Delay before processing (in seconds)
    pub delay_seconds: Option<u64>,

    /// Message attributes/metadata
    pub attributes: HashMap<String, String>,

    /// Message headers (for AMQP compatibility)
    pub headers: HashMap<String, serde_json::Value>,

    /// Message group ID (for FIFO queues)
    pub message_group_id: Option<String>,

    /// Message deduplication ID (for FIFO queues)
    pub message_deduplication_id: Option<String>,

    /// RabbitMQ routing key
    pub routing_key: Option<String>,

    /// Compression flag
    pub compressed: bool,

    /// Original message size before compression
    pub original_size: Option<usize>,
}

impl QueueMessage {
    /// Create new message builder
    pub fn builder() -> QueueMessageBuilder {
        QueueMessageBuilder::new()
    }

    /// Check if message is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if message should be visible now
    pub fn is_visible(&self) -> bool {
        Utc::now() >= self.visible_at
    }

    /// Check if message should go to dead letter queue
    pub fn should_dead_letter(&self) -> bool {
        self.receive_count >= self.max_receive_count
    }

    /// Get message age in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.enqueued_at).num_seconds()
    }

    /// Get remaining visibility time in seconds
    pub fn remaining_visibility(&self) -> i64 {
        (self.visible_at - Utc::now()).num_seconds().max(0)
    }

    /// Mark as received (increment receive count)
    pub fn mark_received(&mut self) {
        self.receive_count += 1;
        self.status = MessageStatus::Processing;
        self.visible_at = Utc::now() + Duration::seconds(self.visibility_timeout as i64);
    }

    /// Mark as acknowledged
    pub fn mark_acknowledged(&mut self) {
        self.status = MessageStatus::Acknowledged;
    }

    /// Mark as failed
    pub fn mark_failed(&mut self) {
        self.status = MessageStatus::Failed;
    }

    /// Mark as dead lettered
    pub fn mark_dead_lettered(&mut self) {
        self.status = MessageStatus::DeadLettered;
    }

    /// Reset for retry (make visible again)
    pub fn reset_for_retry(&mut self, delay_seconds: Option<u64>) {
        self.status = MessageStatus::Pending;
        let delay = delay_seconds.unwrap_or(0);
        self.visible_at = Utc::now() + Duration::seconds(delay as i64);
    }

    /// Validate message
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("Message ID cannot be empty".to_string());
        }

        if self.visibility_timeout == 0 {
            return Err("Visibility timeout must be greater than 0".to_string());
        }

        if self.max_receive_count == 0 {
            return Err("Max receive count must be greater than 0".to_string());
        }

        Ok(())
    }

    /// Get message size in bytes (serialized)
    pub fn size_bytes(&self) -> Result<usize, serde_json::Error> {
        serde_json::to_vec(self).map(|v| v.len())
    }
}

/// Queue message builder
pub struct QueueMessageBuilder {
    message: QueueMessage,
}

impl QueueMessageBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            message: QueueMessage {
                id: Uuid::new_v4().to_string(),
                payload: serde_json::Value::Null,
                priority: QueuePriority::Normal,
                receive_count: 0,
                max_receive_count: crate::DEFAULT_MAX_RECEIVE_COUNT,
                enqueued_at: Utc::now(),
                created_at: Utc::now(),
                visible_at: Utc::now(),
                expires_at: None,
                visibility_timeout: crate::DEFAULT_VISIBILITY_TIMEOUT,
                status: MessageStatus::Pending,
                delay_seconds: None,
                attributes: HashMap::new(),
                headers: HashMap::new(),
                message_group_id: None,
                message_deduplication_id: None,
                routing_key: None,
                compressed: false,
                original_size: None,
            },
        }
    }

    /// Set message ID
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.message.id = id.into();
        self
    }

    /// Set payload
    pub fn payload<T: Serialize>(mut self, payload: T) -> Result<Self, serde_json::Error> {
        self.message.payload = serde_json::to_value(payload)?;
        Ok(self)
    }

    /// Set JSON payload directly
    pub fn json_payload(mut self, payload: serde_json::Value) -> Self {
        self.message.payload = payload;
        self
    }

    /// Set string payload
    pub fn text_payload(mut self, text: impl Into<String>) -> Self {
        self.message.payload = serde_json::Value::String(text.into());
        self
    }

    /// Set priority
    pub fn priority(mut self, priority: QueuePriority) -> Self {
        self.message.priority = priority;
        self
    }

    /// Set max receive count
    pub fn max_receive_count(mut self, count: u32) -> Self {
        self.message.max_receive_count = count;
        self
    }

    /// Set visibility timeout in seconds
    pub fn visibility_timeout(mut self, timeout: u64) -> Self {
        self.message.visibility_timeout = timeout;
        self
    }

    /// Set delay before processing in seconds
    pub fn delay(mut self, seconds: u64) -> Self {
        self.message.delay_seconds = Some(seconds);
        self.message.visible_at = Utc::now() + chrono::Duration::seconds(seconds as i64);
        self
    }

    /// Set expiration time
    pub fn expires_at(mut self, time: DateTime<Utc>) -> Self {
        self.message.expires_at = Some(time);
        self
    }

    /// Set expiration in seconds from now
    pub fn expires_in(mut self, seconds: u64) -> Self {
        self.message.expires_at = Some(Utc::now() + chrono::Duration::seconds(seconds as i64));
        self
    }

    /// Add attribute
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.message.attributes.insert(key.into(), value.into());
        self
    }

    /// Set multiple attributes
    pub fn attributes(mut self, attrs: HashMap<String, String>) -> Self {
        self.message.attributes.extend(attrs);
        self
    }

    /// Set message group ID (for FIFO queues)
    pub fn message_group_id(mut self, group_id: impl Into<String>) -> Self {
        self.message.message_group_id = Some(group_id.into());
        self
    }

    /// Set message deduplication ID (for FIFO queues)
    pub fn message_deduplication_id(mut self, dedup_id: impl Into<String>) -> Self {
        self.message.message_deduplication_id = Some(dedup_id.into());
        self
    }

    /// Set RabbitMQ routing key
    pub fn routing_key(mut self, routing_key: impl Into<String>) -> Self {
        self.message.routing_key = Some(routing_key.into());
        self
    }

    /// Enable compression
    pub fn compress(mut self, enabled: bool) -> Self {
        self.message.compressed = enabled;
        self
    }

    /// Build the queue message
    pub fn build(self) -> QueueMessage {
        self.message
    }
}

impl Default for QueueMessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch receive result
#[derive(Debug, Clone)]
pub struct BatchReceiveResult {
    /// Received messages
    pub messages: Vec<QueueMessage>,

    /// Number of messages requested
    pub requested: usize,

    /// Number of messages available
    pub available: usize,

    /// Total messages in queue
    pub total_in_queue: u64,

    /// Processing statistics
    pub processing_time_ms: u64,
}

/// Queue health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QueueHealth {
    /// Queue is healthy
    Healthy,

    /// Queue is degraded (slow processing)
    Degraded,

    /// Queue is unhealthy (errors, high backlog)
    Unhealthy,
}

impl QueueHealth {
    /// Determine health status based on metrics
    pub fn from_metrics(
        avg_processing_time_ms: f64,
        backlog_size: u64,
        error_rate: f64,
    ) -> Self {
        let processing_ok = avg_processing_time_ms < 5000.0; // 5 seconds
        let backlog_ok = backlog_size < 1000;
        let error_ok = error_rate < 0.05; // 5% error rate

        if processing_ok && backlog_ok && error_ok {
            Self::Healthy
        } else if !processing_ok || backlog_size > 5000 {
            Self::Unhealthy
        } else {
            Self::Degraded
        }
    }
}

/// Queue health check result
#[derive(Debug, Clone)]
pub struct QueueHealthCheck {
    /// Overall health status
    pub status: QueueHealth,

    /// Queue size
    pub queue_size: u64,

    /// Average processing time in milliseconds
    pub avg_processing_time_ms: Option<f64>,

    /// Messages per second
    pub messages_per_second: Option<f64>,

    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,

    /// Last activity timestamp
    pub last_activity: Option<DateTime<Utc>>,

    /// Health check timestamp
    pub checked_at: DateTime<Utc>,

    /// Additional health details
    pub details: HashMap<String, String>,
}
//! Backbone Framework Queue Module
//!
//! Provides queue functionality with Redis and AWS SQS support.
//!
//! ## Features
//!
//! - **Redis Backend**: In-memory queue with Redis backend
//! - **AWS SQS Backend**: Cloud-based queue with Amazon SQS
//! - **Priority Queues**: Support for message prioritization
//! - **Delayed Messages**: Schedule messages for future delivery
//! - **Dead Letter Queues**: Handle failed messages
//! - **Batch Operations**: Process multiple messages efficiently
//! - **Async/Await**: Full async support with tokio
//! - **Message Compression**: Automatic compression for large messages
//!
//! ## Quick Start
//!
//! ```rust
//! use backbone_queue::{QueueService, RedisQueue, QueueMessage};
//!
//! // Redis queue service
//! let queue = RedisQueue::new("redis://localhost:6379", "my_queue").await?;
//!
//! // Send message
//! let message = QueueMessage::builder()
//!     .payload("Hello, World!")
//!     .priority(QueuePriority::Normal)
//!     .build();
//!
//! let message_id = queue.enqueue(message).await?;
//!
//! // Receive message
//! let message = queue.dequeue().await?;
//! if let Some(msg) = message {
//!     println!("Received: {}", msg.payload);
//!     queue.ack(msg.id).await?;
//! }
//! ```

pub mod redis;
pub mod sqs;
// pub mod rabbitmq; // Temporarily disabled due to implementation complexity
pub mod rabbitmq_simple;
pub mod traits;
pub mod types;
pub mod monitoring;
pub mod compression;
pub mod fifo;
pub mod queue_manager;
// pub mod validation; // Temporarily disabled due to struct mismatch
// pub mod validation_utils;
pub mod deduplication;
pub mod processor;

// Include tests
#[cfg(test)]
mod rabbitmq_tests; // RabbitMQ-specific tests

pub use traits::*;
pub use types::*;
pub use redis::*;
pub use sqs::*;
// pub use rabbitmq::*; // Temporarily disabled
pub use rabbitmq_simple::*;
// Export monitoring module items individually to avoid ambiguous re-exports
pub use monitoring::{
    QueueMetrics, QueueMonitorService, AlertEvent, AlertSeverity, AlertThresholds,
    TimestampedValue, AlertCallback, ConsoleAlertCallback,
    WebhookAlertCallback, MetricsReport
};

// Export compression module items
pub use compression::{
    MessageCompressor, CompressionConfig, CompressionAlgorithm, CompressionStats,
    CompressedMessageBuilder, utils as compression_utils
};

// Export FIFO module items
pub use fifo::{
    FifoQueueService, FifoQueueServiceWrapper, FifoQueueConfig, FifoQueueStats,
    MessageGroupStats, utils as fifo_utils, MessageVolume
};

// Export queue manager module items
pub use queue_manager::{
    QueueManager, QueueConfig as QueueManagerConfig, QueueAdminService, MaintenanceAction, MaintenanceResult
};

// Export health check types
pub use types::QueueHealthCheck;

// Export validation module items (temporarily disabled)
// pub use validation::{
//     ConfigValidator, ValidationResult, ValidationError, ValidationWarning,
//     ValidationSeverity, ValidationEnvironment, ErrorHandler
// };

// Export deduplication module items
pub use deduplication::{
    MessageDeduplicator, DeduplicationConfig, DeduplicationStrategy, DeduplicationCache,
    DeduplicationEntry, ProcessingStatus, ExactlyOnceRecord, ProcessingResult,
    DeduplicationStats, DeduplicationCacheBackend, ExactlyOnceStorage,
    MemoryDeduplicationCache, MemoryExactlyOnceStorage
};

// Export processor module items
pub use processor::{
    MessageProcessor, ProcessingOutcome, ProcessedMessage, BatchProcessingResult,
    ProcessingContext, BatchContext, BatchConfig, BatchTimeoutPolicy, RetryConfig,
    RetryPolicy, RetryHandler, ProcessorStats, BatchingProcessor, SimpleMessageProcessor
};


/// Queue module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default queue name
pub const DEFAULT_QUEUE_NAME: &str = "default";

/// Maximum message size (256KB for SQS, 1GB for Redis)
pub const MAX_MESSAGE_SIZE: usize = 256 * 1024;

/// Default visibility timeout (30 seconds)
pub const DEFAULT_VISIBILITY_TIMEOUT: u64 = 30;

/// Maximum receive count before dead letter queue
pub const DEFAULT_MAX_RECEIVE_COUNT: u32 = 5;

/// Queue error types
#[derive(thiserror::Error, Debug)]
pub enum QueueError {
    #[error("Redis connection error: {0}")]
    RedisConnection(String),

    #[error("Redis operation error: {0}")]
    RedisOperation(String),

    #[error("AWS SQS error: {0}")]
    SqsError(String),

    #[error("Message serialization error: {0}")]
    Serialization(String),

    #[error("Message deserialization error: {0}")]
    Deserialization(String),

    #[error("Message too large: {size} bytes (max: {max} bytes)")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Invalid queue name: {0}")]
    InvalidQueueName(String),

    #[error("Invalid message ID: {0}")]
    InvalidMessageId(String),

    #[error("Queue not found: {0}")]
    QueueNotFound(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Publish error: {0}")]
    PublishError(String),

    #[error("Consumer error: {0}")]
    ConsumerError(String),

    #[error("Queue error: {0}")]
    Other(String),
}

/// Result type for queue operations
pub type QueueResult<T> = Result<T, QueueError>;

/// Queue configuration
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Queue name
    pub queue_name: String,

    /// Visibility timeout in seconds
    pub visibility_timeout: u64,

    /// Message retention period in seconds
    pub message_retention_period: Option<u64>,

    /// Maximum receive count before dead letter queue
    pub max_receive_count: u32,

    /// Dead letter queue name (optional)
    pub dead_letter_queue: Option<String>,

    /// Enable message compression
    pub enable_compression: bool,

    /// Compression threshold in bytes
    pub compression_threshold: usize,

    /// Default message priority
    pub default_priority: QueuePriority,

    /// Enable batch operations
    pub enable_batch_operations: bool,

    /// Batch size for receive operations
    pub batch_size: usize,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            queue_name: DEFAULT_QUEUE_NAME.to_string(),
            visibility_timeout: DEFAULT_VISIBILITY_TIMEOUT,
            message_retention_period: None,
            max_receive_count: DEFAULT_MAX_RECEIVE_COUNT,
            dead_letter_queue: None,
            enable_compression: false,
            compression_threshold: 1024,
            default_priority: QueuePriority::Normal,
            enable_batch_operations: true,
            batch_size: 10,
        }
    }
}

/// Queue backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueBackend {
    Redis,
    Sqs,
    RabbitMQ,
}

impl QueueBackend {
    /// Get backend name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Redis => "redis",
            Self::Sqs => "sqs",
            Self::RabbitMQ => "rabbitmq",
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct QueueStats {
    /// Total number of messages in queue
    pub total_messages: u64,

    /// Number of visible messages
    pub visible_messages: u64,

    /// Number of invisible messages (being processed)
    pub invisible_messages: u64,

    /// Number of delayed messages
    pub delayed_messages: u64,

    /// Number of messages in dead letter queue
    pub dead_letter_messages: u64,

    /// Average message processing time in milliseconds
    pub avg_processing_time_ms: Option<f64>,

    /// Messages processed per second
    pub messages_per_second: Option<f64>,

    /// Total messages processed
    pub total_processed: u64,

    /// Total messages failed
    pub total_failed: u64,

    /// Queue age in seconds
    pub queue_age_seconds: Option<u64>,

    /// Backend-specific statistics
    pub backend_stats: std::collections::HashMap<String, serde_json::Value>,
}


impl QueueStats {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.total_processed + self.total_failed;
        if total > 0 {
            self.total_processed as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Calculate failure rate
    pub fn failure_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }
}
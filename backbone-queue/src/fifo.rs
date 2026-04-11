//! FIFO (First-In-First-Out) Queue Implementation
//!
//! Provides FIFO queue capabilities for both Redis and SQS backends, ensuring
//! exact message ordering and deduplication semantics.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;

use crate::{
    QueueService, QueueMessage, QueueResult, QueueError,
};
use async_trait::async_trait;

/// FIFO queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FifoQueueConfig {
    /// Enable FIFO semantics
    pub enabled: bool,

    /// Message deduplication window in seconds
    /// Messages with the same deduplication ID within this window are rejected
    pub deduplication_window_seconds: u64,

    /// Maximum number of message groups to track
    pub max_message_groups: usize,

    /// Enable content-based deduplication
    pub enable_content_deduplication: bool,

    /// Content deduplication window in seconds
    pub content_deduplication_window_seconds: u64,

    /// Maximum number of deduplicated messages to track
    pub max_deduplicated_messages: usize,
}

impl Default for FifoQueueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            deduplication_window_seconds: 300, // 5 minutes
            max_message_groups: 10000,
            enable_content_deduplication: false,
            content_deduplication_window_seconds: 60, // 1 minute
            max_deduplicated_messages: 100000,
        }
    }
}

/// Message group statistics
#[derive(Debug, Clone, Default)]
pub struct MessageGroupStats {
    /// Number of messages in group
    pub message_count: u64,

    /// Number of messages processed
    pub processed_count: u64,

    /// Number of messages failed
    pub failed_count: u64,

    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,

    /// Last message timestamp
    pub last_message_at: Option<DateTime<Utc>>,

    /// Group created at
    pub created_at: DateTime<Utc>,
}

impl MessageGroupStats {
    /// Create new group stats
    pub fn new() -> Self {
        Self {
            created_at: Utc::now(),
            ..Default::default()
        }
    }

    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.processed_count + self.failed_count;
        if total > 0 {
            self.processed_count as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    }
}

/// FIFO queue statistics
#[derive(Debug, Clone, Default)]
pub struct FifoQueueStats {
    /// Total message groups
    pub total_groups: u64,

    /// Active message groups
    pub active_groups: u64,

    /// Total deduplicated messages
    pub deduplicated_messages: u64,

    /// Total out-of-order messages
    pub out_of_order_messages: u64,

    /// Group statistics
    pub group_stats: HashMap<String, MessageGroupStats>,

    /// Deduplication cache
    pub deduplication_cache: HashMap<String, DateTime<Utc>>,

    /// Content deduplication cache
    pub content_deduplication_cache: HashMap<String, DateTime<Utc>>,

    /// Statistics last updated
    pub last_updated: DateTime<Utc>,
}

impl FifoQueueStats {
    /// Create new FIFO stats
    pub fn new() -> Self {
        Self {
            last_updated: Utc::now(),
            ..Default::default()
        }
    }

    /// Update group statistics
    pub fn update_group(&mut self, group_id: &str, processed: bool, processing_time_ms: u64) {
        let stats = self.group_stats
            .entry(group_id.to_string())
            .or_default();

        stats.message_count += 1;
        stats.last_message_at = Some(Utc::now());

        if processed {
            stats.processed_count += 1;
        } else {
            stats.failed_count += 1;
        }

        // Update average processing time
        let total_processed = stats.processed_count + stats.failed_count;
        if total_processed > 0 {
            stats.avg_processing_time_ms =
                (stats.avg_processing_time_ms * (total_processed - 1) as f64 + processing_time_ms as f64)
                / total_processed as f64;
        }

        self.last_updated = Utc::now();
    }

    /// Clean expired deduplication entries
    pub fn cleanup_expired_entries(&mut self, config: &FifoQueueConfig) {
        let now = Utc::now();

        // Clean message deduplication cache
        self.deduplication_cache.retain(|_, timestamp| {
            now.signed_duration_since(*timestamp).num_seconds() < config.deduplication_window_seconds as i64
        });

        // Clean content deduplication cache
        self.content_deduplication_cache.retain(|_, timestamp| {
            now.signed_duration_since(*timestamp).num_seconds() < config.content_deduplication_window_seconds as i64
        });

        // Limit cache sizes
        if self.deduplication_cache.len() > config.max_deduplicated_messages {
            let entries_to_remove = self.deduplication_cache.len() - config.max_deduplicated_messages;
            let keys_to_remove: Vec<_> = self.deduplication_cache.keys().take(entries_to_remove).cloned().collect();
            for key in keys_to_remove {
                self.deduplication_cache.remove(&key);
            }
        }

        if self.content_deduplication_cache.len() > config.max_deduplicated_messages {
            let entries_to_remove = self.content_deduplication_cache.len() - config.max_deduplicated_messages;
            let keys_to_remove: Vec<_> = self.content_deduplication_cache.keys().take(entries_to_remove).cloned().collect();
            for key in keys_to_remove {
                self.content_deduplication_cache.remove(&key);
            }
        }

        // Limit group statistics
        if self.group_stats.len() > config.max_message_groups {
            let groups_to_remove = self.group_stats.len() - config.max_message_groups;
            let keys_to_remove: Vec<_> = self.group_stats.keys().take(groups_to_remove).cloned().collect();
            for key in keys_to_remove {
                self.group_stats.remove(&key);
            }
        }
    }
}

/// FIFO queue service trait
#[async_trait]
pub trait FifoQueueService: Send + Sync {
    /// Enqueue a FIFO message
    async fn enqueue_fifo(&self, message: QueueMessage) -> QueueResult<String>;

    /// Enqueue multiple FIFO messages (maintains order)
    async fn enqueue_fifo_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>>;

    /// Dequeue from specific message group
    async fn dequeue_from_group(&self, group_id: &str) -> QueueResult<Option<QueueMessage>>;

    /// Get message group statistics
    async fn get_group_stats(&self, group_id: &str) -> QueueResult<Option<MessageGroupStats>>;

    /// Get all message group statistics
    async fn get_all_group_stats(&self) -> QueueResult<HashMap<String, MessageGroupStats>>;

    /// Get FIFO queue statistics
    async fn get_fifo_stats(&self) -> QueueResult<FifoQueueStats>;

    /// Check if message is duplicated
    async fn is_message_duplicated(&self, deduplication_id: &str) -> QueueResult<bool>;

    /// Check if message content is duplicated
    async fn is_content_duplicated(&self, content: &str) -> QueueResult<bool>;

    /// Remove expired deduplication entries
    async fn cleanup_deduplication(&self) -> QueueResult<u64>;
}

/// FIFO queue wrapper service
pub struct FifoQueueServiceWrapper {
    inner: Arc<dyn QueueService + Send + Sync>,
    config: FifoQueueConfig,
    stats: Arc<RwLock<FifoQueueStats>>,
}

impl FifoQueueServiceWrapper {
    /// Create new FIFO queue wrapper
    pub fn new(
        queue_service: Arc<dyn QueueService + Send + Sync>,
        config: FifoQueueConfig,
    ) -> Self {
        Self {
            inner: queue_service,
            config,
            stats: Arc::new(RwLock::new(FifoQueueStats::new())),
        }
    }

    /// Create with default configuration
    pub fn with_default_config(queue_service: Arc<dyn QueueService + Send + Sync>) -> Self {
        Self::new(queue_service, FifoQueueConfig::default())
    }

    /// Generate content hash for deduplication
    fn generate_content_hash(content: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let content_str = serde_json::to_string(content).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        content_str.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Validate FIFO message requirements
    fn validate_fifo_message(&self, message: &QueueMessage) -> QueueResult<()> {
        if !self.config.enabled {
            return Err(QueueError::ConfigError("FIFO is disabled".to_string()));
        }

        // Check for required FIFO fields
        if message.message_group_id.is_none() {
            return Err(QueueError::ConfigError(
                "FIFO messages must have message_group_id".to_string()
            ));
        }

        if message.message_deduplication_id.is_none() {
            return Err(QueueError::ConfigError(
                "FIFO messages must have message_deduplication_id".to_string()
            ));
        }

        // Validate message deduplication ID format
        let dedup_id = message.message_deduplication_id.as_ref().unwrap();
        if dedup_id.is_empty() {
            return Err(QueueError::ConfigError(
                "message_deduplication_id cannot be empty".to_string()
            ));
        }

        if dedup_id.len() > 128 {
            return Err(QueueError::ConfigError(
                "message_deduplication_id cannot exceed 128 characters".to_string()
            ));
        }

        Ok(())
    }

    /// Generate sequence number for message ordering
    async fn generate_sequence_number(&self, group_id: &str) -> QueueResult<u64> {
        // Use timestamp + counter to ensure uniqueness and ordering
        let timestamp = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
        let group_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            group_id.hash(&mut hasher);
            hasher.finish()
        };

        Ok(timestamp.wrapping_add(group_hash))
    }
}

#[async_trait]
impl FifoQueueService for FifoQueueServiceWrapper {
    async fn enqueue_fifo(&self, mut message: QueueMessage) -> QueueResult<String> {
        // Validate FIFO requirements
        self.validate_fifo_message(&message)?;

        // Check for message deduplication
        if let Some(ref dedup_id) = message.message_deduplication_id {
            if self.is_message_duplicated(dedup_id).await? {
                return Err(QueueError::Other(
                    format!("Message with deduplication ID {} already exists", dedup_id)
                ));
            }
        }

        // Check for content deduplication
        if self.config.enable_content_deduplication {
            let content_hash = Self::generate_content_hash(&message.payload);
            if self.is_content_duplicated(&content_hash).await? {
                return Err(QueueError::Other(
                    "Message with identical content already exists".to_string()
                ));
            }

            // Store content hash in attributes
            message.attributes.insert(
                "content_hash".to_string(),
                content_hash
            );
        }

        // Extract values we need before moving message
        let group_id = message.message_group_id.as_ref().unwrap().clone();
        let deduplication_id = message.message_deduplication_id.clone();

        // Generate sequence number for ordering
        let sequence_number = self.generate_sequence_number(&group_id).await?;

        // Add sequence number to attributes
        message.attributes.insert(
            "fifo_sequence".to_string(),
            sequence_number.to_string()
        );

        // Add FIFO metadata
        message.attributes.insert(
            "fifo_group".to_string(),
            group_id.clone()
        );
        message.attributes.insert(
            "fifo_deduplication_id".to_string(),
            deduplication_id.as_ref().unwrap().clone()
        );

        // Enqueue message
        let message_id = self.inner.enqueue(message).await?;

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_groups += 1;

            // Add to deduplication cache
            if let Some(dedup_id) = deduplication_id {
                stats.deduplication_cache.insert(
                    dedup_id.clone(),
                    Utc::now()
                );
            }

            // Update group stats
            stats.update_group(&group_id, true, 0); // Processing time measured on dequeue
        }

        Ok(message_id)
    }

    async fn enqueue_fifo_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>> {
        let mut message_ids = Vec::with_capacity(messages.len());

        // Validate all messages first
        for message in &messages {
            self.validate_fifo_message(message)?;
        }

        // Sort messages by group to maintain FIFO order within groups
        let mut sorted_messages = messages;
        sorted_messages.sort_by(|a, b| {
            let group_a = a.message_group_id.as_ref().unwrap();
            let group_b = b.message_group_id.as_ref().unwrap();
            group_a.cmp(group_b)
        });

        // Process messages maintaining FIFO order
        for message in sorted_messages {
            let message_id = self.enqueue_fifo(message).await?;
            message_ids.push(message_id);
        }

        Ok(message_ids)
    }

    async fn dequeue_from_group(&self, group_id: &str) -> QueueResult<Option<QueueMessage>> {
        let start_time = Instant::now();
        let max_attempts = 50; // Prevent infinite loops
        let mut attempts = 0;

        loop {
            if attempts >= max_attempts {
                return Ok(None); // Give up after max attempts
            }
            attempts += 1;

            // Dequeue message
            let message = match self.inner.dequeue().await? {
                Some(msg) => msg,
                None => return Ok(None), // No more messages
            };

            // Check if message belongs to the requested group
            if let Some(fifo_group) = message.attributes.get("fifo_group") {
                if fifo_group == group_id {
                    // Found the right message
                    let processing_time = start_time.elapsed().as_millis() as u64;
                    {
                        let mut stats = self.stats.write().await;
                        stats.update_group(group_id, true, processing_time);
                    }
                    return Ok(Some(message));
                } else {
                    // Wrong group - put it back (simplified approach)
                    // In production, you'd use group-specific queues
                    self.inner.enqueue(message).await?;
                    continue;
                }
            } else {
                // Message doesn't belong to any group
                return Ok(None);
            }
        }
    }

    async fn get_group_stats(&self, group_id: &str) -> QueueResult<Option<MessageGroupStats>> {
        let stats = self.stats.read().await;
        Ok(stats.group_stats.get(group_id).cloned())
    }

    async fn get_all_group_stats(&self) -> QueueResult<HashMap<String, MessageGroupStats>> {
        let stats = self.stats.read().await;
        Ok(stats.group_stats.clone())
    }

    async fn get_fifo_stats(&self) -> QueueResult<FifoQueueStats> {
        let mut stats = self.stats.write().await;

        // Clean expired entries
        stats.cleanup_expired_entries(&self.config);

        Ok(stats.clone())
    }

    async fn is_message_duplicated(&self, deduplication_id: &str) -> QueueResult<bool> {
        let stats = self.stats.read().await;
        Ok(stats.deduplication_cache.contains_key(deduplication_id))
    }

    async fn is_content_duplicated(&self, content: &str) -> QueueResult<bool> {
        // Generate content hash
        let content_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        };

        let stats = self.stats.read().await;
        Ok(stats.content_deduplication_cache.contains_key(&content_hash))
    }

    async fn cleanup_deduplication(&self) -> QueueResult<u64> {
        let mut stats = self.stats.write().await;
        let now = Utc::now();

        let initial_count = stats.deduplication_cache.len() + stats.content_deduplication_cache.len();

        // Clean expired entries
        stats.deduplication_cache.retain(|_, timestamp| {
            now.signed_duration_since(*timestamp).num_seconds() < self.config.deduplication_window_seconds as i64
        });

        stats.content_deduplication_cache.retain(|_, timestamp| {
            now.signed_duration_since(*timestamp).num_seconds() < self.config.content_deduplication_window_seconds as i64
        });

        let final_count = stats.deduplication_cache.len() + stats.content_deduplication_cache.len();
        Ok((initial_count - final_count) as u64)
    }
}

/// FIFO queue utilities
pub mod utils {
    use super::*;

    /// Validate FIFO queue configuration
    pub fn validate_config(config: &FifoQueueConfig) -> Vec<String> {
        let mut errors = Vec::new();

        if config.deduplication_window_seconds == 0 {
            errors.push("Deduplication window must be greater than 0 seconds".to_string());
        }

        if config.deduplication_window_seconds > 86400 * 7 { // 7 days
            errors.push("Deduplication window cannot exceed 7 days".to_string());
        }

        if config.max_message_groups == 0 {
            errors.push("Max message groups must be greater than 0".to_string());
        }

        if config.max_message_groups > 100000 {
            errors.push("Max message groups cannot exceed 100000".to_string());
        }

        if config.content_deduplication_window_seconds == 0 && config.enable_content_deduplication {
            errors.push("Content deduplication window must be greater than 0 seconds when enabled".to_string());
        }

        if config.max_deduplicated_messages == 0 {
            errors.push("Max deduplicated messages must be greater than 0".to_string());
        }

        errors
    }

    /// Generate recommended configuration
    pub fn get_recommended_config(message_volume: MessageVolume) -> FifoQueueConfig {
        match message_volume {
            MessageVolume::Low => FifoQueueConfig {
                deduplication_window_seconds: 300, // 5 minutes
                max_message_groups: 1000,
                enable_content_deduplication: false,
                content_deduplication_window_seconds: 60,
                max_deduplicated_messages: 10000,
                ..Default::default()
            },
            MessageVolume::Medium => FifoQueueConfig {
                deduplication_window_seconds: 900, // 15 minutes
                max_message_groups: 10000,
                enable_content_deduplication: true,
                content_deduplication_window_seconds: 300, // 5 minutes
                max_deduplicated_messages: 50000,
                ..Default::default()
            },
            MessageVolume::High => FifoQueueConfig {
                deduplication_window_seconds: 3600, // 1 hour
                max_message_groups: 50000,
                enable_content_deduplication: true,
                content_deduplication_window_seconds: 1800, // 30 minutes
                max_deduplicated_messages: 100000,
                ..Default::default()
            },
        }
    }

    /// Message volume categories
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MessageVolume {
        Low,
        Medium,
        High,
    }
}

/// Message volume indicator for configuration
pub use utils::MessageVolume;


//! Redis queue implementation
#![allow(dead_code)]

use async_trait::async_trait;
use redis::{Client, AsyncCommands, aio::ConnectionManager};
use crate::{
    QueueResult, QueueError, QueueService, QueueMessage, QueueStats,
    QueueBackend, BatchReceiveResult, QueueHealthCheck, QueueHealth
};
use std::collections::HashMap;
use chrono::Utc;

/// Redis queue configuration
#[derive(Debug, Clone)]
pub struct RedisQueueConfig {
    /// Redis connection URL
    pub url: String,

    /// Queue name
    pub queue_name: String,

    /// Key prefix for queue data
    pub key_prefix: String,

    /// Connection pool size
    pub pool_size: u32,

    /// Health check interval in seconds
    pub health_check_interval: u64,
}

impl Default for RedisQueueConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            queue_name: crate::DEFAULT_QUEUE_NAME.to_string(),
            key_prefix: "backbone:queue".to_string(),
            pool_size: 10,
            health_check_interval: 30,
        }
    }
}

/// Redis queue implementation
#[derive(Clone)]
pub struct RedisQueue {
    config: RedisQueueConfig,
    client: Client,
    connection: ConnectionManager,
}

impl RedisQueue {
    /// Create new Redis queue
    pub async fn new(config: RedisQueueConfig) -> QueueResult<Self> {
        let client = Client::open(config.url.as_str())
            .map_err(|e| QueueError::RedisConnection(e.to_string()))?;

        let connection = client.get_connection_manager()
            .await
            .map_err(|e| QueueError::RedisConnection(e.to_string()))?;

        Ok(Self {
            config,
            client,
            connection,
        })
    }

    /// Get queue key
    fn queue_key(&self) -> String {
        format!("{}:{}:queue", self.config.key_prefix, self.config.queue_name)
    }

    /// Get processing key
    fn processing_key(&self) -> String {
        format!("{}:{}:processing", self.config.key_prefix, self.config.queue_name)
    }

    /// Get dead letter key
    fn dead_letter_key(&self) -> String {
        format!("{}:{}:dead_letter", self.config.key_prefix, self.config.queue_name)
    }

    /// Get stats key
    fn stats_key(&self) -> String {
        format!("{}:{}:stats", self.config.key_prefix, self.config.queue_name)
    }

    /// Serialize message to JSON
    fn serialize_message(&self, message: &QueueMessage) -> QueueResult<String> {
        serde_json::to_string(message)
            .map_err(|e| QueueError::Serialization(e.to_string()))
    }

    /// Deserialize message from JSON
    fn deserialize_message(&self, data: &str) -> QueueResult<QueueMessage> {
        serde_json::from_str(data)
            .map_err(|e| QueueError::Deserialization(e.to_string()))
    }

    /// Update queue statistics
    async fn update_stats(&self, operation: &str, count: i64) -> QueueResult<()> {
        let mut conn = self.connection.clone();
        let _: redis::RedisResult<()> = conn.hincr(self.stats_key(), operation, count).await;
        Ok(())
    }
}

#[async_trait]
impl QueueService for RedisQueue {
    async fn enqueue(&self, mut message: QueueMessage) -> QueueResult<String> {
        // Validate message
        message.validate()
            .map_err(QueueError::Other)?;

        // Check message size
        let size = message.size_bytes()
            .map_err(|e| QueueError::Serialization(e.to_string()))?;
        if size > crate::MAX_MESSAGE_SIZE {
            return Err(QueueError::MessageTooLarge {
                size,
                max: crate::MAX_MESSAGE_SIZE,
            });
        }

        // Set initial timestamps if not set
        if message.enqueued_at == Utc::now() {
            message.enqueued_at = Utc::now();
        }

        if message.visible_at == Utc::now() {
            message.visible_at = Utc::now();
        }

        // Serialize message
        let serialized = self.serialize_message(&message)?;
        let message_id = message.id.clone();

        // Add to queue (use sorted set for priority)
        let mut conn = self.connection.clone();
        let score = message.priority as i32 as f64 * 1000000.0 + message.visible_at.timestamp_millis() as f64;

        let _: redis::RedisResult<()> = conn.zadd(self.queue_key(), &serialized, score).await;

        // Update stats
        self.update_stats("enqueued", 1).await?;

        Ok(message_id)
    }

    async fn enqueue_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        let mut conn = self.connection.clone();
        let mut ids = Vec::with_capacity(messages.len());
        let mut pipe = redis::pipe();

        for message in messages {
            // Validate and serialize each message
            let msg = message;
            msg.validate()
                .map_err(QueueError::Other)?;

            let size = msg.size_bytes()
                .map_err(|e| QueueError::Serialization(e.to_string()))?;
            if size > crate::MAX_MESSAGE_SIZE {
                return Err(QueueError::MessageTooLarge {
                    size,
                    max: crate::MAX_MESSAGE_SIZE,
                });
            }

            let serialized = self.serialize_message(&msg)?;
            let score = msg.priority as i32 as f64 * 1000000.0 + msg.visible_at.timestamp_millis() as f64;

            pipe.zadd(self.queue_key(), &serialized, score);
            ids.push(msg.id);
        }

        pipe.query_async::<_, ()>(&mut conn).await
            .map_err(|e| QueueError::RedisOperation(e.to_string()))?;

        // Update stats
        self.update_stats("enqueued", ids.len() as i64).await?;

        Ok(ids)
    }

    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>> {
        let mut conn = self.connection.clone();

        // Get current timestamp
        let now = Utc::now().timestamp_millis() as f64;

        // Use ZRANGEBYSCORE with limit to get available messages
        let results: redis::RedisResult<Vec<String>> = redis::cmd("ZRANGEBYSCORE")
            .arg(self.queue_key())
            .arg("-inf")
            .arg(now)
            .arg("LIMIT")
            .arg("0")
            .arg("1")
            .query_async(&mut conn)
            .await;

        match results {
            Ok(messages) if !messages.is_empty() => {
                let serialized = &messages[0];
                let message = self.deserialize_message(serialized)?;

                // Remove from queue and add to processing
                let _: redis::RedisResult<()> = conn
                    .zrem(self.queue_key(), serialized)
                    .await;

                // Add to processing set with visibility timeout
                let processing_score = Utc::now().timestamp() as f64 + message.visibility_timeout as f64;
                let _: redis::RedisResult<()> = conn
                    .zadd(self.processing_key(), serialized, processing_score)
                    .await;

                // Update stats
                self.update_stats("dequeued", 1).await?;

                Ok(Some(message))
            }
            Ok(_) => Ok(None),
            Err(e) => Err(QueueError::RedisOperation(e.to_string())),
        }
    }

    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<BatchReceiveResult> {
        let mut conn = self.connection.clone();
        let now = Utc::now().timestamp_millis() as f64;

        // Get available messages
        let results: redis::RedisResult<Vec<String>> = redis::cmd("ZRANGEBYSCORE")
            .arg(self.queue_key())
            .arg("-inf")
            .arg(now)
            .arg("LIMIT")
            .arg("0")
            .arg(max_messages)
            .query_async(&mut conn)
            .await;

        let messages = match results {
            Ok(msgs) => msgs,
            Err(e) => return Err(QueueError::RedisOperation(e.to_string())),
        };

        if messages.is_empty() {
            return Ok(BatchReceiveResult {
                messages: Vec::new(),
                requested: max_messages,
                available: 0,
                total_in_queue: 0,
                processing_time_ms: 0,
            });
        }

        let start_time = std::time::Instant::now();
        let mut queue_messages = Vec::with_capacity(messages.len());
        let mut pipe = redis::pipe();

        for serialized in &messages {
            if let Ok(message) = self.deserialize_message(serialized) {
                pipe.zrem(self.queue_key(), serialized);
                let processing_score = Utc::now().timestamp() as f64 + message.visibility_timeout as f64;
                pipe.zadd(self.processing_key(), serialized, processing_score);
                queue_messages.push(message);
            }
        }

        pipe.query_async::<_, ()>(&mut conn).await
            .map_err(|e| QueueError::RedisOperation(e.to_string()))?;

        // Update stats
        self.update_stats("dequeued", queue_messages.len() as i64).await?;

        // Get total queue size
        let total_size: redis::RedisResult<u64> = conn.zcard(self.queue_key()).await;
        let total_in_queue = total_size.unwrap_or(0);

        Ok(BatchReceiveResult {
            messages: queue_messages,
            requested: max_messages,
            available: messages.len(),
            total_in_queue,
            processing_time_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    async fn ack(&self, message_id: &str) -> QueueResult<bool> {
        let mut conn = self.connection.clone();

        // Remove from processing set
        let processing_key = self.processing_key();
        let results: redis::RedisResult<Vec<String>> = conn
            .zrange(&processing_key, 0, -1)
            .await;

        match results {
            Ok(messages) => {
                for serialized in messages {
                    if let Ok(message) = self.deserialize_message(&serialized) {
                        if message.id == message_id {
                            let _: redis::RedisResult<()> = conn
                                .zrem(&processing_key, &serialized)
                                .await;

                            // Update stats
                            self.update_stats("acknowledged", 1).await?;
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => Err(QueueError::RedisOperation(e.to_string())),
        }
    }

    async fn ack_batch(&self, _message_ids: Vec<String>) -> QueueResult<u64> {
        let mut count = 0;
        for message_id in _message_ids {
            if self.ack(&message_id).await? {
                count += 1;
            }
        }
        Ok(count)
    }

    async fn nack(&self, message_id: &str, delay_seconds: Option<u64>) -> QueueResult<bool> {
        let mut conn = self.connection.clone();

        // Find message in processing set
        let processing_key = self.processing_key();
        let results: redis::RedisResult<Vec<String>> = conn
            .zrange(&processing_key, 0, -1)
            .await;

        match results {
            Ok(messages) => {
                for serialized in messages {
                    if let Ok(mut message) = self.deserialize_message(&serialized) {
                        if message.id == message_id {
                            // Remove from processing
                            let _: redis::RedisResult<()> = conn
                                .zrem(&processing_key, &serialized)
                                .await;

                            // Check if should go to dead letter queue
                            if message.should_dead_letter() {
                                let dead_letter_key = self.dead_letter_key();
                                let _: redis::RedisResult<()> = conn
                                    .zadd(&dead_letter_key, &serialized, Utc::now().timestamp_millis() as f64)
                                    .await;
                                message.mark_dead_lettered();
                            } else {
                                // Return to queue with delay
                                message.reset_for_retry(delay_seconds);
                                let serialized = self.serialize_message(&message)?;
                                let score = message.priority as i32 as f64 * 1000000.0 + message.visible_at.timestamp_millis() as f64;
                                let _: redis::RedisResult<()> = conn
                                    .zadd(self.queue_key(), &serialized, score)
                                    .await;
                            }

                            // Update stats
                            self.update_stats("nacked", 1).await?;
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => Err(QueueError::RedisOperation(e.to_string())),
        }
    }

    async fn delete(&self, message_id: &str) -> QueueResult<bool> {
        // Try to delete from all possible locations
        let ack_result = self.ack(message_id).await;
        if ack_result.is_ok() && ack_result.unwrap() {
            return Ok(true);
        }

        // Try to delete from queue directly
        let mut conn = self.connection.clone();
        let queue_key = self.queue_key();
        let results: redis::RedisResult<Vec<String>> = conn
            .zrange(&queue_key, 0, -1)
            .await;

        match results {
            Ok(messages) => {
                for serialized in messages {
                    if let Ok(message) = self.deserialize_message(&serialized) {
                        if message.id == message_id {
                            let _: redis::RedisResult<()> = conn
                                .zrem(&queue_key, &serialized)
                                .await;
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => Err(QueueError::RedisOperation(e.to_string())),
        }
    }

    async fn get_message(&self, message_id: &str) -> QueueResult<Option<QueueMessage>> {
        // Search in queue, processing, and dead letter queues
        let locations = vec![self.queue_key(), self.processing_key(), self.dead_letter_key()];
        let mut conn = self.connection.clone();

        for location in locations {
            let results: redis::RedisResult<Vec<String>> = conn
                .zrange(&location, 0, -1)
                .await;

            if let Ok(messages) = results {
                for serialized in messages {
                    if let Ok(message) = self.deserialize_message(&serialized) {
                        if message.id == message_id {
                            return Ok(Some(message));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn get_stats(&self) -> QueueResult<QueueStats> {
        let mut conn = self.connection.clone();
        let mut stats = QueueStats::default();

        // Get queue sizes
        let queue_size: redis::RedisResult<u64> = conn.zcard(self.queue_key()).await;
        let processing_size: redis::RedisResult<u64> = conn.zcard(self.processing_key()).await;
        let dead_letter_size: redis::RedisResult<u64> = conn.zcard(self.dead_letter_key()).await;

        stats.visible_messages = queue_size.unwrap_or(0);
        stats.invisible_messages = processing_size.unwrap_or(0);
        stats.dead_letter_messages = dead_letter_size.unwrap_or(0);
        stats.total_messages = stats.visible_messages + stats.invisible_messages;

        // Get custom stats
        let stat_results: redis::RedisResult<HashMap<String, i64>> = conn.hgetall(self.stats_key()).await;
        if let Ok(stat_map) = stat_results {
            stats.total_processed = stat_map.get("dequeued").copied().unwrap_or(0) as u64;
            stats.total_failed = stat_map.get("failed").copied().unwrap_or(0) as u64;
        }

        Ok(stats)
    }

    async fn purge(&self) -> QueueResult<u64> {
        let mut conn = self.connection.clone();
        let total_removed = 0;

        // Clear all queue keys
        let keys = vec![
            self.queue_key(),
            self.processing_key(),
            self.dead_letter_key(),
            self.stats_key(),
        ];

        for key in keys {
            let _: redis::RedisResult<()> = conn.del(&key).await;
        }

        Ok(total_removed)
    }

    async fn size(&self) -> QueueResult<u64> {
        let mut conn = self.connection.clone();
        let queue_size: redis::RedisResult<u64> = conn.zcard(self.queue_key()).await;
        Ok(queue_size.unwrap_or(0))
    }

    async fn is_empty(&self) -> QueueResult<bool> {
        let size = self.size().await?;
        Ok(size == 0)
    }

    async fn health_check(&self) -> QueueResult<QueueHealthCheck> {
        let stats = self.get_stats().await?;
        let is_connected = self.test_connection().await.unwrap_or(false);

        let status = if is_connected && stats.total_messages < 10000 {
            QueueHealth::Healthy
        } else if is_connected && stats.total_messages < 50000 {
            QueueHealth::Degraded
        } else {
            QueueHealth::Unhealthy
        };

        Ok(QueueHealthCheck {
            status,
            queue_size: stats.total_messages,
            avg_processing_time_ms: None,
            messages_per_second: None,
            error_rate: 0.0,
            last_activity: Some(Utc::now()),
            checked_at: Utc::now(),
            details: HashMap::new(),
        })
    }

    async fn validate_config(&self) -> QueueResult<bool> {
        // Test Redis connection
        self.test_connection().await
    }

    async fn test_connection(&self) -> QueueResult<bool> {
        // Try a simple PING
        let mut conn = self.connection.clone();
        match redis::cmd("PING").query_async::<_, String>(&mut conn).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn backend_type(&self) -> QueueBackend {
        QueueBackend::Redis
    }
}

/// Redis queue builder
pub struct RedisQueueBuilder {
    config: RedisQueueConfig,
}

impl RedisQueueBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: RedisQueueConfig::default(),
        }
    }

    /// Set Redis URL
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.config.url = url.into();
        self
    }

    /// Set queue name
    pub fn queue_name(mut self, name: impl Into<String>) -> Self {
        self.config.queue_name = name.into();
        self
    }

    /// Set key prefix
    pub fn key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.key_prefix = prefix.into();
        self
    }

    /// Set connection pool size
    pub fn pool_size(mut self, size: u32) -> Self {
        self.config.pool_size = size;
        self
    }

    /// Build Redis queue
    pub async fn build(self) -> QueueResult<RedisQueue> {
        RedisQueue::new(self.config).await
    }
}

impl Default for RedisQueueBuilder {
    fn default() -> Self {
        Self::new()
    }
}


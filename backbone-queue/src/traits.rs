//! Queue service traits

use async_trait::async_trait;
use crate::{
    QueueResult, QueueMessage, QueueStats, QueueConfig,
    QueueHealthCheck, BatchReceiveResult, QueueBackend
};
use std::collections::HashMap;

/// Generic queue service trait
#[async_trait]
pub trait QueueService: Send + Sync {
    /// Enqueue a message
    async fn enqueue(&self, message: QueueMessage) -> QueueResult<String>;

    /// Enqueue multiple messages
    async fn enqueue_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>> {
        let mut ids = Vec::with_capacity(messages.len());
        for message in messages {
            ids.push(self.enqueue(message).await?);
        }
        Ok(ids)
    }

    /// Dequeue a message
    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>>;

    /// Dequeue multiple messages
    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<BatchReceiveResult>;

    /// Acknowledge a message (mark as processed)
    async fn ack(&self, message_id: &str) -> QueueResult<bool>;

    /// Acknowledge multiple messages
    async fn ack_batch(&self, message_ids: Vec<String>) -> QueueResult<u64>;

    /// Nack/negative acknowledge a message (return to queue)
    async fn nack(&self, message_id: &str, delay_seconds: Option<u64>) -> QueueResult<bool>;

    /// Delete a message
    async fn delete(&self, message_id: &str) -> QueueResult<bool>;

    /// Delete multiple messages
    async fn delete_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<u64> {
        let mut count = 0;
        for message in messages {
            if self.delete(&message.id).await? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Get message by ID
    async fn get_message(&self, message_id: &str) -> QueueResult<Option<QueueMessage>>;

    /// Get queue statistics
    async fn get_stats(&self) -> QueueResult<QueueStats>;

    /// Purge all messages from queue
    async fn purge(&self) -> QueueResult<u64>;

    /// Get queue size (number of messages)
    async fn size(&self) -> QueueResult<u64>;

    /// Check if queue is empty
    async fn is_empty(&self) -> QueueResult<bool>;

    /// Get queue health status
    async fn health_check(&self) -> QueueResult<QueueHealthCheck>;

    /// Validate queue configuration
    async fn validate_config(&self) -> QueueResult<bool>;

    /// Test queue connection
    async fn test_connection(&self) -> QueueResult<bool>;

    /// Get backend type
    fn backend_type(&self) -> QueueBackend;
}

/// Queue management trait for administrative operations
#[async_trait]
pub trait QueueManager: Send + Sync {
    /// Create a new queue
    async fn create_queue(&self, config: QueueConfig) -> QueueResult<bool>;

    /// Delete a queue
    async fn delete_queue(&self, queue_name: &str) -> QueueResult<bool>;

    /// List all queues
    async fn list_queues(&self) -> QueueResult<Vec<String>>;

    /// Get queue configuration
    async fn get_queue_config(&self, queue_name: &str) -> QueueResult<Option<QueueConfig>>;

    /// Update queue configuration
    async fn update_queue_config(&self, queue_name: &str, config: QueueConfig) -> QueueResult<bool>;

    /// Pause message processing for a queue
    async fn pause_queue(&self, queue_name: &str) -> QueueResult<bool>;

    /// Resume message processing for a queue
    async fn resume_queue(&self, queue_name: &str) -> QueueResult<bool>;

    /// Get queue health status
    async fn get_queue_health(&self, queue_name: &str) -> QueueResult<QueueHealthCheck>;
}

/// Message processing trait
#[async_trait]
pub trait MessageProcessor: Send + Sync {
    /// Process a single message
    async fn process_message(&self, message: QueueMessage) -> QueueResult<bool>;

    /// Process multiple messages
    async fn process_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<bool>> {
        let mut results = Vec::with_capacity(messages.len());
        for message in messages {
            results.push(self.process_message(message).await?);
        }
        Ok(results)
    }

    /// Get processor name
    fn name(&self) -> &str;

    /// Get processor version
    fn version(&self) -> &str;
}

/// Queue monitoring trait
#[async_trait]
pub trait QueueMonitor: Send + Sync {
    /// Start monitoring
    async fn start_monitoring(&self, queue_name: &str) -> QueueResult<()>;

    /// Stop monitoring
    async fn stop_monitoring(&self, queue_name: &str) -> QueueResult<()>;

    /// Get real-time metrics
    async fn get_metrics(&self, queue_name: &str) -> QueueResult<HashMap<String, f64>>;

    /// Set alert thresholds
    async fn set_alert_thresholds(&self, queue_name: &str, thresholds: HashMap<String, f64>) -> QueueResult<()>;

    /// Get alert status
    async fn get_alerts(&self, queue_name: &str) -> QueueResult<Vec<String>>;
}
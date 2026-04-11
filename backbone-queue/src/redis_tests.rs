//! Unit tests for Redis queue implementation

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::types::{QueueMessage, QueuePriority, MessageStatus};
    use crate::redis::{RedisQueue, RedisQueueConfig, RedisQueueBuilder};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio_test;
    use uuid::Uuid;

    /// Create test Redis queue configuration
    fn create_test_config() -> RedisQueueConfig {
        RedisQueueConfig {
            url: "redis://localhost:6379".to_string(),
            queue_name: format!("test_queue_{}", uuid::Uuid::new_v4().to_string().replace("-", "")),
            key_prefix: "test".to_string(),
            pool_size: 1,
            health_check_interval: 1,
        }
    }

    /// Create test queue message
    fn create_test_message() -> QueueMessage {
        QueueMessage::builder()
            .text_payload("Test message payload")
            .priority(QueuePriority::Normal)
            .visibility_timeout(30)
            .max_receive_count(3)
            .build()
    }

    /// Create test message with priority
    fn create_test_message_with_priority(priority: QueuePriority) -> QueueMessage {
        QueueMessage::builder()
            .text_payload(format!("Test message with {:?} priority", priority))
            .priority(priority)
            .visibility_timeout(30)
            .max_receive_count(3)
            .build()
    }

    /// Set up test queue and return queue instance
    async fn setup_test_queue() -> QueueResult<RedisQueue> {
        let config = create_test_config();
        let queue = RedisQueue::new(config).await;

        // Clear any existing test data
        if let Ok(ref q) = queue {
            q.purge().await?;
        }

        queue
    }

    #[tokio::test]
    async fn test_redis_queue_creation() -> QueueResult<()> {
        let config = create_test_config();
        let queue = RedisQueue::new(config).await?;

        // Verify backend type
        assert_eq!(queue.backend_type(), QueueBackend::Redis);

        // Test connection
        assert!(queue.test_connection().await?);

        // Validate configuration
        assert!(queue.validate_config().await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_enqueue_single_message() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = create_test_message();

        // Enqueue message
        let message_id = queue.enqueue(message.clone()).await?;

        // Verify message ID is returned
        assert_eq!(message_id, message.id);

        // Check queue size
        let size = queue.size().await?;
        assert_eq!(size, 1);

        // Verify queue is not empty
        assert!(!queue.is_empty().await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_enqueue_batch_messages() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        let messages = vec![
            create_test_message_with_priority(QueuePriority::Low),
            create_test_message_with_priority(QueuePriority::Normal),
            create_test_message_with_priority(QueuePriority::High),
            create_test_message_with_priority(QueuePriority::Critical),
        ];

        // Enqueue messages in batch
        let message_ids = queue.enqueue_batch(messages.clone()).await?;

        // Verify all message IDs are returned
        assert_eq!(message_ids.len(), messages.len());
        for (i, id) in message_ids.iter().enumerate() {
            assert_eq!(id, &messages[i].id);
        }

        // Check queue size
        let size = queue.size().await?;
        assert_eq!(size, 4);

        Ok(())
    }

    #[tokio::test]
    async fn test_dequeue_message_priority_order() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Enqueue messages with different priorities
        let messages = vec![
            create_test_message_with_priority(QueuePriority::Low),
            create_test_message_with_priority(QueuePriority::Critical),
            create_test_message_with_priority(QueuePriority::Normal),
            create_test_message_with_priority(QueuePriority::High),
        ];

        queue.enqueue_batch(messages).await?;

        // Dequeue messages and verify priority order
        let mut received_priorities = Vec::new();

        for _ in 0..4 {
            if let Some(message) = queue.dequeue().await? {
                received_priorities.push(message.priority);
            }
        }

        // Should be in priority order: Critical, High, Normal, Low
        assert_eq!(received_priorities, vec![
            QueuePriority::Critical,
            QueuePriority::High,
            QueuePriority::Normal,
            QueuePriority::Low,
        ]);

        // Queue should be empty now
        assert!(queue.is_empty().await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_dequeue_batch_messages() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Enqueue 5 messages
        let mut messages = Vec::new();
        for i in 0..5 {
            messages.push(QueueMessage::builder()
                .text_payload(format!("Message {}", i))
                .priority(QueuePriority::Normal)
                .build());
        }

        queue.enqueue_batch(messages).await?;

        // Dequeue batch of 3 messages
        let batch_result = queue.dequeue_batch(3).await?;

        assert_eq!(batch_result.messages.len(), 3);
        assert_eq!(batch_result.requested, 3);
        assert_eq!(batch_result.available, 3);
        assert_eq!(batch_result.total_in_queue, 2); // 2 remaining in queue

        // Dequeue remaining messages
        let batch_result2 = queue.dequeue_batch(10).await?;
        assert_eq!(batch_result2.messages.len(), 2);
        assert_eq!(batch_result2.total_in_queue, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_acknowledgment() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = create_test_message();

        queue.enqueue(message.clone()).await?;

        // Dequeue message
        let dequeued = queue.dequeue().await?;
        assert!(dequeued.is_some());

        // Acknowledge message
        let ack_result = queue.ack(&message.id).await?;
        assert!(ack_result);

        // Queue should be empty (message was acknowledged)
        assert!(queue.is_empty().await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_negative_acknowledgment() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = create_test_message();

        queue.enqueue(message.clone()).await?;

        // Dequeue message
        let dequeued = queue.dequeue().await?;
        assert!(dequeued.is_some());

        // Negative acknowledge with delay
        let nack_result = queue.nack(&message.id, Some(5)).await?;
        assert!(nack_result);

        // Message should not be immediately visible (due to delay)
        tokio::time::sleep(Duration::from_millis(100)).await;
        let immediate_dequeue = queue.dequeue().await?;
        assert!(immediate_dequeue.is_none());

        // Wait for delay and retry
        tokio::time::sleep(Duration::from_secs(6)).await;
        let delayed_dequeue = queue.dequeue().await?;
        assert!(delayed_dequeue.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_message_delete() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = create_test_message();

        queue.enqueue(message.clone()).await?;

        // Delete message directly from queue
        let delete_result = queue.delete(&message.id).await?;
        assert!(delete_result);

        // Queue should be empty
        assert!(queue.is_empty().await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_dead_letter_queue() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = QueueMessage::builder()
            .text_payload("Test dead letter")
            .max_receive_count(2) // Low threshold for testing
            .visibility_timeout(1)
            .build();

        queue.enqueue(message.clone()).await?;

        // Receive message twice to exceed max_receive_count
        for _ in 0..2 {
            let dequeued = queue.dequeue().await?;
            assert!(dequeued.is_some());

            // Negative acknowledge to return to queue
            queue.nack(&message.id, None).await?;
        }

        // Third time should send to dead letter queue
        let dequeued = queue.dequeue().await?;
        assert!(dequeued.is_some());
        queue.nack(&message.id, None).await?;

        // Message should not be in regular queue anymore
        let final_dequeue = queue.dequeue().await?;
        assert!(final_dequeue.is_none());

        // But should be retrievable via get_message (from dead letter queue)
        let dead_message = queue.get_message(&message.id).await?;
        assert!(dead_message.is_some());
        assert_eq!(dead_message.unwrap().status, MessageStatus::DeadLettered);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_visibility_timeout() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = QueueMessage::builder()
            .text_payload("Test visibility timeout")
            .visibility_timeout(1) // 1 second timeout
            .build();

        queue.enqueue(message.clone()).await?;

        // Dequeue message (should become invisible)
        let dequeued = queue.dequeue().await?;
        assert!(dequeued.is_some());

        // Immediately try to dequeue again (should not be available)
        let immediate_dequeue = queue.dequeue().await?;
        assert!(immediate_dequeue.is_none());

        // Wait for visibility timeout to expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should be available again
        let delayed_dequeue = queue.dequeue().await?;
        assert!(delayed_dequeue.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_message_by_id() -> QueueResult<()> {
        let queue = setup_test_queue().await?;
        let message = create_test_message();

        queue.enqueue(message.clone()).await?;

        // Get message by ID
        let retrieved = queue.get_message(&message.id).await?;
        assert!(retrieved.is_some());

        let retrieved_message = retrieved.unwrap();
        assert_eq!(retrieved_message.id, message.id);
        assert_eq!(retrieved_message.payload, message.payload);

        // Try non-existent message
        let not_found = queue.get_message("non_existent_id").await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_statistics() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Initial stats
        let stats = queue.get_stats().await?;
        assert_eq!(stats.visible_messages, 0);
        assert_eq!(stats.invisible_messages, 0);
        assert_eq!(stats.total_messages, 0);

        // Enqueue some messages
        queue.enqueue_batch(vec![
            create_test_message(),
            create_test_message(),
            create_test_message(),
        ]).await?;

        // Stats after enqueue
        let stats = queue.get_stats().await?;
        assert_eq!(stats.visible_messages, 3);
        assert_eq!(stats.total_messages, 3);

        // Dequeue one message
        queue.dequeue().await?;

        // Stats after dequeue
        let stats = queue.get_stats().await?;
        assert_eq!(stats.visible_messages, 2);
        assert_eq!(stats.invisible_messages, 1);
        assert_eq!(stats.total_messages, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_purge() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Add some messages
        queue.enqueue_batch(vec![
            create_test_message(),
            create_test_message(),
            create_test_message(),
        ]).await?;

        // Verify queue has messages
        assert!(!queue.is_empty().await?);

        // Purge queue
        let purged_count = queue.purge().await?;

        // Verify queue is empty
        assert!(queue.is_empty().await?);
        assert_eq!(queue.size().await?, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_health_check() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Get health status
        let health = queue.health_check().await?;

        // Should be healthy (Redis connection works, small queue)
        assert_eq!(health.status, QueueHealth::Healthy);
        assert_eq!(health.queue_size, 0);
        assert!(health.error_rate < 0.1);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_attributes() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        let mut attributes = HashMap::new();
        attributes.insert("source".to_string(), "test".to_string());
        attributes.insert("version".to_string(), "1.0".to_string());

        let message = QueueMessage::builder()
            .text_payload("Message with attributes")
            .attributes(attributes.clone())
            .message_group_id("test-group".to_string())
            .message_deduplication_id("test-dedup-123".to_string())
            .build();

        let message_id = queue.enqueue(message.clone()).await?;

        // Retrieve message and verify attributes
        let retrieved = queue.get_message(&message_id).await?;
        assert!(retrieved.is_some());

        let retrieved_message = retrieved.unwrap();
        assert_eq!(retrieved_message.attributes, attributes);
        assert_eq!(retrieved_message.message_group_id, Some("test-group".to_string()));
        assert_eq!(retrieved_message.message_deduplication_id, Some("test-dedup-123".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_message_builder() -> QueueResult<()> {
        // Test all builder methods
        let message = QueueMessage::builder()
            .id("test-id-123")
            .text_payload("Builder test message")
            .priority(QueuePriority::High)
            .max_receive_count(5)
            .visibility_timeout(60)
            .delay(10)
            .expires_in(3600)
            .attribute("test_key", "test_value")
            .message_group_id("group-123")
            .message_deduplication_id("dedup-123")
            .compress(true)
            .build();

        assert_eq!(message.id, "test-id-123");
        assert_eq!(message.priority, QueuePriority::High);
        assert_eq!(message.max_receive_count, 5);
        assert_eq!(message.visibility_timeout, 60);
        assert_eq!(message.delay_seconds, Some(10));
        assert!(message.expires_at.is_some());
        assert!(message.attributes.contains_key("test_key"));
        assert_eq!(message.message_group_id, Some("group-123".to_string()));
        assert_eq!(message.message_deduplication_id, Some("dedup-123".to_string()));
        assert!(message.compressed);

        // Test message validation
        assert!(message.validate().is_ok());

        // Test message size calculation
        let size = message.size_bytes();
        assert!(size.is_ok());
        assert!(size.unwrap() > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_validation() -> QueueResult<()> {
        // Test valid message
        let valid_message = create_test_message();
        assert!(valid_message.validate().is_ok());

        // Test invalid message (empty ID)
        let mut invalid_message = create_test_message();
        invalid_message.id = "".to_string();
        assert!(invalid_message.validate().is_err());

        // Test invalid message (zero visibility timeout)
        let mut invalid_message = create_test_message();
        invalid_message.visibility_timeout = 0;
        assert!(invalid_message.validate().is_err());

        // Test invalid message (zero max receive count)
        let mut invalid_message = create_test_message();
        invalid_message.max_receive_count = 0;
        assert!(invalid_message.validate().is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_message_lifecycle() -> QueueResult<()> {
        let mut message = create_test_message();

        // Test initial state
        assert_eq!(message.status, MessageStatus::Pending);
        assert_eq!(message.receive_count, 0);

        // Test mark as received
        message.mark_received();
        assert_eq!(message.status, MessageStatus::Processing);
        assert_eq!(message.receive_count, 1);
        assert!(message.visible_at > chrono::Utc::now());

        // Test mark as acknowledged
        message.mark_acknowledged();
        assert_eq!(message.status, MessageStatus::Acknowledged);

        // Test reset for retry
        message.reset_for_retry(Some(5));
        assert_eq!(message.status, MessageStatus::Pending);
        assert!(message.visible_at > chrono::Utc::now());

        // Test mark as failed
        message.mark_failed();
        assert_eq!(message.status, MessageStatus::Failed);

        // Test mark as dead lettered
        message.mark_dead_lettered();
        assert_eq!(message.status, MessageStatus::DeadLettered);

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_priority_from_i32() -> QueueResult<()> {
        // Test conversion from i32 values
        assert_eq!(QueuePriority::from(1), QueuePriority::Low);
        assert_eq!(QueuePriority::from(5), QueuePriority::Normal);
        assert_eq!(QueuePriority::from(10), QueuePriority::High);
        assert_eq!(QueuePriority::from(20), QueuePriority::Critical);

        // Test default for unknown values
        assert_eq!(QueuePriority::from(999), QueuePriority::Normal);
        assert_eq!(QueuePriority::from(-1), QueuePriority::Normal);

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_priority_display() -> QueueResult<()> {
        assert_eq!(QueuePriority::Low.to_string(), "Low");
        assert_eq!(QueuePriority::Normal.to_string(), "Normal");
        assert_eq!(QueuePriority::High.to_string(), "High");
        assert_eq!(QueuePriority::Critical.to_string(), "Critical");

        Ok(())
    }

    #[tokio::test]
    async fn test_redis_queue_builder() -> QueueResult<()> {
        let queue = RedisQueueBuilder::new()
            .url("redis://localhost:6379")
            .queue_name("test_builder_queue")
            .key_prefix("builder_test")
            .pool_size(5)
            .build()
            .await?;

        // Verify configuration
        assert_eq!(queue.config.queue_name, "test_builder_queue");
        assert_eq!(queue.config.key_prefix, "builder_test");
        assert_eq!(queue.config.pool_size, 5);

        // Test functionality
        assert!(queue.test_connection().await?);

        // Cleanup
        queue.purge().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_large_message_handling() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Create a large message payload
        let large_payload = "x".repeat(100000); // 100KB
        let large_message = QueueMessage::builder()
            .text_payload(large_payload)
            .build();

        // Should succeed (within reasonable limits)
        let message_id = queue.enqueue(large_message).await?;
        assert!(!message_id.is_empty());

        // Retrieve and verify
        let retrieved = queue.get_message(&message_id).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().payload.as_str().unwrap().len(), 100000);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_operations() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Enqueue messages concurrently
        let mut handles = Vec::new();

        for i in 0..10 {
            let queue_clone = queue.clone();
            let handle = tokio::spawn(async move {
                let message = QueueMessage::builder()
                    .text_payload(format!("Concurrent message {}", i))
                    .build();

                queue_clone.enqueue(message).await
            });
            handles.push(handle);
        }

        // Wait for all enqueues to complete
        for handle in handles {
            let _ = handle.await??;
        }

        // Verify all messages were enqueued
        let size = queue.size().await?;
        assert_eq!(size, 10);

        // Dequeue messages concurrently
        let mut handles = Vec::new();

        for _ in 0..5 {
            let queue_clone = queue.clone();
            let handle = tokio::spawn(async move {
                queue_clone.dequeue().await
            });
            handles.push(handle);
        }

        // Wait for dequeues to complete
        let mut dequeued_count = 0;
        for handle in handles {
            if handle.await?.is_some() {
                dequeued_count += 1;
            }
        }

        assert_eq!(dequeued_count, 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_error_handling() -> QueueResult<()> {
        let queue = setup_test_queue().await?;

        // Test ack non-existent message
        let ack_result = queue.ack("non_existent_id").await?;
        assert!(!ack_result);

        // Test nack non-existent message
        let nack_result = queue.nack("non_existent_id", None).await?;
        assert!(!nack_result);

        // Test delete non-existent message
        let delete_result = queue.delete("non_existent_id").await?;
        assert!(!delete_result);

        Ok(())
    }
}
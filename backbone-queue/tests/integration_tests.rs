//! Integration tests for Backbone Queue Module

use backbone_queue::{
    QueueService, QueueMessage, QueuePriority, QueueBackend, MessageStatus, QueueHealth,
    redis::{RedisQueue, RedisQueueBuilder},
    sqs::{SqsQueue, SqsQueueBuilder},
};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

/// Test configuration
struct TestConfig {
    redis_url: String,
    sqs_region: String,
    sqs_queue_url: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            redis_url: std::env::var("REDIS_TEST_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            sqs_region: std::env::var("AWS_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string()),
            sqs_queue_url: std::env::var("SQS_QUEUE_URL")
                .unwrap_or_else(|_| "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue".to_string()),
        }
    }
}

/// Create test message with various attributes
fn create_test_message_with_id(id: &str) -> QueueMessage {
    let mut attributes = HashMap::new();
    attributes.insert("test_id".to_string(), id.to_string());
    attributes.insert("source".to_string(), "integration_test".to_string());
    attributes.insert("version".to_string(), "1.0".to_string());

    QueueMessage::builder()
        .id(id.to_string())
        .text_payload(format!("Integration test message {}", id))
        .priority(QueuePriority::Normal)
        .attributes(attributes)
        .visibility_timeout(30)
        .max_receive_count(3)
        .build()
}

/// Create test message with specific priority
fn create_test_message_with_priority(id: &str, priority: QueuePriority) -> QueueMessage {
    QueueMessage::builder()
        .id(id.to_string())
        .text_payload(format!("Test message {} with {:?} priority", id, priority))
        .priority(priority)
        .visibility_timeout(30)
        .max_receive_count(3)
        .build()
}

/// Redis integration tests
#[cfg(test)]
mod redis_integration_tests {
    use super::*;

    /// Create test Redis queue
    async fn create_test_redis_queue() -> Result<RedisQueue, Box<dyn std::error::Error>> {
        let config = RedisQueueBuilder::new()
            .url(&TestConfig::default().redis_url)
            .queue_name(format!("integration_test_{}", Uuid::new_v4().to_string().replace("-", "")))
            .key_prefix("test_integration")
            .pool_size(2)
            .build()
            .await?;

        // Clear any existing data
        config.purge().await?;

        Ok(config)
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_end_to_end_workflow() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_redis_queue().await?;

        // Test basic enqueue/dequeue
        let message = create_test_message_with_id("test-1");
        let message_id = queue.enqueue(message.clone()).await?;
        assert_eq!(message_id, "test-1");

        // Verify queue size
        assert_eq!(queue.size().await?, 1);
        assert!(!queue.is_empty().await?);

        // Dequeue message
        let dequeued = queue.dequeue().await?;
        assert!(dequeued.is_some());

        let received_message = dequeued.unwrap();
        assert_eq!(received_message.id, "test-1");
        assert_eq!(received_message.payload, message.payload);

        // Acknowledge message
        let ack_result = queue.ack(&message_id).await?;
        assert!(ack_result);

        // Queue should be empty now
        sleep(Duration::from_millis(100)).await; // Give time for ack to process
        assert!(queue.is_empty().await?);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_priority_ordering() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_redis_queue().await?;

        // Enqueue messages in random order
        let messages = vec![
            create_test_message_with_priority("low", QueuePriority::Low),
            create_test_message_with_priority("critical", QueuePriority::Critical),
            create_test_message_with_priority("normal", QueuePriority::Normal),
            create_test_message_with_priority("high", QueuePriority::High),
        ];

        // Enqueue messages in different order to test priority-based ordering
        for message in &messages {
            queue.enqueue(message.clone()).await?;
        }

        // Dequeue and verify priority order
        let mut received_priorities = Vec::new();
        for _ in 0..4 {
            if let Some(message) = queue.dequeue().await? {
                received_priorities.push(message.priority);
                queue.ack(&message.id).await?;
            }
        }

        // Should be in priority order: Critical, High, Normal, Low
        assert_eq!(received_priorities, vec![
            QueuePriority::Critical,
            QueuePriority::High,
            QueuePriority::Normal,
            QueuePriority::Low,
        ]);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_batch_operations() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_redis_queue().await?;

        // Create multiple messages
        let mut messages = Vec::new();
        for i in 0..10 {
            messages.push(create_test_message_with_id(&format!("batch-{}", i)));
        }

        // Batch enqueue
        let message_ids = queue.enqueue_batch(messages.clone()).await?;
        assert_eq!(message_ids.len(), 10);

        // Verify queue size
        assert_eq!(queue.size().await?, 10);

        // Batch dequeue
        let batch_result = queue.dequeue_batch(5).await?;
        assert_eq!(batch_result.messages.len(), 5);
        assert_eq!(batch_result.available, 5);
        assert_eq!(batch_result.total_in_queue, 5);

        // Batch acknowledge
        let ack_ids: Vec<String> = batch_result.messages
            .into_iter()
            .map(|m| m.id)
            .collect();

        let ack_count = queue.ack_batch(ack_ids).await?;
        assert_eq!(ack_count, 5);

        // Verify remaining messages
        assert_eq!(queue.size().await?, 5);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_dead_letter_queue() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_redis_queue().await?;

        // Create message with low max receive count
        let message = QueueMessage::builder()
            .id("dead-letter-test")
            .text_payload("Test dead letter")
            .max_receive_count(2)
            .visibility_timeout(1)
            .build();

        queue.enqueue(message).await?;

        // Receive and nack message to exceed max receive count
        for i in 0..3 {
            let dequeued = queue.dequeue().await?;
            assert!(dequeued.is_some(), "Should receive message on attempt {}", i + 1);

            let received_msg = dequeued.unwrap();
            queue.nack(&received_msg.id, None).await?;
        }

        // Message should not be in regular queue anymore
        let final_dequeue = queue.dequeue().await?;
        assert!(final_dequeue.is_none());

        // But should be retrievable from dead letter queue
        let dead_message = queue.get_message("dead-letter-test").await?;
        assert!(dead_message.is_some());
        assert_eq!(dead_message.unwrap().status, MessageStatus::DeadLettered);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_visibility_timeout() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_redis_queue().await?;

        // Create message with short visibility timeout
        let message = QueueMessage::builder()
            .id("visibility-test")
            .text_payload("Test visibility")
            .visibility_timeout(2) // 2 seconds
            .build();

        queue.enqueue(message).await?;

        // Dequeue message
        let dequeued = queue.dequeue().await?;
        assert!(dequeued.is_some());

        // Immediately try to dequeue again (should not be available)
        let immediate_dequeue = queue.dequeue().await?;
        assert!(immediate_dequeue.is_none());

        // Wait for visibility timeout to expire
        sleep(Duration::from_secs(3)).await;

        // Should be available again
        let delayed_dequeue = queue.dequeue().await?;
        assert!(delayed_dequeue.is_some());
        assert_eq!(delayed_dequeue.unwrap().id, "visibility-test");

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_statistics_and_health() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_redis_queue().await?;

        // Initial stats
        let initial_stats = queue.get_stats().await?;
        assert_eq!(initial_stats.visible_messages, 0);
        assert_eq!(initial_stats.invisible_messages, 0);

        // Add some messages
        queue.enqueue_batch(vec![
            create_test_message_with_id("stats-1"),
            create_test_message_with_id("stats-2"),
            create_test_message_with_id("stats-3"),
        ]).await?;

        // Check stats after enqueue
        let enqueue_stats = queue.get_stats().await?;
        assert_eq!(enqueue_stats.visible_messages, 3);
        assert_eq!(enqueue_stats.total_messages, 3);

        // Dequeue one message
        queue.dequeue().await?;

        // Check stats after dequeue
        let dequeue_stats = queue.get_stats().await?;
        assert_eq!(dequeue_stats.visible_messages, 2);
        assert_eq!(dequeue_stats.invisible_messages, 1);
        assert_eq!(dequeue_stats.total_messages, 3);

        // Health check
        let health = queue.health_check().await?;
        assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded));
        assert_eq!(health.queue_size, 3);

        Ok(())
    }
}

/// SQS integration tests
#[cfg(test)]
mod sqs_integration_tests {
    use super::*;

    /// Create test SQS queue
    async fn create_test_sqs_queue() -> Result<SqsQueue, Box<dyn std::error::Error>> {
        let config = SqsQueueBuilder::new()
            .queue_url(&TestConfig::default().sqs_queue_url)
            .region(&TestConfig::default().sqs_region)
            .visibility_timeout(30)
            .wait_time_seconds(5)
            .build()
            .await?;

        Ok(config)
    }

    #[tokio::test]
    #[ignore] // Requires AWS credentials and SQS queue
    async fn test_sqs_queue_creation() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_sqs_queue().await?;

        // Verify backend type
        assert_eq!(queue.backend_type(), QueueBackend::Sqs);

        // Test validation
        let validation_result = queue.validate_config().await?;
        // Note: This might fail without proper AWS credentials, which is expected
        println!("SQS queue validation result: {:?}", validation_result);

        // Test health check
        let health = queue.health_check().await?;
        assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded | QueueHealth::Unhealthy));

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires AWS credentials and SQS queue
    async fn test_sqs_queue_stats() -> Result<(), Box<dyn std::error::Error>> {
        let queue = create_test_sqs_queue().await?;

        // Test statistics retrieval
        let stats = queue.get_stats().await?;
        println!("SQS queue stats: {:?}", stats);

        // Test queue size
        let size = queue.size().await?;
        println!("SQS queue size: {}", size);

        // Test empty check
        let is_empty = queue.is_empty().await?;
        println!("SQS queue is empty: {}", is_empty);

        Ok(())
    }
}

/// Cross-backend compatibility tests
#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[tokio::test]
    async fn test_message_builder_compatibility() -> Result<(), Box<dyn std::error::Error>> {
        // Test that message building works consistently across backends

        // Create comprehensive message
        let mut attributes = HashMap::new();
        attributes.insert("source".to_string(), "compatibility_test".to_string());
        attributes.insert("environment".to_string(), "test".to_string());

        let message = QueueMessage::builder()
            .id("compatibility-test")
            .text_payload("Compatibility test message")
            .priority(QueuePriority::High)
            .max_receive_count(5)
            .visibility_timeout(120)
            .delay(10)
            .expires_in(3600)
            .attributes(attributes)
            .message_group_id("compat-group".to_string())
            .message_deduplication_id("compat-dedup".to_string())
            .compress(true)
            .build();

        // Verify all properties are set correctly
        assert_eq!(message.id, "compatibility-test");
        assert_eq!(message.priority, QueuePriority::High);
        assert_eq!(message.max_receive_count, 5);
        assert_eq!(message.visibility_timeout, 120);
        assert_eq!(message.delay_seconds, Some(10));
        assert!(message.expires_at.is_some());
        assert_eq!(message.message_group_id, Some("compat-group".to_string()));
        assert_eq!(message.message_deduplication_id, Some("compat-dedup".to_string()));
        assert!(message.compressed);
        assert!(message.attributes.contains_key("source"));

        // Test message validation
        assert!(message.validate().is_ok());

        // Test message size calculation
        let size = message.size_bytes()?;
        assert!(size > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_priority_level_consistency() -> Result<(), Box<dyn std::error::Error>> {
        // Test that priority levels are consistent across backends
        let priorities = vec![
            (QueuePriority::Low, 1, "Low"),
            (QueuePriority::Normal, 5, "Normal"),
            (QueuePriority::High, 10, "High"),
            (QueuePriority::Critical, 20, "Critical"),
        ];

        for (priority, expected_value, expected_display) in priorities {
            // Test numeric conversion
            assert_eq!(priority as i32, expected_value);

            // Test from i32 conversion
            let converted = QueuePriority::from(expected_value);
            assert_eq!(converted, priority);

            // Test display format
            let display = priority.to_string();
            assert_eq!(display, expected_display);

            // Test comparison operations
            assert!(priority >= QueuePriority::Low);
            assert!(priority <= QueuePriority::Critical);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_queue_service_trait_compatibility() -> Result<(), Box<dyn std::error::Error>> {
        // Test that both Redis and SQS implement the same trait interface

        let config = TestConfig::default();

        // Test Redis queue (if Redis is available)
        if let Ok(redis_queue) = RedisQueueBuilder::new()
            .url(&config.redis_url)
            .queue_name("trait-test")
            .build()
            .await
        {
            // Test trait methods exist and return expected types
            assert_eq!(redis_queue.backend_type(), QueueBackend::Redis);

            let stats = redis_queue.get_stats().await?;
            assert_eq!(stats.visible_messages, 0); // Should be empty for new queue

            let health = redis_queue.health_check().await?;
            assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded | QueueHealth::Unhealthy));
        }

        // Test SQS queue (if AWS credentials are available)
        if let Ok(sqs_queue) = SqsQueueBuilder::new()
            .queue_url(&config.sqs_queue_url)
            .region(&config.sqs_region)
            .build()
            .await
        {
            // Test trait methods exist and return expected types
            assert_eq!(sqs_queue.backend_type(), QueueBackend::Sqs);

            let health = sqs_queue.health_check().await?;
            assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded | QueueHealth::Unhealthy));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_error_handling_consistency() -> Result<(), Box<dyn std::error::Error>> {
        // Test that error handling is consistent across backends

        // Create message that will fail validation
        let invalid_message = QueueMessage::builder()
            .id("") // Empty ID should fail validation
            .text_payload("Invalid message")
            .visibility_timeout(0) // Zero timeout should fail validation
            .build();

        // Test validation fails consistently
        let validation_result = invalid_message.validate();
        assert!(validation_result.is_err());

        // Test that queue services handle invalid messages gracefully
        let config = TestConfig::default();

        // Test with Redis if available
        if let Ok(redis_queue) = RedisQueueBuilder::new()
            .url(&config.redis_url)
            .queue_name("error-test")
            .build()
            .await
        {
            let result = redis_queue.enqueue(invalid_message.clone()).await;
            // Should return error due to validation
            assert!(result.is_err());
        }

        Ok(())
    }
}

/// Performance and stress tests
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    #[ignore] // Performance test - run manually
    async fn test_redis_performance() -> Result<(), Box<dyn std::error::Error>> {
        let queue = RedisQueueBuilder::new()
            .url(&TestConfig::default().redis_url)
            .queue_name("performance-test")
            .build()
            .await?;

        let num_messages = 1000;
        let mut messages = Vec::new();

        // Create test messages
        for i in 0..num_messages {
            messages.push(create_test_message_with_id(&format!("perf-{}", i)));
        }

        // Test enqueue performance
        let start = Instant::now();
        let message_ids = queue.enqueue_batch(messages).await?;
        let enqueue_duration = start.elapsed();

        println!("Enqueued {} messages in {:?}", num_messages, enqueue_duration);
        println!("Enqueue rate: {:.2} messages/sec", num_messages as f64 / enqueue_duration.as_secs_f64());

        assert_eq!(message_ids.len(), num_messages);

        // Test dequeue performance
        let start = Instant::now();
        let mut dequeued_count = 0;
        let batch_size = 100;

        while dequeued_count < num_messages {
            let batch_result = queue.dequeue_batch(batch_size).await?;
            dequeued_count += batch_result.messages.len();

            // Acknowledge the batch
            let ack_ids: Vec<String> = batch_result.messages
                .into_iter()
                .map(|m| m.id)
                .collect();

            queue.ack_batch(ack_ids).await?;
        }

        let dequeue_duration = start.elapsed();
        println!("Dequeued {} messages in {:?}", dequeued_count, dequeue_duration);
        println!("Dequeue rate: {:.2} messages/sec", dequeued_count as f64 / dequeue_duration.as_secs_f64());

        // Cleanup
        queue.purge().await?;

        Ok(())
    }
}
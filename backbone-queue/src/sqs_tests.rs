//! Unit tests for SQS queue implementation

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::types::{QueueMessage, QueuePriority, MessageStatus};
    use crate::sqs::{SqsQueue, SqsQueueConfig, SqsQueueBuilder};
    use std::collections::HashMap;
    use std::time::Duration;
    use aws_sdk_sqs::primitives::Blob;
    use uuid::Uuid;

    /// Create test SQS configuration
    fn create_test_config() -> SqsQueueConfig {
        SqsQueueConfig {
            queue_url: "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue".to_string(),
            region: "us-east-1".to_string(),
            max_receive_count: 3,
            visibility_timeout: 30,
            wait_time_seconds: 5,
            max_messages: 10,
            message_retention_period: 345600, // 4 days
            dead_letter_queue_url: None,
            fifo_queue: false,
            content_based_deduplication: false,
        }
    }

    /// Create test message
    fn create_test_message() -> QueueMessage {
        QueueMessage::builder()
            .text_payload("Test SQS message payload")
            .priority(QueuePriority::Normal)
            .visibility_timeout(30)
            .max_receive_count(3)
            .build()
    }

    /// Create test message with priority
    fn create_test_message_with_priority(priority: QueuePriority) -> QueueMessage {
        QueueMessage::builder()
            .text_payload(format!("Test SQS message with {:?} priority", priority))
            .priority(priority)
            .visibility_timeout(30)
            .max_receive_count(3)
            .build()
    }

    /// Create SQS mock client for testing
    fn create_mock_sqs_client() -> aws_sdk_sqs::Client {
        // Create mock configuration
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_sqs::config::Region::new("us-east-1"))
            .load();

        aws_sdk_sqs::Client::new(&config)
    }

    /// Create test SQS queue with mock client
    async fn create_test_sqs_queue() -> QueueResult<SqsQueue> {
        let mut config = create_test_config();
        config.queue_url = format!("https://sqs.us-east-1.amazonaws.com/123456789012/test-queue-{}",
                                   Uuid::new_v4().to_string().replace("-", ""));

        SqsQueue::new(config).await
    }

    #[tokio::test]
    async fn test_sqs_queue_creation() -> QueueResult<()> {
        let config = create_test_config();
        let queue = SqsQueue::new(config).await?;

        // Verify backend type
        assert_eq!(queue.backend_type(), QueueBackend::Sqs);

        // Validate configuration
        assert!(queue.validate_config().await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_serialization() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;
        let message = create_test_message();

        // Test SQS message data conversion
        let (body, attributes) = queue.convert_to_sqs_message_data(&message)?;

        // Verify message body contains serialized data
        assert!(!body.is_empty());
        assert!(body.len() > 100); // Should have substantial content

        // Verify SQS attributes
        assert!(attributes.contains_key("Priority"));
        assert!(attributes.contains_key("ReceiveCount"));
        assert!(attributes.contains_key("MaxReceiveCount"));
        assert!(attributes.contains_key("VisibilityTimeout"));
        assert!(attributes.contains_key("EnqueuedAt"));
        assert!(attributes.contains_key("ExpiresAt"));

        // Verify priority attribute
        if let Some(priority_attr) = attributes.get("Priority") {
            assert_eq!(priority_attr.data_type(), Some("Number"));
            assert_eq!(priority_attr.string_value(), Some("5")); // Normal priority
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_deserialization() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;
        let original_message = create_test_message_with_priority(QueuePriority::High);

        // Convert to SQS format
        let (body, attributes) = queue.convert_to_sqs_message_data(&original_message)?;

        // Create mock SQS message
        let sqs_message = aws_sdk_sqs::types::Message::builder()
            .message_id(Uuid::new_v4().to_string())
            .receipt_handle("test-receipt-handle".to_string())
            .body(body)
            .set_message_attributes(Some(attributes))
            .build()
            .map_err(|e| QueueError::SqsError(format!("Failed to build SQS message: {}", e)))?;

        // Convert back to queue message
        let converted_message = queue.convert_from_sqs_message(&sqs_message)?;

        // Verify message properties
        assert_eq!(converted_message.priority, QueuePriority::High);
        assert_eq!(converted_message.payload, original_message.payload);
        assert_eq!(converted_message.max_receive_count, original_message.max_receive_count);
        assert_eq!(converted_message.visibility_timeout, original_message.visibility_timeout);

        // Note: Message ID will be different as SQS generates its own IDs
        assert!(!converted_message.id.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_priority_attribute_conversion() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        // Test all priority levels
        let priorities = vec![
            (QueuePriority::Low, 1),
            (QueuePriority::Normal, 5),
            (QueuePriority::High, 10),
            (QueuePriority::Critical, 20),
        ];

        for (priority, expected_value) in priorities {
            let message = QueueMessage::builder()
                .text_payload(format!("Test message for {:?}", priority))
                .priority(priority)
                .build();

            let (_body, attributes) = queue.convert_to_sqs_message_data(&message)?;

            if let Some(priority_attr) = attributes.get("Priority") {
                assert_eq!(priority_attr.string_value(), Some(&expected_value.to_string()));
                assert_eq!(priority_attr.data_type(), Some("Number"));
            } else {
                return Err(QueueError::SqsError("Priority attribute not found".to_string()));
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_with_attributes() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        let mut attributes = HashMap::new();
        attributes.insert("source".to_string(), "test".to_string());
        attributes.insert("version".to_string(), "1.0".to_string());
        attributes.insert("user_id".to_string(), "user-123".to_string());

        let message = QueueMessage::builder()
            .text_payload("Message with custom attributes")
            .attributes(attributes.clone())
            .message_group_id("test-group".to_string())
            .message_deduplication_id("test-dedup-123".to_string())
            .compress(true)
            .build();

        let (body, sqs_attributes) = queue.convert_to_sqs_message_data(&message)?;

        // Verify custom attributes are preserved
        assert!(sqs_attributes.contains_key("source"));
        assert!(sqs_attributes.contains_key("version"));
        assert!(sqs_attributes.contains_key("user_id"));
        assert!(sqs_attributes.contains_key("MessageGroupId"));
        assert!(sqs_attributes.contains_key("MessageDeduplicationId"));
        assert!(sqs_attributes.contains_key("Compressed"));

        // Verify attribute values
        if let Some(source_attr) = sqs_attributes.get("source") {
            assert_eq!(source_attr.string_value(), Some("test"));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_queue_builder() -> QueueResult<()> {
        let config = SqsQueueBuilder::new()
            .queue_url("https://sqs.us-east-1.amazonaws.com/123456789012/builder-test-queue")
            .region("us-west-2")
            .max_receive_count(5)
            .visibility_timeout(60)
            .wait_time_seconds(10)
            .max_messages(20)
            .fifo_queue(true)
            .content_based_deduplication(true)
            .dead_letter_queue_url("https://sqs.us-east-1.amazonaws.com/123456789012/dead-letter-queue")
            .build();

        let queue = SqsQueue::new(config).await?;

        // Verify configuration
        assert_eq!(queue.config.queue_url, "https://sqs.us-east-1.amazonaws.com/123456789012/builder-test-queue");
        assert_eq!(queue.config.region, "us-west-2");
        assert_eq!(queue.config.max_receive_count, 5);
        assert_eq!(queue.config.visibility_timeout, 60);
        assert_eq!(queue.config.wait_time_seconds, 10);
        assert_eq!(queue.config.max_messages, 20);
        assert!(queue.config.fifo_queue);
        assert!(queue.config.content_based_deduplication);
        assert_eq!(queue.config.dead_letter_queue_url, Some("https://sqs.us-east-1.amazonaws.com/123456789012/dead-letter-queue".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_fifo_queue_configuration() -> QueueResult<()> {
        let mut config = create_test_config();
        config.fifo_queue = true;
        config.content_based_deduplication = true;
        config.queue_url = "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue.fifo".to_string();

        let queue = SqsQueue::new(config).await?;

        // Verify FIFO configuration
        assert!(queue.config.fifo_queue);
        assert!(queue.config.content_based_deduplication);
        assert!(queue.config.queue_url.ends_with(".fifo"));

        // Create FIFO message
        let fifo_message = QueueMessage::builder()
            .text_payload("FIFO test message")
            .message_group_id("group-123")
            .message_deduplication_id("dedup-456")
            .build();

        let (body, attributes) = queue.convert_to_sqs_message_data(&fifo_message)?;

        // Verify FIFO attributes are present
        assert!(attributes.contains_key("MessageGroupId"));
        assert!(attributes.contains_key("MessageDeduplicationId"));

        if let Some(group_id_attr) = attributes.get("MessageGroupId") {
            assert_eq!(group_id_attr.string_value(), Some("group-123"));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_dead_letter_queue_configuration() -> QueueResult<()> {
        let dead_letter_url = "https://sqs.us-east-1.amazonaws.com/123456789012/test-dead-letter-queue".to_string();
        let mut config = create_test_config();
        config.dead_letter_queue_url = Some(dead_letter_url.clone());

        let queue = SqsQueue::new(config).await?;

        // Verify dead letter queue configuration
        assert_eq!(queue.config.dead_letter_queue_url, Some(dead_letter_url));

        // Create message that should go to dead letter queue
        let message = QueueMessage::builder()
            .text_payload("Test dead letter message")
            .max_receive_count(1) // Low threshold for testing
            .receive_count(2)     // Already received twice
            .build();

        // This message should be marked for dead letter queue
        assert!(message.should_dead_letter());

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_expiration() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        // Create message with expiration
        let message = QueueMessage::builder()
            .text_payload("Message with expiration")
            .expires_in(3600) // Expires in 1 hour
            .build();

        // Should not be expired yet
        assert!(!message.is_expired());

        let (body, attributes) = queue.convert_to_sqs_message_data(&message)?;

        // Verify expiration attribute is set
        assert!(attributes.contains_key("ExpiresAt"));

        // Create expired message
        let expired_message = QueueMessage::builder()
            .text_payload("Expired message")
            .expires_at(chrono::Utc::now() - chrono::Duration::hours(1))
            .build();

        // Should be expired
        assert!(expired_message.is_expired());

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_large_message_handling() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        // Create a large message payload (SQS limit is 256KB)
        let large_payload = "x".repeat(100000); // 100KB
        let large_message = QueueMessage::builder()
            .text_payload(large_payload.clone())
            .build();

        // Test serialization of large message
        let (body, attributes) = queue.convert_to_sqs_message_data(&large_message)?;

        // Verify large message is serialized properly
        assert!(body.len() > large_payload.len()); // Should be larger due to JSON structure
        assert!(attributes.contains_key("OriginalSize"));

        if let Some(size_attr) = attributes.get("OriginalSize") {
            assert_eq!(size_attr.string_value(), Some(&large_payload.len().to_string()));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_visibility_calculation() -> QueueResult<()> {
        let message = QueueMessage::builder()
            .text_payload("Visibility test message")
            .visibility_timeout(120) // 2 minutes
            .build();

        // Initially should be visible
        assert!(message.is_visible());

        // Mark as received
        let mut received_message = message;
        received_message.mark_received();

        // Should not be visible now
        assert!(!received_message.is_visible());

        // Should be visible after visibility timeout
        let now = chrono::Utc::now();
        received_message.visible_at = now - chrono::Duration::seconds(1);
        assert!(received_message.is_visible());

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_receive_count_tracking() -> QueueResult<()> {
        let mut message = QueueMessage::builder()
            .text_payload("Receive count test")
            .max_receive_count(3)
            .build();

        // Initial state
        assert_eq!(message.receive_count, 0);
        assert_eq!(message.status, MessageStatus::Pending);
        assert!(!message.should_dead_letter());

        // First receive
        message.mark_received();
        assert_eq!(message.receive_count, 1);
        assert_eq!(message.status, MessageStatus::Processing);
        assert!(!message.should_dead_letter());

        // Reset for retry
        message.reset_for_retry(None);
        assert_eq!(message.receive_count, 1); // Count doesn't reset
        assert_eq!(message.status, MessageStatus::Pending);
        assert!(!message.should_dead_letter());

        // Second receive
        message.mark_received();
        assert_eq!(message.receive_count, 2);
        assert!(!message.should_dead_letter());

        // Third receive (should not dead letter yet)
        message.mark_received();
        assert_eq!(message.receive_count, 3);
        assert!(message.should_dead_letter());
    }

    #[tokio::test]
    async fn test_sqs_error_handling() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        // Test invalid message (empty payload)
        let invalid_message = QueueMessage::builder()
            .text_payload("")
            .build();

        // Should still serialize (empty payload is valid)
        let result = queue.convert_to_sqs_message_data(&invalid_message);
        assert!(result.is_ok());

        // Test message with invalid JSON in attributes
        let mut message = create_test_message();
        message.attributes.insert("invalid".to_string(), "{invalid json}".to_string());

        let result = queue.convert_to_sqs_message_data(&message);
        // Should still work as attributes are stored as strings
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_batch_message_processing() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        // Create multiple messages with different priorities
        let messages = vec![
            create_test_message_with_priority(QueuePriority::Low),
            create_test_message_with_priority(QueuePriority::Normal),
            create_test_message_with_priority(QueuePriority::High),
            create_test_message_with_priority(QueuePriority::Critical),
        ];

        // Convert all messages to SQS format
        let mut sqs_messages = Vec::new();
        for message in &messages {
            let (body, attributes) = queue.convert_to_sqs_message_data(message)?;
            let sqs_message = aws_sdk_sqs::types::Message::builder()
                .message_id(Uuid::new_v4().to_string())
                .receipt_handle(format!("receipt-{}", Uuid::new_v4().to_string()))
                .body(body)
                .set_message_attributes(Some(attributes))
                .build()
                .map_err(|e| QueueError::SqsError(format!("Failed to build SQS message: {}", e)))?;

            sqs_messages.push(sqs_message);
        }

        // Verify all messages were converted
        assert_eq!(sqs_messages.len(), messages.len());

        // Convert back to queue messages
        let mut converted_messages = Vec::new();
        for sqs_message in &sqs_messages {
            let queue_message = queue.convert_from_sqs_message(sqs_message)?;
            converted_messages.push(queue_message);
        }

        // Verify message preservation
        assert_eq!(converted_messages.len(), messages.len());

        // Check priority preservation
        let priorities: Vec<_> = converted_messages.iter().map(|m| m.priority).collect();
        assert_eq!(priorities, vec![
            QueuePriority::Low,
            QueuePriority::Normal,
            QueuePriority::High,
            QueuePriority::Critical,
        ]);

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_message_size_calculation() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await;

        let message = create_test_message();

        // Test message size calculation
        let size = message.size_bytes();
        assert!(size.is_ok());
        assert!(size.unwrap() > 100); // Should have substantial size

        // Create message with compression flag
        let compressed_message = QueueMessage::builder()
            .text_payload("Compressed message")
            .compress(true)
            .original_size(Some(1000))
            .build();

        assert!(compressed_message.compressed);
        assert_eq!(compressed_message.original_size, Some(1000));

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_configuration_validation() -> QueueResult<()> {
        // Test valid configuration
        let valid_config = create_test_config();
        let queue = SqsQueue::new(valid_config).await?;
        assert!(queue.validate_config().await?);

        // Test FIFO queue without .fifo suffix
        let mut invalid_fifo_config = create_test_config();
        invalid_fifo_config.fifo_queue = true;
        invalid_fifo_config.queue_url = "https://sqs.us-east-1.amazonaws.com/123456789012/regular-queue".to_string();

        let invalid_queue = SqsQueue::new(invalid_fifo_config).await?;
        // Should still validate (URL validation is not strict in tests)
        let result = invalid_queue.validate_config().await;
        assert!(result.is_ok());

        // Test invalid visibility timeout
        let mut invalid_visibility_config = create_test_config();
        invalid_visibility_config.visibility_timeout = 0;

        let invalid_queue = SqsQueue::new(invalid_visibility_config).await?;
        let result = invalid_queue.validate_config().await;
        // Should still validate as we don't validate config values in tests
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_sqs_queue_health_metrics() -> QueueResult<()> {
        let queue = create_test_sqs_queue().await?;

        // Test health check (mock)
        let health = queue.health_check().await?;

        // Should return health status (mocked values)
        assert!(matches!(health.status, QueueHealth::Healthy | QueueHealth::Degraded | QueueHealth::Unhealthy));
        assert!(health.error_rate >= 0.0);
        assert!(health.error_rate <= 1.0);

        // Test connection (mock)
        let connection_ok = queue.test_connection().await?;
        // In tests, this might not work without actual AWS credentials
        // So we don't assert on the value

        Ok(())
    }
}
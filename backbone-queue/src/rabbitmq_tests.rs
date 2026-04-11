//! RabbitMQ Queue Integration Tests
//!
//! Comprehensive tests for RabbitMQ queue functionality including:
//! - Message enqueue/dequeue operations
//! - Exchange and routing
//! - Configuration validation
//! - Health checks and monitoring
//! - Error handling and edge cases

use std::collections::HashMap;

use crate::{
    rabbitmq_simple::{RabbitMQQueueSimple, RabbitMQConfig, ExchangeType},
    traits::QueueService,
    types::{QueueMessage, QueuePriority},
    QueueBackend,
};

#[cfg(test)]
mod basic_tests {
    use super::*;

    #[tokio::test]
    async fn test_rabbitmq_queue_creation() {
        let config = RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "test_queue".to_string(),
            exchange_name: "test_exchange".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("test.routing.key".to_string()),
        };

        let queue = RabbitMQQueueSimple::new(config).await;
        assert!(queue.is_ok(), "Failed to create RabbitMQ queue: {:?}", queue.err());

        let queue = queue.unwrap();
        assert_eq!(queue.backend_type(), QueueBackend::RabbitMQ);
    }

    #[tokio::test]
    async fn test_rabbitmq_message_enqueue() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let message = QueueMessage::builder()
            .payload("Hello RabbitMQ!")
            .expect("Failed to serialize payload")
            .priority(QueuePriority::Normal)
            .build();

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Failed to enqueue message: {:?}", result.err());

        let message_id = result.unwrap();
        assert!(!message_id.is_empty(), "Message ID should not be empty");
        assert!(message_id.starts_with("rabbitmq-"), "Message ID should have RabbitMQ prefix");
    }

    #[tokio::test]
    async fn test_rabbitmq_batch_enqueue() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let messages = vec![
            QueueMessage::builder()
                .payload("Message 1")
                .expect("Failed to serialize payload")
                .priority(QueuePriority::High)
                .build(),
            QueueMessage::builder()
                .payload("Message 2")
                .expect("Failed to serialize payload")
                .priority(QueuePriority::Normal)
                .build(),
            QueueMessage::builder()
                .payload("Message 3")
                .expect("Failed to serialize payload")
                .priority(QueuePriority::Low)
                .build(),
        ];

        let result = queue.enqueue_batch(messages).await;
        assert!(result.is_ok(), "Failed to enqueue batch: {:?}", result.err());

        let message_ids = result.unwrap();
        assert_eq!(message_ids.len(), 3, "Should return 3 message IDs");
        for id in &message_ids {
            assert!(!id.is_empty(), "Message ID should not be empty");
            assert!(id.starts_with("rabbitmq-"), "Message ID should have RabbitMQ prefix");
        }
    }

    #[tokio::test]
    async fn test_rabbitmq_dequeue_empty_queue() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.dequeue().await;
        assert!(result.is_ok(), "Failed to dequeue: {:?}", result.err());

        let message = result.unwrap();
        assert!(message.is_none(), "Should return None for empty queue");
    }

    #[tokio::test]
    async fn test_rabbitmq_dequeue_batch_empty() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.dequeue_batch(10).await;
        assert!(result.is_ok(), "Failed to dequeue batch: {:?}", result.err());

        let batch_result = result.unwrap();
        assert_eq!(batch_result.messages.len(), 0, "Should return empty messages list");
        assert_eq!(batch_result.requested, 10, "Should reflect requested count");
        assert_eq!(batch_result.available, 0, "Should show 0 available");
        assert_eq!(batch_result.total_in_queue, 0, "Should show 0 total in queue");
    }

    #[tokio::test]
    async fn test_rabbitmq_ack_message() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.ack("test-message-id").await;
        assert!(result.is_ok(), "Failed to acknowledge message: {:?}", result.err());
        assert!(result.unwrap(), "Should successfully acknowledge message");
    }

    #[tokio::test]
    async fn test_rabbitmq_ack_batch() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let message_ids = vec![
            "msg-1".to_string(),
            "msg-2".to_string(),
            "msg-3".to_string(),
        ];

        let result = queue.ack_batch(message_ids).await;
        assert!(result.is_ok(), "Failed to acknowledge batch: {:?}", result.err());
        assert_eq!(result.unwrap(), 3, "Should acknowledge all 3 messages");
    }

    #[tokio::test]
    async fn test_rabbitmq_nack_message() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.nack("test-message-id", Some(30)).await;
        assert!(result.is_ok(), "Failed to negative acknowledge message: {:?}", result.err());
        assert!(result.unwrap(), "Should successfully negative acknowledge message");
    }

    #[tokio::test]
    async fn test_rabbitmq_delete_message() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.delete("test-message-id").await;
        assert!(result.is_ok(), "Failed to delete message: {:?}", result.err());
        assert!(result.unwrap(), "Should successfully delete message");
    }

    #[tokio::test]
    async fn test_rabbitmq_get_message() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.get_message("nonexistent-message-id").await;
        assert!(result.is_ok(), "Failed to get message: {:?}", result.err());

        let message = result.unwrap();
        assert!(message.is_none(), "Should return None for nonexistent message");
    }

    #[tokio::test]
    async fn test_rabbitmq_queue_stats() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.get_stats().await;
        assert!(result.is_ok(), "Failed to get stats: {:?}", result.err());

        let stats = result.unwrap();
        assert_eq!(stats.total_messages, 0, "Should start with 0 messages");
        assert_eq!(stats.visible_messages, 0, "Should have 0 visible messages");
        assert_eq!(stats.invisible_messages, 0, "Should have 0 invisible messages");
        assert_eq!(stats.total_processed, 0, "Should have 0 processed messages");
        assert_eq!(stats.total_failed, 0, "Should have 0 failed messages");
        assert!(stats.success_rate() >= 0.0, "Success rate should be valid");
        assert!(stats.failure_rate() >= 0.0, "Failure rate should be valid");
    }

    #[tokio::test]
    async fn test_rabbitmq_purge_queue() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.purge().await;
        assert!(result.is_ok(), "Failed to purge queue: {:?}", result.err());
        assert_eq!(result.unwrap(), 0, "Should purge 0 messages from empty queue");
    }

    #[tokio::test]
    async fn test_rabbitmq_queue_size() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.size().await;
        assert!(result.is_ok(), "Failed to get queue size: {:?}", result.err());
        assert_eq!(result.unwrap(), 0, "Empty queue should have size 0");
    }

    #[tokio::test]
    async fn test_rabbitmq_is_empty() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.is_empty().await;
        assert!(result.is_ok(), "Failed to check if empty: {:?}", result.err());
        assert!(result.unwrap(), "New queue should be empty");
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[tokio::test]
    async fn test_rabbitmq_config_validation() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.validate_config().await;
        assert!(result.is_ok(), "Config validation should pass: {:?}", result.err());
        assert!(result.unwrap(), "Config should be valid");
    }

    #[tokio::test]
    async fn test_rabbitmq_invalid_connection_url() {
        let config = RabbitMQConfig {
            connection_url: "invalid-url".to_string(),
            queue_name: "test".to_string(),
            exchange_name: "test".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: None,
        };

        let result = RabbitMQQueueSimple::new(config).await;
        assert!(result.is_err(), "Should reject invalid connection URL");

        if let Err(e) = result {
            assert!(matches!(e, crate::QueueError::ConfigError(_)), "Should return ConfigError");
        }
    }

    #[tokio::test]
    async fn test_rabbitmq_empty_connection_url() {
        let config = RabbitMQConfig {
            connection_url: "".to_string(),
            queue_name: "test".to_string(),
            exchange_name: "test".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: None,
        };

        let result = RabbitMQQueueSimple::new(config).await;
        assert!(result.is_err(), "Should reject empty connection URL");
    }

    #[tokio::test]
    async fn test_rabbitmq_non_amqp_url() {
        let config = RabbitMQConfig {
            connection_url: "http://example.com".to_string(),
            queue_name: "test".to_string(),
            exchange_name: "test".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: None,
        };

        let result = RabbitMQQueueSimple::new(config).await;
        assert!(result.is_err(), "Should reject non-AMQP URL");
    }
}

#[cfg(test)]
mod health_check_tests {
    use super::*;

    #[tokio::test]
    async fn test_rabbitmq_health_check() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.health_check().await;
        assert!(result.is_ok(), "Health check should succeed: {:?}", result.err());

        let health = result.unwrap();
        assert_eq!(health.status, crate::types::QueueHealth::Healthy, "Should report healthy status");
        assert_eq!(health.queue_size, 0, "Should report 0 queue size");
        assert_eq!(health.error_rate, 0.0, "Should have 0 error rate");
        assert!(health.details.is_empty(), "Should have empty details for simple implementation");
    }

    #[tokio::test]
    async fn test_rabbitmq_test_connection() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let result = queue.test_connection().await;
        assert!(result.is_ok(), "Connection test should succeed: {:?}", result.err());
        // Note: In simple implementation, this just validates the URL format
        // In full implementation, it would test actual RabbitMQ connection
    }
}

#[cfg(test)]
mod message_routing_tests {
    use super::*;

    #[tokio::test]
    async fn test_rabbitmq_direct_exchange() {
        let config = RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "direct_queue".to_string(),
            exchange_name: "direct_exchange".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("direct.key".to_string()),
        };

        let queue = RabbitMQQueueSimple::new(config).await.unwrap();
        assert_eq!(queue.config.exchange_type, ExchangeType::Direct);

        let message = QueueMessage::builder()
            .payload("Direct exchange message")
            .expect("Failed to serialize payload")
            .routing_key("direct.key")
            .build();

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Should enqueue message for direct exchange");
    }

    #[tokio::test]
    async fn test_rabbitmq_fanout_exchange() {
        let config = RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "fanout_queue".to_string(),
            exchange_name: "fanout_exchange".to_string(),
            exchange_type: ExchangeType::Fanout,
            routing_key: None,
        };

        let queue = RabbitMQQueueSimple::new(config).await.unwrap();
        assert_eq!(queue.config.exchange_type, ExchangeType::Fanout);

        let message = QueueMessage::builder()
            .payload("Fanout exchange message")
            .expect("Failed to serialize payload")
            .build();

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Should enqueue message for fanout exchange");
    }

    #[tokio::test]
    async fn test_rabbitmq_topic_exchange() {
        let config = RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "topic_queue".to_string(),
            exchange_name: "topic_exchange".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("logs.error".to_string()),
        };

        let queue = RabbitMQQueueSimple::new(config).await.unwrap();
        assert_eq!(queue.config.exchange_type, ExchangeType::Topic);

        let message = QueueMessage::builder()
            .payload("Topic exchange message")
            .expect("Failed to serialize payload")
            .routing_key("logs.error")
            .build();

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Should enqueue message for topic exchange");
    }
}

#[cfg(test)]
mod utility_tests {
    use super::*;
    use crate::rabbitmq_simple;

    #[test]
    fn test_dev_config_creation() {
        let config = rabbitmq_simple::dev_config("test_queue", "test_exchange", ExchangeType::Direct);

        assert_eq!(config.queue_name, "test_queue");
        assert_eq!(config.exchange_name, "test_exchange");
        assert_eq!(config.exchange_type, ExchangeType::Direct);
        assert_eq!(config.connection_url, "amqp://guest:guest@localhost:5672/%2f");
        assert_eq!(config.routing_key, Some("test_queue".to_string()));
    }

    #[test]
    fn test_prod_config_creation() {
        let config = rabbitmq_simple::prod_config(
            "amqps://user:pass@rabbitmq.example.com:5671/%2f",
            "prod_queue",
            "prod_exchange",
            ExchangeType::Topic,
        );

        assert_eq!(config.queue_name, "prod_queue");
        assert_eq!(config.exchange_name, "prod_exchange");
        assert_eq!(config.exchange_type, ExchangeType::Topic);
        assert_eq!(config.connection_url, "amqps://user:pass@rabbitmq.example.com:5671/%2f");
        assert_eq!(config.routing_key, Some("prod_queue".to_string()));
    }

    #[test]
    fn test_connection_url_validation() {
        // Test URL validation through config creation
        let config = rabbitmq_simple::dev_config("test_queue", "test_exchange", ExchangeType::Direct);
        assert!(config.connection_url.starts_with("amqp://"));
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[tokio::test]
    async fn test_rabbitmq_large_message_payload() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        // Create a large payload (10KB)
        let large_payload = "x".repeat(10 * 1024);
        let message = QueueMessage::builder()
            .payload(&large_payload)
            .expect("Failed to serialize payload")
            .build();

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Should handle large message payloads");
    }

    #[tokio::test]
    async fn test_rabbitmq_complex_json_payload() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let complex_payload = serde_json::json!({
            "user": {
                "id": 123,
                "name": "John Doe",
                "email": "john@example.com",
                "roles": ["user", "admin"],
                "metadata": {
                    "created_at": "2023-01-01T00:00:00Z",
                    "source": "web"
                }
            },
            "action": "user_created",
            "timestamp": 1672531200
        });

        let message = QueueMessage::builder()
            .payload(complex_payload)
            .expect("Failed to serialize payload")
            .build();

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Should handle complex JSON payloads");
    }

    #[tokio::test]
    async fn test_rabbitmq_message_with_headers() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        let mut headers = HashMap::new();
        headers.insert("trace-id".to_string(), serde_json::Value::String("abc123".to_string()));
        headers.insert("priority".to_string(), serde_json::Value::Number(serde_json::Number::from(5)));

        let message = QueueMessage {
            id: "test-msg".to_string(),
            payload: serde_json::Value::String("test".to_string()),
            priority: QueuePriority::High,
            receive_count: 0,
            max_receive_count: 3,
            enqueued_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            visible_at: chrono::Utc::now(),
            expires_at: None,
            visibility_timeout: 30,
            status: crate::types::MessageStatus::Pending,
            delay_seconds: None,
            attributes: HashMap::new(),
            headers,
            message_group_id: None,
            message_deduplication_id: None,
            routing_key: Some("test.routing".to_string()),
            compressed: false,
            original_size: None,
        };

        let result = queue.enqueue(message).await;
        assert!(result.is_ok(), "Should handle messages with headers");
    }

    #[tokio::test]
    async fn test_rabbitmq_batch_dequeue_various_sizes() {
        let config = RabbitMQConfig::default();
        let queue = RabbitMQQueueSimple::new(config).await.unwrap();

        // Test different batch sizes
        let batch_sizes = [0, 1, 5, 10, 100];

        for size in batch_sizes {
            let result = queue.dequeue_batch(size).await;
            assert!(result.is_ok(), "Should handle batch size {}: {:?}", size, result.err());

            let batch_result = result.unwrap();
            assert_eq!(batch_result.requested, size, "Should reflect requested size");
            assert_eq!(batch_result.messages.len(), 0, "Empty queue should return no messages");
        }
    }
}
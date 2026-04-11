//! Queue Manager Tests

use std::sync::Arc;
use tokio::test;
use chrono::Utc;

use crate::{
    QueueManager, QueueConfig, QueueService, QueueMessage, QueueResult, QueueError,
    queue_manager::{QueueAdminService, MaintenanceAction, MaintenanceResult},
    types::QueuePriority,
};

// Mock queue implementation for testing
struct MockQueue {
    name: String,
}

#[async_trait::async_trait]
impl QueueService for MockQueue {
    async fn enqueue(&self, _message: QueueMessage) -> QueueResult<String> {
        Ok("mock-message-id".to_string())
    }

    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>> {
        Ok(None)
    }

    async fn dequeue_batch(&self, _max_messages: usize, _wait_seconds: Option<u64>) -> QueueResult<Vec<QueueMessage>> {
        Ok(vec![])
    }

    async fn peek(&self) -> QueueResult<Option<QueueMessage>> {
        Ok(None)
    }

    async fn ack(&self, _message_id: &str) -> QueueResult<bool> {
        Ok(true)
    }

    async fn ack_batch(&self, _message_ids: Vec<String>) -> QueueResult<u64> {
        Ok(0)
    }

    async fn nack(&self, _message_id: &str, _delay_seconds: Option<u64>) -> QueueResult<bool> {
        Ok(true)
    }

    async fn delete(&self, _message_id: &str) -> QueueResult<bool> {
        Ok(true)
    }

    async fn delete_batch(&self, _message_ids: Vec<String>) -> QueueResult<u64> {
        Ok(0)
    }

    async fn size(&self) -> QueueResult<usize> {
        Ok(0)
    }

    async fn is_empty(&self) -> QueueResult<bool> {
        Ok(true)
    }

    async fn get_stats(&self) -> QueueResult<crate::QueueStats> {
        Ok(crate::QueueStats {
            total_messages: 0,
            pending_messages: 0,
            processing_messages: 0,
            completed_messages: 0,
            failed_messages: 0,
            dead_letter_messages: 0,
            average_processing_time_ms: 0.0,
            oldest_message_age_seconds: 0,
            newest_message_age_seconds: 0,
            queue_depth: 0,
            throughput_per_second: 0.0,
            error_rate: 0.0,
            last_updated: Utc::now(),
        })
    }

    async fn health_check(&self) -> QueueResult<crate::QueueHealth> {
        Ok(crate::QueueHealth {
            is_healthy: true,
            status: "healthy".to_string(),
            message: Some("Mock queue is healthy".to_string()),
            last_check: Utc::now(),
            uptime_seconds: 3600,
            response_time_ms: 10,
        })
    }

    async fn purge(&self) -> QueueResult<()> {
        Ok(())
    }

    async fn requeue(&self, _message_id: &str, _delay_seconds: Option<u64>) -> QueueResult<bool> {
        Ok(true)
    }

    async fn extend_visibility(&self, _message_id: &str, _additional_seconds: u64) -> QueueResult<bool> {
        Ok(true)
    }

    async fn change_priority(&self, _message_id: &str, _new_priority: QueuePriority) -> QueueResult<bool> {
        Ok(true)
    }
}

#[test]
async fn test_queue_manager_create() {
    let manager = QueueManager::new();
    let queues = manager.list_queues().await;
    assert!(queues.is_empty());
}

#[test]
async fn test_queue_config_validation() {
    // Valid config
    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );
    assert!(config.validate().is_ok());

    // Invalid: empty name
    config.name.clear();
    assert!(config.validate().is_err());

    // Invalid: empty type
    config.name = "test-queue".to_string();
    config.queue_type.clear();
    assert!(config.validate().is_err());

    // Invalid: empty URL
    config.queue_type = "redis".to_string();
    config.connection_url.clear();
    assert!(config.validate().is_err());

    // Invalid: zero visibility timeout
    config.connection_url = "redis://localhost:6379".to_string();
    config.visibility_timeout = 0;
    assert!(config.validate().is_err());

    // Invalid: zero max receive count
    config.visibility_timeout = 30;
    config.max_receive_count = 0;
    assert!(config.validate().is_err());
}

#[test]
async fn test_register_queue() {
    let manager = QueueManager::new();

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    // Register queue
    let result = manager.register_queue(mock_queue.clone(), config.clone()).await;
    assert!(result.is_ok());

    // Verify queue is listed
    let queues = manager.list_queues().await;
    assert_eq!(queues.len(), 1);
    assert!(queues.contains(&"test-queue".to_string()));

    // Verify we can get the queue
    let retrieved_queue = manager.get_queue("test-queue").await;
    assert!(retrieved_queue.is_ok());

    // Verify we can get the config
    let retrieved_config = manager.get_config("test-queue").await;
    assert!(retrieved_config.is_ok());
    assert_eq!(retrieved_config.unwrap().name, "test-queue");
}

#[test]
async fn test_unregister_queue() {
    let manager = QueueManager::new();

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    // Register queue
    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    // Verify queue exists
    let queues = manager.list_queues().await;
    assert_eq!(queues.len(), 1);

    // Unregister queue
    let result = manager.unregister_queue("test-queue").await;
    assert!(result.is_ok());

    // Verify queue is removed
    let queues = manager.list_queues().await;
    assert!(queues.is_empty());

    // Verify queue is no longer accessible
    let retrieved_queue = manager.get_queue("test-queue").await;
    assert!(retrieved_queue.is_err());
    assert!(matches!(retrieved_queue.unwrap_err(), QueueError::NotFound(_)));
}

#[test]
async fn test_get_nonexistent_queue() {
    let manager = QueueManager::new();

    let result = manager.get_queue("nonexistent").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), QueueError::NotFound(_)));
}

#[test]
async fn test_get_queue_stats() {
    let manager = QueueManager::new();

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    let stats = manager.get_stats("test-queue").await;
    assert!(stats.is_ok());

    let stats = stats.unwrap();
    assert_eq!(stats.total_messages, 0);
    assert_eq!(stats.pending_messages, 0);
}

#[test]
async fn test_get_queue_health() {
    let manager = QueueManager::new();

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    let health = manager.get_health("test-queue").await;
    assert!(health.is_ok());

    let health = health.unwrap();
    assert!(health.is_healthy);
    assert_eq!(health.status, "healthy");
}

#[test]
async fn test_purge_queue() {
    let manager = QueueManager::new();

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    let result = manager.purge_queue("test-queue").await;
    assert!(result.is_ok());
}

#[test]
async fn test_list_configs() {
    let manager = QueueManager::new();

    let config1 = QueueConfig::new(
        "test-queue-1".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let config2 = QueueConfig::new(
        "test-queue-2".to_string(),
        "sqs".to_string(),
        "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue-2".to_string(),
    );

    let mock_queue1 = Arc::new(MockQueue {
        name: "test-queue-1".to_string(),
    });

    let mock_queue2 = Arc::new(MockQueue {
        name: "test-queue-2".to_string(),
    });

    manager.register_queue(mock_queue1, config1.clone()).await.unwrap();
    manager.register_queue(mock_queue2, config2.clone()).await.unwrap();

    let configs = manager.list_configs().await;
    assert_eq!(configs.len(), 2);

    let config_names: Vec<String> = configs.iter().map(|c| c.name.clone()).collect();
    assert!(config_names.contains(&"test-queue-1".to_string()));
    assert!(config_names.contains(&"test-queue-2".to_string()));
}

#[test]
async fn test_update_config() {
    let manager = QueueManager::new();

    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    // Update config
    config.visibility_timeout = 60;
    config.max_receive_count = 5;

    let result = manager.update_config("test-queue", config.clone()).await;
    assert!(result.is_ok());

    // Verify update
    let retrieved_config = manager.get_config("test-queue").await.unwrap();
    assert_eq!(retrieved_config.visibility_timeout, 60);
    assert_eq!(retrieved_config.max_receive_count, 5);
}

#[test]
async fn test_get_all_health() {
    let manager = QueueManager::new();

    let config1 = QueueConfig::new(
        "test-queue-1".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let config2 = QueueConfig::new(
        "test-queue-2".to_string(),
        "sqs".to_string(),
        "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue-2".to_string(),
    );

    let mock_queue1 = Arc::new(MockQueue {
        name: "test-queue-1".to_string(),
    });

    let mock_queue2 = Arc::new(MockQueue {
        name: "test-queue-2".to_string(),
    });

    manager.register_queue(mock_queue1, config1.clone()).await.unwrap();
    manager.register_queue(mock_queue2, config2.clone()).await.unwrap();

    let all_health = manager.get_all_health().await;
    assert_eq!(all_health.len(), 2);

    for (queue_name, health_result) in all_health {
        assert!(health_result.is_ok());
        let health = health_result.unwrap();
        assert!(health.is_healthy);
        assert!(queue_name.starts_with("test-queue-"));
    }
}

#[test]
async fn test_queue_maintenance() {
    let manager = QueueManager::new();

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    let result = manager.perform_queue_maintenance("test-queue").await;
    assert!(result.success);
    assert!(!result.actions.is_empty());
    assert!(result.duration_ms > 0);
}

#[test]
async fn test_admin_service_trait() {
    let manager = Arc::new(QueueManager::new());

    let config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let mock_queue = Arc::new(MockQueue {
        name: "test-queue".to_string(),
    });

    // Test create queue (not implemented in mock)
    let result = manager.create_queue(config.clone()).await;
    assert!(result.is_err());

    // Register queue manually for other tests
    manager.register_queue(mock_queue.clone(), config.clone()).await.unwrap();

    // Test list queues
    let queues = manager.list_queues().await;
    assert!(queues.is_ok());
    assert_eq!(queues.unwrap().len(), 1);

    // Test get config
    let retrieved_config = manager.get_queue_config("test-queue").await;
    assert!(retrieved_config.is_ok());
    assert_eq!(retrieved_config.unwrap().name, "test-queue");

    // Test get stats
    let stats = manager.get_queue_stats("test-queue").await;
    assert!(stats.is_ok());

    // Test get health
    let health = manager.get_queue_health("test-queue").await;
    assert!(health.is_ok());

    // Test purge queue
    let purge_result = manager.purge_queue("test-queue").await;
    assert!(purge_result.is_ok());

    // Test maintenance
    let maintenance_result = manager.perform_maintenance(None).await;
    assert!(maintenance_result.is_ok());
    assert_eq!(maintenance_result.unwrap().len(), 1);
}

#[test]
async fn test_config_touch() {
    let mut config = QueueConfig::new(
        "test-queue".to_string(),
        "redis".to_string(),
        "redis://localhost:6379".to_string(),
    );

    let original_updated = config.updated_at;

    // Wait a bit to ensure different timestamp
    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

    config.touch();

    assert!(config.updated_at > original_updated);
}

#[test]
async fn test_maintenance_result_serialization() {
    let result = MaintenanceResult {
        queue_name: "test-queue".to_string(),
        success: true,
        duration_ms: 100,
        actions: vec![
            MaintenanceAction::CleanupExpired,
            MaintenanceAction::UpdateMetrics,
        ],
        error_message: None,
    };

    // Test JSON serialization
    let json = serde_json::to_string(&result);
    assert!(json.is_ok());

    // Test deserialization
    let deserialized: Result<MaintenanceResult, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());

    let deserialized_result = deserialized.unwrap();
    assert_eq!(deserialized_result.queue_name, "test-queue");
    assert_eq!(deserialized_result.actions.len(), 2);
}
//! Unit tests for FIFO queue module

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QueueMessage, QueuePriority, MessageStatus};
    use std::collections::HashMap;
    use chrono::Utc;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock queue service for testing
    struct MockQueueService {
        messages: Arc<Mutex<Vec<QueueMessage>>>,
    }

    impl MockQueueService {
        fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn enqueue(&self, message: QueueMessage) -> crate::QueueResult<String> {
            let mut messages = self.messages.lock().unwrap();
            let message_id = format!("msg-{}", messages.len());
            let mut message = message;
            message.id = message_id.clone();
            messages.push(message);
            Ok(message_id)
        }

        async fn dequeue(&self) -> crate::QueueResult<Option<QueueMessage>> {
            let mut messages = self.messages.lock().unwrap();
            Ok(messages.pop())
        }
    }

    /// Create test FIFO message
    fn create_test_fifo_message(
        id: &str,
        group_id: &str,
        deduplication_id: &str,
        payload: serde_json::Value,
    ) -> QueueMessage {
        QueueMessage {
            id: id.to_string(),
            payload,
            priority: QueuePriority::Normal,
            receive_count: 0,
            max_receive_count: 3,
            enqueued_at: Utc::now(),
            visible_at: Utc::now(),
            expires_at: None,
            visibility_timeout: 30,
            status: MessageStatus::Pending,
            delay_seconds: None,
            attributes: HashMap::new(),
            message_group_id: Some(group_id.to_string()),
            message_deduplication_id: Some(deduplication_id.to_string()),
            compressed: false,
            original_size: None,
        }
    }

    #[tokio::test]
    async fn test_fifo_config_default() {
        let config = FifoQueueConfig::default();
        assert!(config.enabled);
        assert_eq!(config.deduplication_window_seconds, 300);
        assert_eq!(config.max_message_groups, 10000);
        assert!(!config.enable_content_deduplication);
        assert_eq!(config.content_deduplication_window_seconds, 60);
        assert_eq!(config.max_deduplicated_messages, 100000);
    }

    #[tokio::test]
    async fn test_fifo_service_wrapper_creation() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        assert!(fifo_service.config.enabled);
        assert_eq!(fifo_service.stats.read().await.total_groups, 0);
    }

    #[tokio::test]
    async fn test_fifo_service_wrapper_with_default_config() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::with_default_config(mock_service);

        assert!(fifo_service.config.enabled);
    }

    #[tokio::test]
    async fn test_validate_fifo_message_success() {
        let config = FifoQueueConfig::default();
        let service = FifoQueueServiceWrapper::new(
            Arc::new(MockQueueService::new()),
            config,
        );

        let message = create_test_fifo_message(
            "test-1",
            "group-1",
            "dedup-1",
            serde_json::json!({"test": "data"}),
        );

        let result = service.validate_fifo_message(&message);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_fifo_message_no_group_id() {
        let config = FifoQueueConfig::default();
        let service = FifoQueueServiceWrapper::new(
            Arc::new(MockQueueService::new()),
            config,
        );

        let mut message = create_test_fifo_message(
            "test-1",
            "group-1",
            "dedup-1",
            serde_json::json!({"test": "data"}),
        );
        message.message_group_id = None;

        let result = service.validate_fifo_message(&message);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("message_group_id"));
    }

    #[tokio::test]
    async fn test_validate_fifo_message_no_deduplication_id() {
        let config = FifoQueueConfig::default();
        let service = FifoQueueServiceWrapper::new(
            Arc::new(MockQueueService::new()),
            config,
        );

        let mut message = create_test_fifo_message(
            "test-1",
            "group-1",
            "dedup-1",
            serde_json::json!({"test": "data"}),
        );
        message.message_deduplication_id = None;

        let result = service.validate_fifo_message(&message);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("message_deduplication_id"));
    }

    #[tokio::test]
    async fn test_validate_fifo_message_empty_deduplication_id() {
        let config = FifoQueueConfig::default();
        let service = FifoQueueServiceWrapper::new(
            Arc::new(MockQueueService::new()),
            config,
        );

        let mut message = create_test_fifo_message(
            "test-1",
            "group-1",
            "", // Empty deduplication ID
            serde_json::json!({"test": "data"}),
        );
        // Keep the field but make it empty
        message.message_deduplication_id = Some("".to_string());

        let result = service.validate_fifo_message(&message);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_fifo_message_too_long_deduplication_id() {
        let config = FifoQueueConfig::default();
        let service = FifoQueueServiceWrapper::new(
            Arc::new(MockQueueService::new()),
            config,
        );

        let long_id = "a".repeat(200); // 200 characters
        let mut message = create_test_fifo_message(
            "test-1",
            "group-1",
            &long_id,
            serde_json::json!({"test": "data"}),
        );
        message.message_deduplication_id = Some(long_id);

        let result = service.validate_fifo_message(&message);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed 128 characters"));
    }

    #[tokio::test]
    async fn test_generate_content_hash() {
        let content1 = serde_json::json!({"message": "hello", "timestamp": 12345});
        let content2 = serde_json::json!({"message": "hello", "timestamp": 12345});
        let content3 = serde_json::json!({"message": "world", "timestamp": 12345});

        let hash1 = FifoQueueServiceWrapper::generate_content_hash(&content1);
        let hash2 = FifoQueueServiceWrapper::generate_content_hash(&content2);
        let hash3 = FifoQueueServiceWrapper::generate_content_hash(&content3);

        // Same content should produce same hash
        assert_eq!(hash1, hash2);
        // Different content should produce different hash
        assert_ne!(hash1, hash3);
    }

    #[tokio::test]
    async fn test_enqueue_fifo_success() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        let message = create_test_fifo_message(
            "test-1",
            "group-1",
            "dedup-1",
            serde_json::json!({"test": "data"}),
        );

        let result = fifo_service.enqueue_fifo(message).await;
        assert!(result.is_ok());

        let message_id = result.unwrap();
        assert!(message_id.starts_with("msg-"));

        // Check statistics
        let stats = fifo_service.stats.read().await;
        assert_eq!(stats.total_groups, 1);
        assert!(stats.deduplication_cache.contains_key("dedup-1"));
    }

    #[tokio::test]
    async fn test_enqueue_fifo_duplicate_message() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        let message1 = create_test_fifo_message(
            "test-1",
            "group-1",
            "same-dedup",
            serde_json::json!({"test": "data1"}),
        );

        let message2 = create_test_fifo_message(
            "test-2",
            "group-1",
            "same-dedup", // Same deduplication ID
            serde_json::json!({"test": "data2"}),
        );

        // Enqueue first message
        let result1 = fifo_service.enqueue_fifo(message1).await;
        assert!(result1.is_ok());

        // Try to enqueue duplicate message
        let result2 = fifo_service.enqueue_fifo(message2).await;
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_enqueue_fifo_content_deduplication() {
        let config = FifoQueueConfig {
            enabled: true,
            deduplication_window_seconds: 300,
            max_message_groups: 10000,
            enable_content_deduplication: true, // Enable content deduplication
            content_deduplication_window_seconds: 60,
            max_deduplicated_messages: 100000,
        };

        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(mock_service.clone(), config);

        let content = serde_json::json!({"message": "hello", "data": "test"});

        let message1 = create_test_fifo_message(
            "test-1",
            "group-1",
            "dedup-1",
            content.clone(),
        );

        let message2 = create_test_fifo_message(
            "test-2",
            "group-1",
            "dedup-2", // Different deduplication ID but same content
            content,
        );

        // Enqueue first message
        let result1 = fifo_service.enqueue_fifo(message1).await;
        assert!(result1.is_ok());

        // Try to enqueue message with same content
        let result2 = fifo_service.enqueue_fifo(message2).await;
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("identical content already exists"));
    }

    #[tokio::test]
    async fn test_enqueue_fifo_batch() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        let messages = vec![
            create_test_fifo_message(
                "test-1",
                "group-1",
                "dedup-1",
                serde_json::json!({"test": "data1"}),
            ),
            create_test_fifo_message(
                "test-2",
                "group-2",
                "dedup-2",
                serde_json::json!({"test": "data2"}),
            ),
            create_test_fifo_message(
                "test-3",
                "group-1",
                "dedup-3",
                serde_json::json!({"test": "data3"}),
            ),
        ];

        let result = fifo_service.enqueue_fifo_batch(messages).await;
        assert!(result.is_ok());

        let message_ids = result.unwrap();
        assert_eq!(message_ids.len(), 3);

        // Check statistics
        let stats = fifo_service.stats.read().await;
        assert_eq!(stats.total_groups, 3);
        assert_eq!(stats.deduplication_cache.len(), 3);
    }

    #[tokio::test]
    async fn test_is_message_duplicated() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        // Initially not duplicated
        assert!(!fifo_service.is_message_duplicated("test-dedup").await.unwrap());

        // Add to deduplication cache
        {
            let mut stats = fifo_service.stats.write().await;
            stats.deduplication_cache.insert(
                "test-dedup".to_string(),
                Utc::now(),
            );
        }

        // Now should be duplicated
        assert!(fifo_service.is_message_duplicated("test-dedup").await.unwrap());
    }

    #[tokio::test]
    async fn test_is_content_duplicated() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        let content = serde_json::json!({"test": "data"});

        // Initially not duplicated
        assert!(!fifo_service.is_content_duplicated("test content").await.unwrap());

        // Add content hash to cache
        let content_hash = FifoQueueServiceWrapper::generate_content_hash(&content);
        {
            let mut stats = fifo_service.stats.write().await;
            stats.content_deduplication_cache.insert(
                content_hash,
                Utc::now(),
            );
        }

        // Now should be duplicated
        assert!(fifo_service.is_content_duplicated("test content").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_group_stats() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        // Initially no stats
        let result = fifo_service.get_group_stats("group-1").await.unwrap();
        assert!(result.is_none());

        // Update group stats
        {
            let mut stats = fifo_service.stats.write().await;
            stats.update_group("group-1", true, 100);
            stats.update_group("group-1", false, 200);
        }

        // Now should have stats
        let result = fifo_service.get_group_stats("group-1").await.unwrap();
        assert!(result.is_some());

        let group_stats = result.unwrap();
        assert_eq!(group_stats.message_count, 2);
        assert_eq!(group_stats.processed_count, 1);
        assert_eq!(group_stats.failed_count, 1);
        assert_eq!(group_stats.success_rate(), 50.0);
        assert_eq!(group_stats.avg_processing_time_ms, 150.0); // (100 + 200) / 2
    }

    #[tokio::test]
    async fn test_get_all_group_stats() {
        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(
            mock_service.clone(),
            FifoQueueConfig::default(),
        );

        // Update multiple group stats
        {
            let mut stats = fifo_service.stats.write().await;
            stats.update_group("group-1", true, 100);
            stats.update_group("group-2", true, 150);
            stats.update_group("group-1", false, 200);
        }

        let all_stats = fifo_service.get_all_group_stats().await.unwrap();
        assert_eq!(all_stats.len(), 2);
        assert!(all_stats.contains_key("group-1"));
        assert!(all_stats.contains_key("group-2"));

        let group1_stats = &all_stats["group-1"];
        assert_eq!(group1_stats.message_count, 2);
        let group2_stats = &all_stats["group-2"];
        assert_eq!(group2_stats.message_count, 1);
    }

    #[tokio::test]
    async fn test_cleanup_deduplication() {
        let config = FifoQueueConfig {
            enabled: true,
            deduplication_window_seconds: 1, // Very short window
            max_message_groups: 100,
            enable_content_deduplication: true,
            content_deduplication_window_seconds: 1,
            max_deduplicated_messages: 10,
        };

        let mock_service = Arc::new(MockQueueService::new());
        let fifo_service = FifoQueueServiceWrapper::new(mock_service.clone(), config);

        // Add some expired entries
        {
            let mut stats = fifo_service.stats.write().await;
            let past_time = Utc::now() - chrono::Duration::seconds(10); // 10 seconds ago

            stats.deduplication_cache.insert("old-dedup".to_string(), past_time);
            stats.content_deduplication_cache.insert("old-content".to_string(), past_time);

            // Add recent entries
            let recent_time = Utc::now();
            stats.deduplication_cache.insert("new-dedup".to_string(), recent_time);
            stats.content_deduplication_cache.insert("new-content".to_string(), recent_time);
        }

        // Cleanup should remove expired entries
        let removed_count = fifo_service.cleanup_deduplication().await.unwrap();
        assert_eq!(removed_count, 2); // Two expired entries should be removed

        // Verify expired entries are gone
        let stats = fifo_service.stats.read().await;
        assert!(!stats.deduplication_cache.contains_key("old-dedup"));
        assert!(!stats.content_deduplication_cache.contains_key("old-content"));
        assert!(stats.deduplication_cache.contains_key("new-dedup"));
        assert!(stats.content_deduplication_cache.contains_key("new-content"));
    }

    #[tokio::test]
    async fn test_message_group_stats_new() {
        let stats = MessageGroupStats::new();
        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.processed_count, 0);
        assert_eq!(stats.failed_count, 0);
        assert_eq!(stats.success_rate(), 0.0);
        assert!(stats.created_at <= Utc::now());
        assert!(stats.last_message_at.is_none());
    }

    #[tokio::test]
    async fn test_message_group_stats_update() {
        let mut stats = MessageGroupStats::new();

        stats.update_group("test", true, 100);
        assert_eq!(stats.message_count, 1);
        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.failed_count, 0);
        assert_eq!(stats.success_rate(), 100.0);
        assert_eq!(stats.avg_processing_time_ms, 100.0);

        stats.update_group("test", false, 200);
        assert_eq!(stats.message_count, 2);
        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.success_rate(), 50.0);
        assert_eq!(stats.avg_processing_time_ms, 150.0); // (100 + 200) / 2
    }

    #[test]
    fn test_utils_validate_config_valid() {
        let config = FifoQueueConfig::default();
        let errors = utils::validate_config(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_utils_validate_config_invalid() {
        let mut config = FifoQueueConfig::default();
        config.deduplication_window_seconds = 0; // Invalid

        let errors = utils::validate_config(&config);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("Deduplication window must be greater than 0")));
    }

    #[test]
    fn test_utils_get_recommended_config_low_volume() {
        let config = utils::get_recommended_config(utils::MessageVolume::Low);
        assert_eq!(config.deduplication_window_seconds, 300);
        assert_eq!(config.max_message_groups, 1000);
        assert!(!config.enable_content_deduplication);
    }

    #[test]
    fn test_utils_get_recommended_config_medium_volume() {
        let config = utils::get_recommended_config(utils::MessageVolume::Medium);
        assert_eq!(config.deduplication_window_seconds, 900);
        assert_eq!(config.max_message_groups, 10000);
        assert!(config.enable_content_deduplication);
        assert_eq!(config.content_deduplication_window_seconds, 300);
    }

    #[test]
    fn test_utils_get_recommended_config_high_volume() {
        let config = utils::get_recommended_config(utils::MessageVolume::High);
        assert_eq!(config.deduplication_window_seconds, 3600);
        assert_eq!(config.max_message_groups, 50000);
        assert!(config.enable_content_deduplication);
        assert_eq!(config.content_deduplication_window_seconds, 1800);
    }

    #[test]
    fn test_fifo_queue_stats_new() {
        let stats = FifoQueueStats::new();
        assert_eq!(stats.total_groups, 0);
        assert_eq!(stats.active_groups, 0);
        assert_eq!(stats.deduplicated_messages, 0);
        assert_eq!(stats.out_of_order_messages, 0);
        assert!(stats.group_stats.is_empty());
        assert!(stats.deduplication_cache.is_empty());
        assert!(stats.content_deduplication_cache.is_empty());
        assert!(stats.last_updated <= Utc::now());
    }

    #[test]
    fn test_fifo_queue_stats_cleanup_expired_entries() {
        let mut stats = FifoQueueStats::new();

        // Add config with short windows
        let config = FifoQueueConfig {
            deduplication_window_seconds: 1,
            content_deduplication_window_seconds: 1,
            max_message_groups: 2,
            enable_content_deduplication: true,
            max_deduplicated_messages: 2,
            enabled: true,
        };

        // Add expired entries
        let past_time = Utc::now() - chrono::Duration::seconds(10);
        stats.deduplication_cache.insert("old1".to_string(), past_time);
        stats.deduplication_cache.insert("old2".to_string(), past_time);
        stats.content_deduplication_cache.insert("old3".to_string(), past_time);

        // Add recent entries
        let recent_time = Utc::now();
        stats.deduplication_cache.insert("new1".to_string(), recent_time);
        stats.content_deduplication_cache.insert("new2".to_string(), recent_time);

        // Add too many entries
        for i in 0..5 {
            stats.group_stats.insert(format!("group-{}", i), MessageGroupStats::new());
        }

        stats.cleanup_expired_entries(&config);

        // Should keep only recent entries and respect limits
        assert!(stats.deduplication_cache.contains_key("new1"));
        assert!(stats.content_deduplication_cache.contains_key("new2"));
        assert!(!stats.deduplication_cache.contains_key("old1"));
        assert!(!stats.content_deduplication_cache.contains_key("old3"));
        assert_eq!(stats.group_stats.len(), 2); // Max groups limited
    }
}
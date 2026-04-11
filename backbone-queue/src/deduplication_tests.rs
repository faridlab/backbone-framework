//! Deduplication Module Tests

use std::time::Duration;

use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    deduplication::{
        MessageDeduplicator, DeduplicationConfig, DeduplicationStrategy, DeduplicationCache,
        ProcessingResult, ProcessingStatus,
    },
    QueueMessage,
};

#[tokio::test]
async fn test_memory_deduplication_basic() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let mut message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));
    message.message_deduplication_id = Some("test-dedup-1".to_string());

    // Should not be duplicate initially
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());

    // Record message
    deduplicator.record_message(&message).await.unwrap();

    // Now should be duplicate
    assert!(deduplicator.is_duplicate(&message).await.unwrap());

    // Different message with same deduplication ID should be duplicate
    let mut message2 = QueueMessage::new("test2".to_string(), serde_json::json!({"different": "data"}));
    message2.message_deduplication_id = Some("test-dedup-1".to_string());
    assert!(deduplicator.is_duplicate(&message2).await.unwrap());

    // Different deduplication ID should not be duplicate
    let mut message3 = QueueMessage::new("test3".to_string(), serde_json::json!({"another": "data"}));
    message3.message_deduplication_id = Some("test-dedup-2".to_string());
    assert!(!deduplicator.is_duplicate(&message3).await.unwrap());
}

#[tokio::test]
async fn test_content_deduplication() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::Content,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let payload = serde_json::json!({"test": "data", "number": 42});
    let mut message = QueueMessage::new("test1".to_string(), payload.clone());

    // Should not be duplicate initially
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());

    // Record message
    deduplicator.record_message(&message).await.unwrap();

    // Same content should be duplicate
    let mut message2 = QueueMessage::new("test2".to_string(), payload);
    assert!(deduplicator.is_duplicate(&message2).await.unwrap());

    // Different content should not be duplicate
    let different_payload = serde_json::json!({"test": "data", "number": 43});
    let mut message3 = QueueMessage::new("test3".to_string(), different_payload);
    assert!(!deduplicator.is_duplicate(&message3).await.unwrap());
}

#[tokio::test]
async fn test_both_deduplication_strategy() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::Both,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let payload = serde_json::json!({"test": "data"});
    let mut message = QueueMessage::new("test1".to_string(), payload.clone());
    message.message_deduplication_id = Some("dedup-1".to_string());

    // Record original message
    deduplicator.record_message(&message).await.unwrap();

    // Same deduplication ID should be duplicate
    let mut message2 = QueueMessage::new("test2".to_string(), serde_json::json!({"different": "data"}));
    message2.message_deduplication_id = Some("dedup-1".to_string());
    assert!(deduplicator.is_duplicate(&message2).await.unwrap());

    // Same content should be duplicate
    let mut message3 = QueueMessage::new("test3".to_string(), payload);
    message3.message_deduplication_id = Some("dedup-2".to_string());
    assert!(deduplicator.is_duplicate(&message3).await.unwrap());

    // Different both should not be duplicate
    let mut message4 = QueueMessage::new("test4".to_string(), serde_json::json!({"completely": "different"}));
    message4.message_deduplication_id = Some("dedup-3".to_string());
    assert!(!deduplicator.is_duplicate(&message4).await.unwrap());
}

#[tokio::test]
async fn test_none_deduplication_strategy() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::None,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));

    // Should never be duplicate with None strategy
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());
}

#[tokio::test]
async fn test_exactly_once_processing() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 300,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let mut message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));
    message.message_deduplication_id = Some("dedup-1".to_string());

    let processor_id = "test-processor";

    // Start processing
    let processing_id = deduplicator.start_processing(&message, processor_id).await.unwrap();

    // Second attempt to start processing should fail
    let result = deduplicator.start_processing(&message, processor_id).await;
    assert!(result.is_err());

    // Complete processing successfully
    deduplicator.complete_processing(&processing_id, ProcessingResult::Success, None)
        .await
        .unwrap();

    // Check statistics
    let stats = deduplicator.get_stats().await;
    assert_eq!(stats.exactly_once_violations_prevented, 1);
    assert_eq!(stats.completed_processing_records, 1);
}

#[tokio::test]
async fn test_exactly_once_processing_failure() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 300,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let mut message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));
    message.message_deduplication_id = Some("dedup-1".to_string());

    let processor_id = "test-processor";

    // Start processing
    let processing_id = deduplicator.start_processing(&message, processor_id).await.unwrap();

    // Complete processing with failure
    deduplicator.complete_processing(
        &processing_id,
        ProcessingResult::Failed,
        Some("Test error".to_string()),
    )
    .await
    .unwrap();

    // Check statistics
    let stats = deduplicator.get_stats().await;
    assert_eq!(stats.failed_processing_records, 1);
}

#[tokio::test]
async fn test_deduplication_expiration() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 1, // 1 second
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let mut message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));
    message.message_deduplication_id = Some("dedup-1".to_string());

    // Record message
    deduplicator.record_message(&message).await.unwrap();
    assert!(deduplicator.is_duplicate(&message).await.unwrap());

    // Wait for expiration
    sleep(Duration::from_secs(2)).await;

    // Clean up expired entries
    let cleaned = deduplicator.cleanup_expired().await.unwrap();
    assert!(cleaned > 0);

    // Should no longer be duplicate
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());
}

#[tokio::test]
async fn test_cache_size_limits() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 2, // Very small cache
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    // Add more entries than cache limit
    for i in 0..5 {
        let mut message = QueueMessage::new(format!("test-{}", i), serde_json::json!({"test": i}));
        message.message_deduplication_id = Some(format!("dedup-{}", i));
        deduplicator.record_message(&message).await.unwrap();
    }

    // Check that cache size is limited
    let stats = deduplicator.get_stats().await;
    assert!(stats.cache_size <= 2);
}

#[tokio::test]
async fn test_deduplication_statistics() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 300,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let mut message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));
    message.message_deduplication_id = Some("dedup-1".to_string());

    // Initial stats
    let stats = deduplicator.get_stats().await;
    assert_eq!(stats.total_messages, 0);
    assert_eq!(stats.duplicates_detected, 0);
    assert_eq!(stats.cache_hits, 0);
    assert_eq!(stats.cache_misses, 0);

    // First check - should be cache miss
    assert!(!deduplicator.is_duplicate(&message).await.unwrap());
    let stats = deduplicator.get_stats().await;
    assert_eq!(stats.total_messages, 1);
    assert_eq!(stats.cache_misses, 1);

    // Record message
    deduplicator.record_message(&message).await.unwrap();

    // Second check - should be cache hit
    assert!(deduplicator.is_duplicate(&message).await.unwrap());
    let stats = deduplicator.get_stats().await;
    assert_eq!(stats.total_messages, 2);
    assert_eq!(stats.duplicates_detected, 1);
    assert_eq!(stats.cache_hits, 1);

    // Test duplicate rate calculation
    assert!(stats.duplicate_rate() > 0.0);
    assert!(stats.cache_hit_rate() > 0.0);
}

#[tokio::test]
async fn test_multiple_processors_exactly_once() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 60,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 300,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let mut message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));
    message.message_deduplication_id = Some("dedup-1".to_string());

    let processor1 = "processor-1";
    let processor2 = "processor-2";

    // Processor 1 starts processing
    let processing_id_1 = deduplicator.start_processing(&message, processor1).await.unwrap();

    // Processor 2 should not be able to start processing the same message
    let result = deduplicator.start_processing(&message, processor2).await;
    assert!(result.is_err());

    // Complete processor 1
    deduplicator.complete_processing(&processing_id_1, ProcessingResult::Success, None)
        .await
        .unwrap();

    // Now processor 2 should be able to process (with a new message instance)
    let mut message2 = QueueMessage::new("test2".to_string(), serde_json::json!({"test": "data2"}));
    message2.message_deduplication_id = Some("dedup-1".to_string());

    let processing_id_2 = deduplicator.start_processing(&message2, processor2).await.unwrap();
    assert_ne!(processing_id_1, processing_id_2);
}

#[tokio::test]
async fn test_no_deduplication_id_error() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    // Message without deduplication ID should cause error when trying to record
    let message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));

    let result = deduplicator.record_message(&message).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_exactly_once_disabled_error() {
    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false, // Disabled
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config).unwrap();

    let message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));

    // Should error when exactly-once is disabled
    let result = deduplicator.start_processing(&message, "processor").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_config_validation() {
    // Test valid configs
    let valid_config = DeduplicationConfig::default();
    assert!(MessageDeduplicator::new(valid_config).is_ok());

    let config_with_custom_cache = DeduplicationConfig {
        strategy: DeduplicationStrategy::Content,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 5000,
        enable_exactly_once: true,
        ..Default::default()
    };
    assert!(MessageDeduplicator::new(config_with_custom_cache).is_ok());

    // Test invalid Redis cache (not implemented yet)
    let redis_config = DeduplicationConfig {
        cache_backend: DeduplicationCache::Redis(redis::Client::open("redis://localhost").unwrap()),
        ..Default::default()
    };
    assert!(MessageDeduplicator::new(redis_config).is_err());
}
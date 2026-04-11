//! Message Processor Module Tests

use std::sync::Arc;
use std::time::Duration;

use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    processor::{
        MessageProcessor, ProcessingOutcome, ProcessedMessage, BatchProcessingResult,
        ProcessingContext, BatchConfig, BatchTimeoutPolicy, RetryConfig, RetryPolicy,
        RetryHandler, BatchingProcessor, SimpleMessageProcessor,
    },
    QueueMessage,
};

#[tokio::test]
async fn test_simple_message_processor() {
    let processor = SimpleMessageProcessor::success_processor();

    let message = QueueMessage::new("test-1".to_string(), serde_json::json!({"test": "data"}));

    let context = ProcessingContext {
        processing_id: Uuid::new_v4().to_string(),
        processor_id: "test-processor".to_string(),
        start_time: std::time::Instant::now(),
        attempt_number: 1,
        max_attempts: 3,
        batch_context: None,
        metadata: std::collections::HashMap::new(),
    };

    let processed = processor.process_message(message, context).await;

    assert_eq!(processed.outcome, ProcessingOutcome::Success);
    assert!(processed.processing_time.as_millis() > 0);
    assert!(processed.result_data.is_some());

    // Check statistics
    let stats = processor.get_stats().await;
    assert_eq!(stats.total_messages_processed, 1);
    assert_eq!(stats.total_messages_succeeded, 1);
    assert_eq!(stats.total_messages_failed, 0);
    assert_eq!(stats.success_rate(), 1.0);
}

#[tokio::test]
async fn test_failure_processor() {
    let processor = SimpleMessageProcessor::failure_processor();

    let message = QueueMessage::new("test-1".to_string(), serde_json::json!({"test": "data"}));

    let context = ProcessingContext {
        processing_id: Uuid::new_v4().to_string(),
        processor_id: "test-processor".to_string(),
        start_time: std::time::Instant::now(),
        attempt_number: 1,
        max_attempts: 3,
        batch_context: None,
        metadata: std::collections::HashMap::new(),
    };

    let processed = processor.process_message(message, context).await;

    assert_eq!(processed.outcome, ProcessingOutcome::Failed);

    // Check statistics
    let stats = processor.get_stats().await;
    assert_eq!(stats.total_messages_processed, 1);
    assert_eq!(stats.total_messages_succeeded, 0);
    assert_eq!(stats.total_messages_failed, 1);
    assert_eq!(stats.failure_rate(), 1.0);
}

#[tokio::test]
async fn test_batch_processing_default() {
    let processor = Arc::new(SimpleMessageProcessor::success_processor());

    let messages = vec![
        QueueMessage::new("msg-1".to_string(), serde_json::json!({"id": 1})),
        QueueMessage::new("msg-2".to_string(), serde_json::json!({"id": 2})),
        QueueMessage::new("msg-3".to_string(), serde_json::json!({"id": 3})),
    ];

    let context = ProcessingContext {
        processing_id: Uuid::new_v4().to_string(),
        processor_id: "batch-test".to_string(),
        start_time: std::time::Instant::now(),
        attempt_number: 1,
        max_attempts: 3,
        batch_context: None,
        metadata: std::collections::HashMap::new(),
    };

    let result = processor.process_batch(messages, context).await;

    assert_eq!(result.total_messages, 3);
    assert_eq!(result.successful_messages.len(), 3);
    assert_eq!(result.failed_messages.len(), 0);
    assert_eq!(result.retried_messages.len(), 0);
    assert_eq!(result.success_rate(), 1.0);

    // Verify batch processing time is reasonable
    assert!(result.total_processing_time.as_millis() > 0);
}

#[tokio::test]
async fn test_batch_processing_with_failures() {
    let processor = Arc::new(SimpleMessageProcessor::random_processor(0.5)); // 50% failure rate

    let messages = vec![
        QueueMessage::new("msg-1".to_string(), serde_json::json!({"id": 1})),
        QueueMessage::new("msg-2".to_string(), serde_json::json!({"id": 2})),
        QueueMessage::new("msg-3".to_string(), serde_json::json!({"id": 3})),
        QueueMessage::new("msg-4".to_string(), serde_json::json!({"id": 4})),
    ];

    let context = ProcessingContext {
        processing_id: Uuid::new_v4().to_string(),
        processor_id: "random-test".to_string(),
        start_time: std::time::Instant::now(),
        attempt_number: 1,
        max_attempts: 3,
        batch_context: None,
        metadata: std::collections::HashMap::new(),
    };

    let result = processor.process_batch(messages, context).await;

    assert_eq!(result.total_messages, 4);
    assert_eq!(result.successful_messages.len() + result.failed_messages.len(), 4);

    // Success rate should be around 50% (allowing some variance)
    let success_rate = result.success_rate();
    assert!(success_rate > 0.25 && success_rate < 0.75); // Allow 25% variance
}

#[tokio::test]
async fn test_batching_processor_add_and_flush() {
    let inner = Arc::new(SimpleMessageProcessor::success_processor());
    let batch_config = BatchConfig {
        max_batch_size: 3,
        min_batch_size: 2,
        max_wait_time_ms: 5000,
        enable_auto_batch: false,
        batch_timeout_policy: BatchTimeoutPolicy::WaitFull,
    };

    let batch_processor = BatchingProcessor::new(inner.clone(), batch_config);

    // Add messages to batch
    for i in 1..=2 {
        let message = QueueMessage::new(format!("msg-{}", i), serde_json::json!({"id": i}));
        batch_processor.add_to_batch(message).await.unwrap();
    }

    // Should have 2 messages pending
    assert_eq!(batch_processor.current_batch_size().await, 2);

    // Add one more to reach max batch size
    let message = QueueMessage::new("msg-3".to_string(), serde_json::json!({"id": 3}));
    batch_processor.add_to_batch(message).await.unwrap();

    // Should have processed the batch automatically
    sleep(Duration::from_millis(100)).await; // Allow time for processing
    assert_eq!(batch_processor.current_batch_size().await, 0);

    // Check inner processor stats
    let stats = inner.get_stats().await;
    assert_eq!(stats.total_messages_processed, 3);
    assert_eq!(stats.total_messages_succeeded, 3);
}

#[tokio::test]
async fn test_batching_processor_manual_flush() {
    let inner = Arc::new(SimpleMessageProcessor::success_processor());
    let batch_config = BatchConfig {
        max_batch_size: 10,
        min_batch_size: 2,
        max_wait_time_ms: 5000,
        enable_auto_batch: false,
        batch_timeout_policy: BatchTimeoutPolicy::WaitFull,
    };

    let batch_processor = BatchingProcessor::new(inner.clone(), batch_config);

    // Add 2 messages (min batch size)
    for i in 1..=2 {
        let message = QueueMessage::new(format!("msg-{}", i), serde_json::json!({"id": i}));
        batch_processor.add_to_batch(message).await.unwrap();
    }

    // Should still have 2 messages pending (not reached max batch size)
    assert_eq!(batch_processor.current_batch_size().await, 2);

    // Manually flush the batch
    let result = batch_processor.flush_batch().await.unwrap();
    assert_eq!(result.total_messages, 2);
    assert_eq!(result.success_rate(), 1.0);

    // Should have no more pending messages
    assert_eq!(batch_processor.current_batch_size().await, 0);
}

#[tokio::test]
async fn test_retry_handler_exponential_backoff() {
    let retry_config = RetryConfig {
        max_attempts: 5,
        initial_delay_ms: 1000,
        backoff_multiplier: 2.0,
        max_delay_ms: 10000,
        jitter_percentage: 0.0, // No jitter for deterministic test
        retry_policy: RetryPolicy::Exponential,
    };

    let retry_handler = RetryHandler::new(retry_config);

    // Test exponential backoff: 1000, 2000, 4000, 8000 (but capped at 10000)
    assert_eq!(retry_handler.calculate_delay(0).as_millis(), 1000);
    assert_eq!(retry_handler.calculate_delay(1).as_millis(), 2000);
    assert_eq!(retry_handler.calculate_delay(2).as_millis(), 4000);
    assert_eq!(retry_handler.calculate_delay(3).as_millis(), 8000);
    assert_eq!(retry_handler.calculate_delay(4).as_millis(), 10000); // Capped at max

    // Test retry decision
    assert!(retry_handler.should_retry(0, &ProcessingOutcome::Retry { delay_seconds: 5 }));
    assert!(retry_handler.should_retry(4, &ProcessingOutcome::Retry { delay_seconds: 5 }));
    assert!(!retry_handler.should_retry(5, &ProcessingOutcome::Retry { delay_seconds: 5 }));
    assert!(!retry_handler.should_retry(0, &ProcessingOutcome::Failed));
    assert!(!retry_handler.should_retry(0, &ProcessingOutcome::Success));
}

#[tokio::test]
async fn test_retry_handler_linear_backoff() {
    let retry_config = RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 1000,
        backoff_multiplier: 1.0,
        max_delay_ms: 5000,
        jitter_percentage: 0.0,
        retry_policy: RetryPolicy::Linear,
    };

    let retry_handler = RetryHandler::new(retry_config);

    // Test linear backoff: 1000, 2000, 3000
    assert_eq!(retry_handler.calculate_delay(0).as_millis(), 1000);
    assert_eq!(retry_handler.calculate_delay(1).as_millis(), 2000);
    assert_eq!(retry_handler.calculate_delay(2).as_millis(), 3000);
}

#[tokio::test]
async fn test_retry_handler_fixed_backoff() {
    let retry_config = RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 2000,
        backoff_multiplier: 2.0,
        max_delay_ms: 5000,
        jitter_percentage: 0.0,
        retry_policy: RetryPolicy::Fixed,
    };

    let retry_handler = RetryHandler::new(retry_config);

    // Test fixed backoff: 2000, 2000, 2000
    assert_eq!(retry_handler.calculate_delay(0).as_millis(), 2000);
    assert_eq!(retry_handler.calculate_delay(1).as_millis(), 2000);
    assert_eq!(retry_handler.calculate_delay(2).as_millis(), 2000);
}

#[tokio::test]
async fn test_retry_handler_jitter() {
    let retry_config = RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 1000,
        backoff_multiplier: 2.0,
        max_delay_ms: 10000,
        jitter_percentage: 0.2, // 20% jitter
        retry_policy: RetryPolicy::Exponential,
    };

    let retry_handler = RetryHandler::new(retry_config);

    // Test with jitter - delay should vary but be within expected range
    for _ in 0..10 {
        let delay = retry_handler.calculate_delay(1).as_millis();
        // Expected base delay is 2000ms, with 20% jitter it should be between 1600 and 2400
        assert!(delay >= 1600 && delay <= 2400);
    }
}

#[tokio::test]
async fn test_processor_stats_calculation() {
    let processor = SimpleMessageProcessor::random_processor(0.3); // 30% failure rate

    let messages = vec![
        QueueMessage::new("msg-1".to_string(), serde_json::json!({"id": 1})),
        QueueMessage::new("msg-2".to_string(), serde_json::json!({"id": 2})),
        QueueMessage::new("msg-3".to_string(), serde_json::json!({"id": 3})),
        QueueMessage::new("msg-4".to_string(), serde_json::json!({"id": 4})),
        QueueMessage::new("msg-5".to_string(), serde_json::json!({"id": 5})),
    ];

    for message in messages {
        let context = ProcessingContext {
            processing_id: Uuid::new_v4().to_string(),
            processor_id: "stats-test".to_string(),
            start_time: std::time::Instant::now(),
            attempt_number: 1,
            max_attempts: 3,
            batch_context: None,
            metadata: std::collections::HashMap::new(),
        };

        processor.process_message(message, context).await;
    }

    let stats = processor.get_stats().await;

    assert_eq!(stats.total_messages_processed, 5);
    assert_eq!(stats.total_messages_succeeded + stats.total_messages_failed, 5);
    assert!(stats.success_rate() > 0.4 && stats.success_rate() < 0.8); // Allow variance
    assert_eq!(stats.success_rate() + stats.failure_rate(), 1.0);
    assert!(stats.last_processed_at.is_some());
    assert!(stats.uptime_seconds > 0);
    assert!(stats.avg_processing_time_ms > 0.0);
}

#[tokio::test]
async fn test_processor_stats_reset() {
    let processor = SimpleMessageProcessor::success_processor();

    // Process some messages
    for i in 1..=3 {
        let message = QueueMessage::new(format!("msg-{}", i), serde_json::json!({"id": i}));
        let context = ProcessingContext {
            processing_id: Uuid::new_v4().to_string(),
            processor_id: "reset-test".to_string(),
            start_time: std::time::Instant::now(),
            attempt_number: 1,
            max_attempts: 3,
            batch_context: None,
            metadata: std::collections::HashMap::new(),
        };

        processor.process_message(message, context).await;
    }

    // Check stats before reset
    let stats_before = processor.get_stats().await;
    assert_eq!(stats_before.total_messages_processed, 3);

    // Reset stats
    processor.reset_stats().await;

    // Check stats after reset
    let stats_after = processor.get_stats().await;
    assert_eq!(stats_after.total_messages_processed, 0);
    assert_eq!(stats_after.total_messages_succeeded, 0);
    assert_eq!(stats_after.total_messages_failed, 0);
    assert_eq!(stats_after.total_messages_retried, 0);
}

#[tokio::test]
async fn test_can_process_method() {
    let processor = SimpleMessageProcessor::success_processor();

    let message = QueueMessage::new("test".to_string(), serde_json::json!({"test": "data"}));

    // Default implementation should return true for any message
    assert!(processor.can_process(&message).await);
}

#[tokio::test]
async fn test_processor_metadata() {
    let processor = SimpleMessageProcessor::custom("CustomProcessor");

    assert_eq!(processor.processor_name(), "CustomProcessor");
    assert_eq!(processor.processor_version(), "1.0.0");
}

#[tokio::test]
async fn test_batch_result_calculations() {
    let successful_messages = vec![
        create_test_processed_message("msg-1", ProcessingOutcome::Success),
        create_test_processed_message("msg-2", ProcessingOutcome::Success),
    ];

    let failed_messages = vec![
        create_test_processed_message("msg-3", ProcessingOutcome::Failed),
    ];

    let result = BatchProcessingResult {
        batch_id: "test-batch".to_string(),
        total_messages: 3,
        successful_messages,
        failed_messages,
        retried_messages: vec![],
        total_processing_time: Duration::from_millis(1500),
        batch_start_time: chrono::Utc::now(),
        batch_end_time: chrono::Utc::now(),
    };

    assert_eq!(result.success_rate(), 2.0 / 3.0);
    assert_eq!(result.failure_rate(), 1.0 / 3.0);
    assert_eq!(result.avg_processing_time_per_message(), Duration::from_millis(500));
}

fn create_test_processed_message(id: &str, outcome: ProcessingOutcome) -> ProcessedMessage {
    ProcessedMessage {
        message: QueueMessage::new(id.to_string(), serde_json::json!({"id": id})),
        outcome,
        processing_time: Duration::from_millis(100),
        context: ProcessingContext {
            processing_id: Uuid::new_v4().to_string(),
            processor_id: "test".to_string(),
            start_time: std::time::Instant::now(),
            attempt_number: 1,
            max_attempts: 3,
            batch_context: None,
            metadata: std::collections::HashMap::new(),
        },
        error_message: None,
        result_data: None,
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_end_to_end_batching_with_retry() {
        let inner = Arc::new(SimpleMessageProcessor::random_processor(0.2)); // 20% failure rate
        let batch_config = BatchConfig::default();
        let retry_config = RetryConfig::default();

        let batch_processor = BatchingProcessor::new(inner.clone(), batch_config);
        let retry_handler = RetryHandler::new(retry_config);

        // Add several messages to the batch
        for i in 1..=10 {
            let message = QueueMessage::new(format!("msg-{}", i), serde_json::json!({"id": i}));
            batch_processor.add_to_batch(message).await.unwrap();
        }

        // Flush and process
        let batch_result = batch_processor.flush_batch().await.unwrap();

        // Simulate retry logic for failed messages
        let mut retried_count = 0;
        for failed_msg in &batch_result.failed_messages {
            if retry_handler.should_retry(failed_msg.context.attempt_number, &failed_msg.outcome) {
                retried_count += 1;
            }
        }

        // Verify batch was processed
        assert_eq!(batch_result.total_messages, 10);
        assert!(batch_result.successful_messages.len() > 5); // Should be mostly successful

        // Check final stats
        let final_stats = inner.get_stats().await;
        assert_eq!(final_stats.total_messages_processed, 10);
        assert!(final_stats.success_rate() > 0.5);
    }
}
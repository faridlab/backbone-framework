//! Message Deduplication and Exactly-Once Processing Demo
//!
//! Demonstrates comprehensive deduplication capabilities and exactly-once processing.

use backbone_queue::{
    MessageDeduplicator, DeduplicationConfig, DeduplicationStrategy, DeduplicationCache,
    QueueMessage, ProcessingResult,
};
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🔀 Message Deduplication and Exactly-Once Processing Demo");
    println!("=======================================================");

    // Test different deduplication strategies
    demo_message_id_deduplication().await?;
    demo_content_deduplication().await?;
    demo_exactly_once_processing().await?;
    demo_deduplication_statistics().await?;
    demo_cleanup_operations().await?;

    println!("\n🎉 Deduplication demo completed!");

    Ok(())
}

async fn demo_message_id_deduplication() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n✅ Message ID Deduplication");
    println!("===========================");

    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 300, // 5 minutes
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config)?;

    // Create test messages
    let mut message1 = QueueMessage::new("msg-1".to_string(), serde_json::json!({
        "order_id": "ORD-001",
        "amount": 100.00,
        "customer": "John Doe"
    }));
    message1.message_deduplication_id = Some("order-ORD-001".to_string());

    let mut message2 = QueueMessage::new("msg-2".to_string(), serde_json::json!({
        "order_id": "ORD-001",
        "amount": 100.00,
        "customer": "John Doe"
    }));
    message2.message_deduplication_id = Some("order-ORD-001".to_string()); // Same deduplication ID

    let mut message3 = QueueMessage::new("msg-3".to_string(), serde_json::json!({
        "order_id": "ORD-002",
        "amount": 200.00,
        "customer": "Jane Smith"
    }));
    message3.message_deduplication_id = Some("order-ORD-002".to_string());

    // Check deduplication
    println!("Message 1 is duplicate: {}", deduplicator.is_duplicate(&message1).await?);
    deduplicator.record_message(&message1).await?;
    println!("Recorded message 1");

    println!("Message 2 is duplicate: {}", deduplicator.is_duplicate(&message2).await?);
    println!("Message 3 is duplicate: {}", deduplicator.is_duplicate(&message3).await?);

    let stats = deduplicator.get_stats().await;
    println!("Deduplication stats: {} total messages, {} duplicates detected, {:.2}% duplicate rate",
        stats.total_messages,
        stats.duplicates_detected,
        stats.duplicate_rate() * 100.0
    );

    Ok(())
}

async fn demo_content_deduplication() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📝 Content Deduplication");
    println!("========================");

    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::Content,
        deduplication_window_seconds: 300,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: false,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config)?;

    // Create messages with identical content but different deduplication IDs
    let mut message1 = QueueMessage::new("msg-1".to_string(), serde_json::json!({
        "payment_id": "PAY-001",
        "amount": 99.99,
        "currency": "USD"
    }));
    message1.message_deduplication_id = Some("pay-1".to_string());

    let mut message2 = QueueMessage::new("msg-2".to_string(), serde_json::json!({
        "payment_id": "PAY-001",
        "amount": 99.99,
        "currency": "USD"
    }));
    message2.message_deduplication_id = Some("pay-2".to_string()); // Different ID but same content

    // Check deduplication
    println!("Message 1 is duplicate: {}", deduplicator.is_duplicate(&message1).await?);
    deduplicator.record_message(&message1).await?;
    println!("Recorded message 1");

    println!("Message 2 is duplicate (same content): {}", deduplicator.is_duplicate(&message2).await?);

    // Different content should not be duplicate
    let mut message3 = QueueMessage::new("msg-3".to_string(), serde_json::json!({
        "payment_id": "PAY-002",
        "amount": 149.99,
        "currency": "USD"
    }));
    message3.message_deduplication_id = Some("pay-3".to_string());

    println!("Message 3 is duplicate (different content): {}", deduplicator.is_duplicate(&message3).await?);

    Ok(())
}

async fn demo_exactly_once_processing() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 Exactly-Once Processing");
    println!("===========================");

    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 300,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 600, // 10 minutes
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config)?;

    let mut message = QueueMessage::new("msg-1".to_string(), serde_json::json!({
        "task_id": "TASK-001",
        "operation": "process_payment",
        "data": {"amount": 100.00}
    }));
    message.message_deduplication_id = Some("task-TASK-001".to_string());

    let processor_id = "payment-processor-1";

    // Start processing
    println!("Starting exactly-once processing...");
    let processing_id = deduplicator.start_processing(&message, processor_id).await?;
    println!("Started processing with ID: {}", processing_id);

    // Try to start processing again (should fail)
    println!("Attempting duplicate processing...");
    let duplicate_result = deduplicator.start_processing(&message, "processor-2").await;
    println!("Duplicate processing result: {}", duplicate_result.is_err());

    // Simulate processing
    println!("Simulating message processing...");
    sleep(Duration::from_millis(500)).await;

    // Complete processing successfully
    println!("Completing processing...");
    deduplicator.complete_processing(&processing_id, ProcessingResult::Success, None).await?;

    // Check statistics
    let stats = deduplicator.get_stats().await;
    println!("Exactly-once stats:");
    println!("  Violations prevented: {}", stats.exactly_once_violations_prevented);
    println!("  Completed processing: {}", stats.completed_processing_records);
    println!("  Active processing records: {}", stats.active_processing_records);

    // Test failed processing
    let mut failed_message = QueueMessage::new("msg-2".to_string(), serde_json::json!({
        "task_id": "TASK-002",
        "operation": "invalid_operation"
    }));
    failed_message.message_deduplication_id = Some("task-TASK-002".to_string());

    let failed_processing_id = deduplicator.start_processing(&failed_message, processor_id).await?;
    deduplicator.complete_processing(
        &failed_processing_id,
        ProcessingResult::Failed,
        Some("Invalid operation".to_string()),
    ).await?;

    let final_stats = deduplicator.get_stats().await;
    println!("Final exactly-once stats:");
    println!("  Failed processing records: {}", final_stats.failed_processing_records);

    Ok(())
}

async fn demo_deduplication_statistics() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Deduplication Statistics");
    println!("=============================");

    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::Both,
        deduplication_window_seconds: 300,
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 600,
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config)?;

    // Process multiple messages to generate statistics
    for i in 0..20 {
        let mut message = QueueMessage::new(format!("msg-{}", i), serde_json::json!({
            "batch_id": i,
            "payload": format!("Test message {}", i)
        }));

        // Create some duplicates
        let dedup_id = if i % 5 == 0 { "duplicate-id" } else { &format!("unique-id-{}", i) };
        message.message_deduplication_id = Some(dedup_id.to_string());

        let is_duplicate = deduplicator.is_duplicate(&message).await?;
        if !is_duplicate {
            deduplicator.record_message(&message).await?;

            // Start exactly-once processing for some messages
            if i % 3 == 0 {
                let processing_id = deduplicator.start_processing(&message, &format!("processor-{}", i % 2)).await?;
                let result = if i % 4 == 0 { ProcessingResult::Success } else { ProcessingResult::Failed };
                deduplicator.complete_processing(&processing_id, result, None).await?;
            }
        }
    }

    let stats = deduplicator.get_stats().await;
    println!("📈 Comprehensive Statistics:");
    println!("  Total messages checked: {}", stats.total_messages);
    println!("  Duplicates detected: {}", stats.duplicates_detected);
    println!("  Duplicate rate: {:.2}%", stats.duplicate_rate() * 100.0);
    println!("  Cache hit rate: {:.2}%", stats.cache_hit_rate() * 100.0);
    println!("  Cache size: {} entries", stats.cache_size);
    println!("  Exactly-once violations prevented: {}", stats.exactly_once_violations_prevented);
    println!("  Completed processing records: {}", stats.completed_processing_records);
    println!("  Failed processing records: {}", stats.failed_processing_records);
    println!("  Active processing records: {}", stats.active_processing_records);
    if let Some(last_cleanup) = stats.last_cleanup_at {
        println!("  Last cleanup: {}", last_cleanup.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    Ok(())
}

async fn demo_cleanup_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧹 Cleanup Operations");
    println!("======================");

    let config = DeduplicationConfig {
        strategy: DeduplicationStrategy::MessageId,
        deduplication_window_seconds: 2, // Very short window for demo
        cache_backend: DeduplicationCache::Memory,
        max_cache_entries: 1000,
        enable_exactly_once: true,
        exactly_once_window_seconds: 5, // Short window for demo
        cleanup_interval_seconds: 1,   // Frequent cleanup
        ..Default::default()
    };

    let deduplicator = MessageDeduplicator::new(config)?;

    // Add some entries
    for i in 0..10 {
        let mut message = QueueMessage::new(format!("msg-{}", i), serde_json::json!({
            "index": i,
            "will_expire": true
        }));
        message.message_deduplication_id = Some(format!("expire-{}", i));

        deduplicator.record_message(&message).await?;

        if i % 2 == 0 {
            let processing_id = deduplicator.start_processing(&message, "cleanup-processor").await?;
            deduplicator.complete_processing(&processing_id, ProcessingResult::Success, None).await?;
        }
    }

    let initial_stats = deduplicator.get_stats().await;
    println!("Initial cache size: {}", initial_stats.cache_size);

    // Wait for entries to expire
    println!("Waiting for entries to expire...");
    sleep(Duration::from_secs(3)).await;

    // Run cleanup
    println!("Running cleanup...");
    let cleaned_count = deduplicator.cleanup_expired().await?;
    println!("Cleaned {} expired entries", cleaned_count);

    let final_stats = deduplicator.get_stats().await;
    println!("Final cache size: {}", final_stats.cache_size);
    if let Some(last_cleanup) = final_stats.last_cleanup_at {
        println!("Last cleanup completed: {}", last_cleanup.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    Ok(())
}

fn show_deduplication_strategies() {
    println!("\n🔀 Deduplication Strategies");
    println!("============================");

    let strategies = vec![
        ("None", "No deduplication - all messages are processed"),
        ("MessageId", "Deduplicate based on message_deduplication_id field"),
        ("Content", "Deduplicate based on content hash (same payload = duplicate)"),
        ("Both", "Both message ID and content must be unique"),
    ];

    for (name, description) in strategies {
        println!("  {}: {}", name, description);
    }
}

fn show_configuration_options() {
    println!("\n⚙️  Configuration Options");
    println!("=========================");

    println!("  • deduplication_window_seconds: How long to remember deduplication keys");
    println!("  • cache_backend: Storage backend (Memory, Redis, Custom)");
    println!("  • max_cache_entries: Maximum number of cached entries");
    println!("  • enable_exactly_once: Enable exactly-once processing guarantees");
    println!("  • exactly_once_window_seconds: How long to track processing records");
    println!("  • cleanup_interval_seconds: How often to run cleanup tasks");
}

fn show_use_cases() {
    println!("\n📚 Common Use Cases");
    println!("====================");

    let use_cases = vec![
        ("Payment Processing", "Prevent duplicate payment processing with message ID deduplication"),
        ("Event Sourcing", "Ensure exactly-once event processing with exactly-once guarantees"),
        ("API Gateway", "Prevent duplicate API requests with content deduplication"),
        ("Data Pipeline", "Ensure data integrity with comprehensive deduplication"),
        ("Notification Systems", "Prevent duplicate notifications with message ID deduplication"),
    ];

    for (use_case, description) in use_cases {
        println!("  • {}: {}", use_case, description);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_deduplication_demo_completes() {
        // This test ensures the demo runs without panicking
        demo_message_id_deduplication().await.unwrap();
        demo_content_deduplication().await.unwrap();
        demo_exactly_once_processing().await.unwrap();
        demo_deduplication_statistics().await.unwrap();
        demo_cleanup_operations().await.unwrap();
    }

    #[tokio::test]
    async fn test_configuration_factory() {
        // Show common configuration patterns
        let low_volume_config = DeduplicationConfig {
            strategy: DeduplicationStrategy::MessageId,
            deduplication_window_seconds: 60,  // 1 minute
            max_cache_entries: 1000,
            enable_exactly_once: false,
            ..Default::default()
        };

        let high_volume_config = DeduplicationConfig {
            strategy: DeduplicationStrategy::Both,
            deduplication_window_seconds: 3600,  // 1 hour
            max_cache_entries: 100000,
            enable_exactly_once: true,
            exactly_once_window_seconds: 7200,  // 2 hours
            ..Default::default()
        };

        assert!(MessageDeduplicator::new(low_volume_config).is_ok());
        assert!(MessageDeduplicator::new(high_volume_config).is_ok());
    }
}
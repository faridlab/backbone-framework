//! FIFO Queue Demo
//!
//! This example demonstrates how to use FIFO (First-In-First-Out) queues
//! with message ordering, deduplication, and group-based processing.

use backbone_queue::{
    QueueService,
    redis::RedisQueueBuilder,
    fifo::{FifoQueueService, FifoQueueServiceWrapper, FifoQueueConfig, MessageVolume},
    types::{QueueMessage, QueuePriority}
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde_json::json;

/// Create FIFO message with proper attributes
fn create_fifo_message(
    id: &str,
    group_id: &str,
    deduplication_id: &str,
    payload: serde_json::Value,
    priority: QueuePriority,
) -> QueueMessage {
    QueueMessage {
        id: id.to_string(),
        payload,
        priority,
        receive_count: 0,
        max_receive_count: 3,
        enqueued_at: chrono::Utc::now(),
        visible_at: chrono::Utc::now(),
        expires_at: None,
        visibility_timeout: 30,
        status: backbone_queue::MessageStatus::Pending,
        delay_seconds: None,
        attributes: std::collections::HashMap::new(),
        message_group_id: Some(group_id.to_string()),
        message_deduplication_id: Some(deduplication_id.to_string()),
        compressed: false,
        original_size: None,
    }
}

/// Create order processing message
fn create_order_message(order_id: &str, action: &str) -> QueueMessage {
    create_fifo_message(
        &format!("order-{}", order_id),
        format!("order-{}", order_id), // One group per order
        format!("order-{}-{}", order_id, action), // Unique deduplication ID per action
        json!({
            "order_id": order_id,
            "action": action,
            "timestamp": chrono::Utc::now(),
            "data": {
                "customer_id": format!("customer-{}", order_id),
                "amount": (order_id.parse::<i64>().unwrap_or(0) * 100),
                "items": vec![
                    {"product": "widget-{}".format!(order_id), "quantity": order_id.parse::<i64>().unwrap_or(1) % 10 + 1)},
                    {"product": "gadget-{}",format!(order_id), "quantity": order_id.parse::<i64>().unwrap_or(1) % 5 + 1)}
                ]
            }
        }),
        QueuePriority::Normal,
    )
}

/// Create user activity message
fn create_activity_message(user_id: &str, activity: &str) -> QueueMessage {
    create_fifo_message(
        &format!("activity-{}", user_id),
        format!("user-{}", user_id), // One group per user
        format!("activity-{}-{}", user_id, chrono::Utc::now().timestamp()),
        json!({
            "user_id": user_id,
            "activity": activity,
            "timestamp": chrono::Utc::now(),
            "metadata": {
                "source": "mobile_app",
                "version": "1.0.0"
            }
        }),
        QueuePriority::Low,
    )
}

/// Demonstrate basic FIFO queue operations
async fn demo_basic_fifo_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 Basic FIFO Queue Operations");
    println!("=============================");

    // Create Redis queue
    let queue = RedisQueueBuilder::new()
        .url("redis://localhost:6379")
        .queue_name("fifo_test_queue")
        .key_prefix("fifo")
        .build()
        .await?;

    // Test connection
    if !queue.test_connection().await? {
        println!("❌ Failed to connect to Redis, skipping demo");
        return Ok(());
    }

    // Clear queue
    queue.purge().await?;

    // Create FIFO service with recommended configuration
    let config = backbone_queue::fifo::utils::get_recommended_config(MessageVolume::Medium);
    let fifo_service = FifoQueueServiceWrapper::new(
        Arc::new(queue),
        config,
    );

    // Enqueue FIFO messages
    println!("\n📤 Enqueuing FIFO messages...");

    let messages = vec![
        create_order_message("1001", "created"),
        create_order_message("1002", "created"),
        create_order_message("1003", "created"),
        create_order_message("1001", "updated"),
        create_order_message("1002", "updated"),
    ];

    for (i, message) in messages.iter().enumerate() {
        match fifo_service.enqueue_fifo(message.clone()).await {
            Ok(id) => {
                println!("  ✅ Message {} enqueued: {} (Group: {})",
                    i + 1,
                    message.message_deduplication_id.as_ref().unwrap(),
                    message.message_group_id.as_ref().unwrap()
                );
            }
            Err(e) => {
                println!("  ❌ Failed to enqueue message {}: {}", i + 1, e);
            }
        }

    // Show FIFO statistics
    let stats = fifo_service.get_fifo_stats().await?;
    println!("\n📊 FIFO Queue Statistics:");
    println!("  Total groups: {}", stats.total_groups);
    println!("  Active groups: {}", stats.active_groups);
    println!("  Deduplicated messages: {}", stats.deduplicated_messages);
    println!("  Deduplication cache size: {}", stats.deduplication_cache.len());

    Ok(())
}

/// Demonstrate message deduplication
async fn demo_message_deduplication() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🚫 Message Deduplication Demo");
    println!("=============================");

    // Create Redis queue
    let queue = RedisQueueBuilder::new()
        .url("redis://localhost:6379")
        .queue_name("dedup_test_queue")
        .key_prefix("dedup")
        .build()
        .await?;

    // Test connection
    if !queue.test_connection().await? {
        println!("❌ Failed to connect to Redis, skipping demo");
        return Ok(());
    }

    // Clear queue
    queue.purge().await?;

    // Create FIFO service with content deduplication enabled
    let config = FifoQueueConfig {
        enabled: true,
        deduplication_window_seconds: 300, // 5 minutes
        max_message_groups: 1000,
        enable_content_deduplication: true, // Enable content deduplication
        content_deduplication_window_seconds: 60, // 1 minute
        max_deduplicated_messages: 10000,
    };

    let fifo_service = FifoQueueServiceWrapper::new(
        Arc::new(queue),
        config,
    );

    println!("\n📝 Testing deduplication IDs...");

    // Same deduplication ID should be rejected
    let message1 = create_fifo_message(
        "test-1",
        "group-1",
        "same-dedup-id",
        json!({"action": "update", "data": "value1"}),
        QueuePriority::Normal,
    );

    let message2 = create_fifo_message(
        "test-2",
        "group-1",
        "same-dedup-id", // Same deduplication ID
        json!({"action": "update", "data": "value2"}),
        QueuePriority::Normal,
    );

    match fifo_service.enqueue_fifo(message1).await {
        Ok(id) => println!("  ✅ First message enqueued: {}", id),
        Err(e) => println!("  ❌ Failed to enqueue first message: {}", e),
    }

    match fifo_service.enqueue_fifo(message2).await {
        Ok(_) => println!("  ⚠️  Second message unexpectedly enqueued"),
        Err(e) => println!("  ✅ Second message correctly rejected: {}", e),
    }

    println!("\n🔄 Testing content deduplication...");

    // Same content should be rejected
    let content = json!({
        "order_id": "1234",
        "amount": 99.99,
        "items": ["item1", "item2"]
    });

    let message3 = create_fifo_message(
        "test-3",
        "group-2",
        "content-dedup-1",
        content.clone(),
        QueuePriority::Normal,
    );

    let message4 = create_fifo_message(
        "test-4",
        "group-2",
        "content-dedup-2", // Different deduplication ID
        content, // Same content
        QueuePriority::Normal,
    );

    match fifo_service.enqueue_fifo(message3).await {
        Ok(id) => println!("  ✅ First content message enqueued: {}", id),
        Err(e) => println!("  ❌ Failed to enqueue first content message: {}", e),
    }

    match fifo_service.enqueue_fifo(message4).await {
        Ok(_) => println!("  ⚠️  Second content message unexpectedly enqueued"),
        Err(e) => println!("  ✅ Second content message correctly rejected: {}", e),
    }

    // Show deduplication statistics
    let stats = fifo_service.get_fifo_stats().await?;
    println!("\n📈 Deduplication Statistics:");
    println!("  Message deduplication cache: {} entries", stats.deduplication_cache.len());
    println!("  Content deduplication cache: {} entries", stats.content_deduplication_cache.len());

    Ok(())
}

/// Demonstrate message group statistics
async fn demo_group_statistics() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Message Group Statistics Demo");
    println!("===============================");

    // Create Redis queue
    let queue = RedisQueueBuilder::new()
        .url("redis://localhost:6379")
        .queue_name("stats_test_queue")
        .key_prefix("stats")
        .build()
        .await?;

    // Test connection
    if !queue.test_connection().await? {
        println!("❌ Failed to connect to Redis, skipping demo");
        return Ok(());
    }

    // Clear queue
    queue.purge().await?;

    let fifo_service = FifoQueueServiceWrapper::new(
        Arc::new(queue),
        FifoQueueConfig::default(),
    );

    // Create messages for different groups
    println!("\n📤 Creating messages for different groups...");

    let groups = vec![
        ("payments", 3),
        ("notifications", 5),
        ("analytics", 2),
        ("backup", 1),
    ];

    for (group_name, count) in groups {
        println!("  📦 Group '{}': {} messages", group_name, count);

        for i in 1..=count {
            let message = create_fifo_message(
                &format!("{}-{}", group_name, i),
                group_name,
                &format!("{}-{}-{}", group_name, i, chrono::Utc::now().timestamp()),
                json!({
                    "group": group_name,
                    "index": i,
                    "data": "test data for group processing"
                }),
                QueuePriority::Normal,
            );

            match fifo_service.enqueue_fifo(message).await {
                Ok(_) => {}, // Success
                Err(e) => println!("    ❌ Failed: {}", e),
            }
        }
    }

    // Simulate processing to update group statistics
    println!("\n⚙️  Simulating message processing...");
    let group_ids: Vec<String> = groups.iter().map(|(name, _)| name.to_string()).collect();

    for group_id in group_ids {
        let mut group_stats = fifo_service.get_group_stats(&group_id).await?.unwrap_or_default();

        // Simulate some processing
        for _ in 0..group_stats.message_count / 2 {
            // Simulate successful processing
            {
                let mut stats = fifo_service.stats.write().await;
                stats.update_group(&group_id, true, 100 + (rand::random::<u64>() % 200));
            }
        }

        // Simulate some failures
        for _ in 0..group_stats.message_count / 4 {
            // Simulate failed processing
            {
                let mut stats = fifo_service.stats.write().await;
                stats.update_group(&group_id, false, 300 + (rand::random::<u64>() % 200));
            }
        }
    }

    // Display group statistics
    println!("\n📈 Message Group Statistics:");
    let all_stats = fifo_service.get_all_group_stats().await?;

    for (group_id, stats) in all_stats {
        println!("  📂 {}: {} messages, {:.1}% success rate, {:.1}ms avg processing time",
            group_id,
            stats.message_count,
            stats.success_rate(),
            stats.avg_processing_time_ms
        );
    }

    Ok(())
}

/// Demonstrate cleanup operations
async fn demo_cleanup_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧹 Cleanup Operations Demo");
    println!("==========================");

    // Create Redis queue
    let queue = RedisQueueBuilder::new()
        .url("redis://localhost:6379")
        .queue_name("cleanup_test_queue")
        .key_prefix("cleanup")
        .build()
        .await?;

    // Test connection
    if !queue.test_connection().await? {
        println!("❌ Failed to connect to Redis, skipping demo");
        return Ok(());
    }

    // Clear queue
    queue.purge().await?;

    let fifo_service = FifoQueueServiceWrapper::new(
        Arc::new(queue),
        FifoQueueConfig {
            enabled: true,
            deduplication_window_seconds: 2, // Very short window for demo
            max_message_groups: 100,
            enable_content_deduplication: true,
            content_deduplication_window_seconds: 2,
            max_deduplicated_messages: 100,
        },
    );

    println!("\n📝 Adding entries that will expire...");

    // Add expired entries
    {
        let mut stats = fifo_service.stats.write().await;
        let past_time = chrono::Utc::now() - chrono::Duration::seconds(10);

        // Add expired deduplication entries
        for i in 0..5 {
            stats.deduplication_cache.insert(
                format!("expired-dedup-{}", i),
                past_time,
            );
        }

        // Add expired content deduplication entries
        for i in 0..3 {
            stats.content_deduplication_cache.insert(
                format!("expired-content-{}", i),
                past_time,
            );
        }

        println!("  🗑️  Added 5 expired message deduplication entries");
        println!("  🗑️  Added 3 expired content deduplication entries");
    }

    let stats_before = fifo_service.get_fifo_stats().await?;
    println!("\n📊 Before cleanup:");
    println!("  Deduplication cache: {} entries", stats_before.deduplication_cache.len());
    println!("  Content deduplication cache: {} entries", stats_before.content_deduplication_cache.len());

    // Perform cleanup
    println!("\n🧹 Performing cleanup...");
    let removed_count = fifo_service.cleanup_deduplication().await?;

    let stats_after = fifo_service.get_fifo_stats().await?;
    println!("\n📊 After cleanup:");
    println!("  Removed {} expired entries", removed_count);
    println!("  Deduplication cache: {} entries", stats_after.deduplication_cache.len());
    println!("  Content deduplication cache: {} entries", stats_after.content_deduplication_cache.len());

    Ok(())
}

/// Demonstrate configuration recommendations
async fn demo_configuration_recommendations() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚙️  Configuration Recommendations");
    println!("===============================");

    let volumes = vec![
        (MessageVolume::Low, "Small application (100-1K messages/day)"),
        (MessageVolume::Medium, "Medium application (1K-10K messages/day)"),
        (MessageVolume::High, "Large application (10K+ messages/day)"),
    ];

    for (volume, description) in volumes {
        let config = backbone_queue::fifo::utils::get_recommended_config(volume);
        let errors = backbone_queue::fifo::utils::validate_config(&config);

        println!("\n📈 {} Volume Recommendation:", volume as i32);
        println!("  📝 Description: {}", description);
        println!("  ⏰ Deduplication window: {} seconds", config.deduplication_window_seconds);
        println!("  📊 Max message groups: {}", config.max_message_groups);
        println!("  🔍 Content deduplication: {}", config.enable_content_deduplication);

        if config.enable_content_deduplication {
            println!("  ⏰ Content deduplication window: {} seconds", config.content_deduplication_window_seconds);
        }

        if !errors.is_empty() {
            println!("  ⚠️  Validation issues:");
            for error in errors {
                println!("    ❌ {}", error);
            }
        } else {
            println!("  ✅ Configuration is valid");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 FIFO Queue Demo");
    println!("===================");

    // Run all demos
    demo_basic_fifo_operations().await?;
    demo_message_deduplication().await?;
    demo_group_statistics().await?;
    demo_cleanup_operations().await?;
    demo_configuration_recommendations().await?;

    println!("\n🎉 FIFO queue demo completed!");

    println!("\n📚 Key Takeaways:");
    println!("  • FIFO ensures exact message ordering within groups");
    println!("  • Message deduplication prevents duplicate processing");
    println!("  • Content deduplication provides additional safety");
    println!("  • Message groups allow parallel processing of independent streams");
    println!("  • Group statistics help monitor individual processing flows");
    println!("  • Automatic cleanup prevents memory leaks");
    println!("  • Configuration recommendations optimize for different volumes");

    Ok(())
}
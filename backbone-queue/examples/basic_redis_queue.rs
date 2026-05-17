//! Basic Redis Queue Example
//!
//! This example demonstrates the fundamental operations of the Redis queue backend:
//! - Creating a queue connection
//! - Enqueuing and dequeuing messages
//! - Message acknowledgment
//! - Basic queue management

use backbone_queue::{
    QueueService,
    redis::RedisQueueBuilder,
    types::{QueueMessage, QueuePriority}
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 Basic Redis Queue Example");
    println!("============================");

    // Create Redis queue connection
    println!("📡 Connecting to Redis...");
    let queue = RedisQueueBuilder::new()
        .url("redis://localhost:6379")
        .queue_name("example_queue")
        .key_prefix("example")
        .pool_size(5)
        .build()
        .await?;

    // Test the connection
    if !queue.test_connection().await? {
        eprintln!("❌ Failed to connect to Redis");
        return Ok(());
    }
    println!("✅ Connected to Redis successfully");

    // Clear any existing messages
    println!("🧹 Clearing existing messages...");
    queue.purge().await?;
    println!("✅ Queue cleared");

    // Example 1: Basic enqueue/dequeue
    println!("\n📬 Example 1: Basic Message Operations");
    println!("-------------------------------------");

    // Create and enqueue a simple message
    let message = QueueMessage::builder()
        .payload("Hello, Redis Queue!").expect("payload serialization")
        .priority(QueuePriority::Normal)
        .build();

    println!("📤 Enqueuing message: {}", message.payload);
    let message_id = queue.enqueue(message.clone()).await?;
    println!("✅ Message enqueued with ID: {}", message_id);

    // Check queue size
    let size = queue.size().await?;
    println!("📊 Queue size: {}", size);

    // Dequeue the message
    println!("📥 Dequeuing message...");
    if let Some(received_message) = queue.dequeue().await? {
        println!("✅ Received message: {}", received_message.payload);
        println!("🔑 Message ID: {}", received_message.id);
        println!("⚖️ Priority: {}", received_message.priority);
        println!("📅 Enqueued at: {}", received_message.enqueued_at);

        // Acknowledge the message
        queue.ack(&received_message.id).await?;
        println!("✅ Message acknowledged");
    } else {
        println!("❌ No message received");
    }

    // Example 2: Priority-based ordering
    println!("\n🏆 Example 2: Priority Message Ordering");
    println!("--------------------------------------");

    // Create messages with different priorities
    let priority_messages = vec![
        QueueMessage::builder()
            .payload("Low priority task").expect("payload serialization")
            .priority(QueuePriority::Low)
            .build(),
        QueueMessage::builder()
            .payload("Critical emergency task").expect("payload serialization")
            .priority(QueuePriority::Critical)
            .build(),
        QueueMessage::builder()
            .payload("Normal background task").expect("payload serialization")
            .priority(QueuePriority::Normal)
            .build(),
        QueueMessage::builder()
            .payload("High priority task").expect("payload serialization")
            .priority(QueuePriority::High)
            .build(),
    ];

    // Enqueue messages in random order
    println!("📤 Enqueuing messages with different priorities...");
    for message in &priority_messages {
        let id = queue.enqueue(message.clone()).await?;
        println!("  - {} ({})", message.payload, message.priority);
    }

    // Dequeue and verify priority ordering
    println!("\n📥 Dequeue order (should be by priority):");
    for _ in 0..priority_messages.len() {
        if let Some(message) = queue.dequeue().await? {
            println!("  1️⃣ {} ({})", message.payload, message.priority);
            queue.ack(&message.id).await?;
        }
    }

    // Example 3: Batch operations
    println!("\n📦 Example 3: Batch Operations");
    println!("----------------------------");

    // Create multiple messages for batch enqueue
    let mut batch_messages = Vec::new();
    for i in 1..=10 {
        batch_messages.push(QueueMessage::builder()
            .payload(format!("Batch message {}", i)).expect("payload serialization")
            .priority(QueuePriority::Normal)
            .build());
    }

    // Enqueue batch
    println!("📤 Enqueuing {} messages in batch...", batch_messages.len());
    let start_time = std::time::Instant::now();
    let message_ids = queue.enqueue_batch(batch_messages.clone()).await?;
    let enqueue_time = start_time.elapsed();
    println!("✅ Batch enqueued in {:?}", enqueue_time);
    println!("📊 Enqueue rate: {:.2} messages/sec", message_ids.len() as f64 / enqueue_time.as_secs_f64());

    // Check queue statistics
    let stats = queue.get_stats().await?;
    println!("📈 Queue Stats:");
    println!("  - Visible messages: {}", stats.visible_messages);
    println!("  - Invisible messages: {}", stats.invisible_messages);
    println!("  - Total messages: {}", stats.total_messages);

    // Dequeue batch
    println!("\n📥 Dequeuing messages in batch...");
    let start_time = std::time::Instant::now();
    let batch_result = queue.dequeue_batch(5).await?;
    let dequeue_time = start_time.elapsed();

    println!("✅ Received {} messages in {:?}", batch_result.messages.len(), dequeue_time);
    println!("📊 Dequeue rate: {:.2} messages/sec", batch_result.messages.len() as f64 / dequeue_time.as_secs_f64());

    // Batch acknowledge
    let ack_ids: Vec<String> = batch_result.messages
        .iter()
        .map(|m| m.id.clone())
        .collect();

    let start_time = std::time::Instant::now();
    let ack_count = queue.ack_batch(ack_ids).await?;
    let ack_time = start_time.elapsed();
    println!("✅ Acknowledged {} messages in {:?}", ack_count, ack_time);

    // Example 4: Message with attributes
    println!("\n🏷️  Example 4: Messages with Attributes");
    println!("------------------------------------");

    use std::collections::HashMap;

    let mut attributes = HashMap::new();
    attributes.insert("source".to_string(), "api".to_string());
    attributes.insert("user_id".to_string(), "12345".to_string());
    attributes.insert("request_id".to_string(), "req-abc-123".to_string());

    let message_with_attrs = QueueMessage::builder()
        .payload("Process user data").expect("payload serialization")
        .priority(QueuePriority::High)
        .attributes(attributes)
        .build();

    println!("📤 Enqueuing message with custom attributes...");
    let id = queue.enqueue(message_with_attrs.clone()).await?;
    println!("✅ Message enqueued with ID: {}", id);

    if let Some(received_message) = queue.dequeue().await? {
        println!("📥 Received message with attributes:");
        println!("  - Payload: {}", received_message.payload);
        println!("  - Attributes:");
        for (key, value) in &received_message.attributes {
            println!("    {}: {}", key, value);
        }
        queue.ack(&received_message.id).await?;
    }

    // Example 5: Queue health monitoring
    println!("\n🏥 Example 5: Health Monitoring");
    println!("------------------------------");

    let health = queue.health_check().await?;
    println!("🏥 Queue Health Status:");
    println!("  - Status: {:?}", health.status);
    println!("  - Queue size: {}", health.queue_size);
    println!("  - Error rate: {:.2}%", health.error_rate * 100.0);
    println!("  - Last activity: {:?}", health.last_activity);
    println!("  - Checked at: {:?}", health.checked_at);

    // Final statistics
    println!("\n📊 Final Queue Statistics");
    println!("=========================");
    let final_stats = queue.get_stats().await?;
    println!("  - Visible messages: {}", final_stats.visible_messages);
    println!("  - Invisible messages: {}", final_stats.invisible_messages);
    println!("  - Dead letter messages: {}", final_stats.dead_letter_messages);
    println!("  - Total processed: {}", final_stats.total_processed);
    println!("  - Total failed: {}", final_stats.total_failed);

    // Cleanup
    println!("\n🧹 Cleaning up...");
    let purged_count = queue.purge().await?;
    println!("✅ Purged {} messages", purged_count);

    println!("\n🎉 Example completed successfully!");
    Ok(())
}
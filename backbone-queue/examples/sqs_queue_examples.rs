//! AWS SQS Queue Examples
//!
//! This example demonstrates various SQS queue operations:
//! - Basic SQS queue setup and usage
//! - FIFO queue operations
//! - Dead letter queue handling
//! - Batch operations and error handling

use backbone_queue::{
    QueueService,
    sqs::SqsQueueBuilder,
    types::{QueueMessage, QueuePriority}
};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 AWS SQS Queue Examples");
    println!("=========================");

    // Configuration - modify these for your AWS setup
    let queue_url = std::env::var("SQS_QUEUE_URL")
        .unwrap_or_else(|_| "https://sqs.us-east-1.amazonaws.com/123456789012/test-queue".to_string());
    let region = std::env::var("AWS_REGION")
        .unwrap_or_else(|_| "us-east-1".to_string());

    // Example 1: Basic SQS Queue Operations
    println!("\n📬 Example 1: Basic SQS Queue Operations");
    println!("----------------------------------------");

    // Create SQS queue connection
    println!("📡 Creating SQS queue connection...");
    let queue = SqsQueueBuilder::new()
        .queue_url(&queue_url)
        .region(&region)
        .visibility_timeout(30)
        .wait_time_seconds(5)
        .build()
        .await?;

    println!("✅ SQS queue connection established");

    // Test connection
    if queue.test_connection().await? {
        println!("✅ SQS connection test successful");
    } else {
        println!("⚠️  SQS connection test failed (queue might not exist)");
    }

    // Create a simple message
    let message = QueueMessage::builder()
        .payload("Hello from SQS Queue!")
        .priority(QueuePriority::Normal)
        .build();

    println!("📤 Enqueuing message: {}", message.payload);

    // Note: SQS operations might fail if the queue doesn't exist
    match queue.enqueue(message.clone()).await {
        Ok(message_id) => {
            println!("✅ Message enqueued successfully: {}", message_id);
        }
        Err(e) => {
            println!("⚠️  Failed to enqueue message: {}", e);
            println!("💡 Make sure the SQS queue exists and you have proper permissions");
        }
    }

    // Get queue statistics
    match queue.get_stats().await {
        Ok(stats) => {
            println!("📊 Queue Statistics:");
            println!("  - Visible messages: {}", stats.visible_messages);
            println!("  - Invisible messages: {}", stats.invisible_messages);
            println!("  - Total messages: {}", stats.total_messages);
        }
        Err(e) => {
            println!("⚠️  Failed to get statistics: {}", e);
        }
    }

    // Example 2: Priority Messages
    println!("\n🏆 Example 2: Priority Messages with Attributes");
    println!("--------------------------------------------");

    let priority_messages = vec![
        QueueMessage::builder()
            .payload("Low priority notification")
            .priority(QueuePriority::Low)
            .build(),
        QueueMessage::builder()
            .payload("High priority alert")
            .priority(QueuePriority::High)
            .build(),
        QueueMessage::builder()
            .payload("Critical emergency response")
            .priority(QueuePriority::Critical)
            .build(),
    ];

    for message in &priority_messages {
        println!("📤 Attempting to enqueue: {} ({})", message.payload, message.priority);
        match queue.enqueue(message.clone()).await {
            Ok(id) => println!("✅ Enqueued: {}", id),
            Err(e) => println!("❌ Failed: {}", e),
        }
    }

    // Example 3: Messages with Custom Attributes
    println!("\n🏷️  Example 3: Messages with Custom Attributes");
    println!("--------------------------------------------");

    let mut attributes = HashMap::new();
    attributes.insert("source_system".to_string(), "payment_service".to_string());
    attributes.insert("transaction_id".to_string(), "txn_123456".to_string());
    attributes.insert("customer_tier".to_string(), "premium".to_string());
    attributes.insert("priority_level".to_string(), "urgent".to_string());

    let attributed_message = QueueMessage::builder()
        .payload("Process high-value transaction")
        .priority(QueuePriority::High)
        .attributes(attributes)
        .delay(10) // Delay processing for 10 seconds
        .expires_in(3600) // Expire in 1 hour
        .build();

    println!("📤 Enqueuing message with custom attributes...");
    match queue.enqueue(attributed_message.clone()).await {
        Ok(id) => println!("✅ Message with attributes enqueued: {}", id),
        Err(e) => println!("❌ Failed to enqueue attributed message: {}", e),
    }

    // Example 4: Batch Operations
    println!("\n📦 Example 4: Batch Operations");
    println!("----------------------------");

    let mut batch_messages = Vec::new();
    for i in 1..=5 {
        let mut attrs = HashMap::new();
        attrs.insert("batch_index".to_string(), i.to_string());

        batch_messages.push(QueueMessage::builder()
            .payload(format!("Batch processing item {}", i))
            .priority(if i % 2 == 0 { QueuePriority::High } else { QueuePriority::Normal })
            .attributes(attrs)
            .build());
    }

    println!("📤 Attempting to enqueue {} messages in batch...", batch_messages.len());
    match queue.enqueue_batch(batch_messages).await {
        Ok(message_ids) => {
            println!("✅ Batch enqueue successful: {} messages", message_ids.len());
            println!("📋 Message IDs: {:?}", message_ids);
        }
        Err(e) => {
            println!("❌ Batch enqueue failed: {}", e);
        }
    }

    // Example 5: FIFO Queue Operations (if using FIFO queue)
    println!("\n🔄 Example 5: FIFO Queue Operations");
    println!("---------------------------------");

    let fifo_queue_url = queue_url.replace("/test-queue", "/test-queue.fifo");

    // Try to create FIFO queue connection
    let fifo_queue = SqsQueueBuilder::new()
        .queue_url(&fifo_queue_url)
        .region(&region)
        .build()
        .await;

    match fifo_queue {
        Ok(queue) => {
            println!("✅ FIFO queue connection established");

            // Create FIFO messages
            let fifo_messages = vec![
                QueueMessage::builder()
                    .payload("First task in sequence")
                    .message_group_id("workflow-1")
                    .message_deduplication_id("task-1")
                    .build(),
                QueueMessage::builder()
                    .payload("Second task in sequence")
                    .message_group_id("workflow-1")
                    .message_deduplication_id("task-2")
                    .build(),
                QueueMessage::builder()
                    .payload("Third task in sequence")
                    .message_group_id("workflow-1")
                    .message_deduplication_id("task-3")
                    .build(),
            ];

            for message in &fifo_messages {
                println!("📤 Enqueuing FIFO message: {}", message.payload);
                match queue.enqueue(message.clone()).await {
                    Ok(id) => println!("✅ FIFO message enqueued: {}", id),
                    Err(e) => println!("❌ FIFO enqueue failed: {}", e),
                }
            }
        }
        Err(e) => {
            println!("⚠️  FIFO queue setup failed: {}", e);
            println!("💡 FIFO queues require .fifo suffix and proper AWS configuration");
        }
    }

    // Example 6: Error Handling and Recovery
    println!("\n🛡️  Example 6: Error Handling and Recovery");
    println!("--------------------------------------");

    // Test with invalid message
    let invalid_message = QueueMessage::builder()
        .id("") // Invalid: empty ID
        .payload("Invalid message")
        .build();

    println!("📤 Attempting to enqueue invalid message...");
    match queue.enqueue(invalid_message).await {
        Ok(_) => println!("⚠️  Invalid message was accepted (unexpected)"),
        Err(e) => println!("✅ Invalid message properly rejected: {}", e),
    }

    // Example 7: Health Monitoring
    println!("\n🏥 Example 7: Health Monitoring");
    println!("----------------------------");

    let health = queue.health_check().await?;
    println!("🏥 SQS Queue Health:");
    println!("  - Status: {:?}", health.status);
    println!("  - Queue size: {}", health.queue_size);
    println!("  - Error rate: {:.2}%", health.error_rate * 100.0);
    println!("  - Checked at: {}", health.checked_at);

    // Additional health details
    for (key, value) in &health.details {
        println!("  - {}: {}", key, value);
    }

    // Example 8: Queue Size Monitoring
    println!("\n📊 Example 8: Queue Size Monitoring");
    println!("---------------------------------");

    let size = queue.size().await?;
    let is_empty = queue.is_empty().await?;

    println!("📊 Current Queue Status:");
    println!("  - Size: {} messages", size);
    println!("  - Is Empty: {}", is_empty);

    if !is_empty {
        println!("💡 Queue has messages ready for processing");
    }

    // Example 9: Configuration Validation
    println!("\n✅ Example 9: Configuration Validation");
    println!("------------------------------------");

    let validation_result = queue.validate_config().await?;
    println!("✅ Configuration validation: {}", validation_result);

    // Final summary
    println!("\n📋 SQS Queue Examples Summary");
    println!("==============================");
    println!("✅ Basic SQS operations demonstrated");
    println!("✅ Priority message handling shown");
    println!("✅ Custom attributes usage illustrated");
    println!("✅ Batch operations attempted");
    println!("✅ FIFO queue setup explored");
    println!("✅ Error handling patterns shown");
    println!("✅ Health monitoring performed");
    println!("✅ Configuration validation completed");

    println!("\n💡 Important Notes:");
    println!("- Ensure AWS credentials are properly configured");
    println!("- SQS queues must exist before running these examples");
    println!("- FIFO queues require .fifo suffix in queue URL");
    println!("- Some operations might fail due to AWS permissions");
    println!("- Check AWS IAM policies for required SQS permissions");

    println!("\n🎉 SQS Queue Examples completed!");

    Ok(())
}
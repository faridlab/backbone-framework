//! RabbitMQ Webhook Processing Example
//!
//! This example demonstrates a real-world webhook processing system using RabbitMQ
//! Features shown:
//! - Webhook intake and queueing
//! - Retry mechanisms with exponential backoff
//! - Dead letter exchange for failed webhooks
//! - Status tracking and monitoring
//! - Priority processing for critical webhooks
//!
//! Run with: cargo run --example rabbitmq_webhook_processor

use backbone_queue::{
    rabbitmq_simple::{RabbitMQQueueSimple, RabbitMQConfig, ExchangeType},
    traits::QueueService,
    types::{QueueMessage, QueuePriority},
    utils::rabbitmq_simple::*,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebhookPayload {
    id: String,
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: String,
    timestamp: String,
    retry_count: u32,
    max_retries: u32,
    priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebhookResult {
    webhook_id: String,
    status: String,
    response_code: Option<u16>,
    response_body: Option<String>,
    processing_time_ms: u64,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("🪝 RabbitMQ Webhook Processing System");
    println!("=====================================\n");

    // Setup webhook processing queues
    setup_webhook_infrastructure().await?;

    // Example 1: Process incoming webhook
    println!("📥 Example 1: Incoming Webhook Processing");
    process_incoming_webhook().await?;
    println!();

    // Example 2: Retry failed webhook
    println!("🔄 Example 2: Failed Webhook Retry");
    retry_failed_webhook().await?;
    println!();

    // Example 3: Priority webhook processing
    println!("⚡ Example 3: Priority Webhook Processing");
    process_priority_webhooks().await?;
    println!();

    // Example 4: Webhook batch processing
    println!("📦 Example 4: Batch Webhook Processing");
    batch_process_webhooks().await?;
    println!();

    // Example 5: Monitor webhook processing
    println!("📊 Example 5: Webhook Processing Monitor");
    monitor_webhook_processing().await?;
    println!();

    println!("✅ Webhook processing examples completed!");
    Ok(())
}

async fn setup_webhook_infrastructure() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🏗️  Setting up webhook processing infrastructure...");

    // Main webhook processing queue
    let webhook_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "webhooks.processing".to_string(),
            exchange_name: "webhooks.direct".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("webhook.process".to_string()),
        }
    ).await?;

    // High priority webhook queue
    let priority_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "webhooks.high_priority".to_string(),
            exchange_name: "webhooks.priority".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("webhook.priority".to_string()),
        }
    ).await?;

    // Failed webhook queue (for retries)
    let retry_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "webhooks.retry".to_string(),
            exchange_name: "webhooks.retry".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("webhook.retry.*".to_string()),
        }
    ).await?;

    // Dead letter queue for permanently failed webhooks
    let dlq_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "webhooks.dead_letter".to_string(),
            exchange_name: "webhooks.dlq".to_string(),
            exchange_type: ExchangeType::Fanout,
            routing_key: None,
        }
    ).await?;

    println!("   ✓ Webhook processing queue: webhooks.processing");
    println!("   ✓ Priority webhook queue: webhooks.high_priority");
    println!("   ✓ Retry queue: webhooks.retry");
    println!("   ✓ Dead letter queue: webhooks.dead_letter");
    println!("   ✓ Infrastructure setup complete!\n");

    // Store queues for later use
    // In a real application, these would be managed by a dependency injection system
    Ok(())
}

async fn process_incoming_webhook() -> Result<(), Box<dyn std::error::Error>> {
    println!("   📥 Processing incoming webhook...");

    // Simulate incoming webhook from external service
    let webhook = WebhookPayload {
        id: "webhook_123456".to_string(),
        url: "https://api.customer.com/webhook".to_string(),
        method: "POST".to_string(),
        headers: {
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json".to_string());
            headers.insert("X-Webhook-Signature".to_string(), "sha256=abc123".to_string());
            headers
        },
        body: serde_json::json!({
            "event": "order.created",
            "order_id": "ord_789012",
            "customer_id": "cust_345678",
            "amount": 99.99,
            "currency": "USD"
        }).to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        retry_count: 0,
        max_retries: 3,
        priority: "normal".to_string(),
    };

    // Create webhook queue configuration
    let webhook_queue = RabbitMQQueueSimple::new(
        dev_config("webhooks.processing", "webhooks.direct")
    ).await?;

    // Convert webhook to QueueMessage
    let webhook_message = QueueMessage::builder()
        .payload(serde_json::to_value(webhook)?)
        .expect("Failed to serialize webhook payload")
        .priority(QueuePriority::Normal)
        .routing_key("webhook.process")
        .build();

    // Enqueue webhook for processing
    let message_id = webhook_queue.enqueue(webhook_message).await?;
    println!("   ✓ Webhook enqueued for processing: {}", message_id);

    // Simulate webhook processing
    simulate_webhook_processing(&webhook_queue, &message_id).await?;

    Ok(())
}

async fn simulate_webhook_processing(
    queue: &RabbitMQQueueSimple,
    message_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   ⚙️  Simulating webhook processing...");

    // Simulate processing time
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Simulate successful webhook delivery
    let result = WebhookResult {
        webhook_id: message_id.to_string(),
        status: "delivered".to_string(),
        response_code: Some(200),
        response_body: Some("{\"status\": \"ok\"}".to_string()),
        processing_time_ms: 487,
        error: None,
    };

    // Store result (in real app, this would go to a database)
    println!("   ✓ Webhook delivered successfully:");
    println!("     - Response Code: {}", result.response_code.unwrap_or(0));
    println!("     - Processing Time: {}ms", result.processing_time_ms);
    println!("     - Status: {}", result.status);

    // Acknowledge the webhook message
    queue.ack(message_id).await?;
    println!("   ✓ Webhook message acknowledged");

    Ok(())
}

async fn retry_failed_webhook() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🔄 Processing failed webhook retry...");

    // Create a webhook that failed initially
    let failed_webhook = WebhookPayload {
        id: "webhook_789012".to_string(),
        url: "https://api.unreliable.com/webhook".to_string(),
        method: "POST".to_string(),
        headers: {
            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), "Bearer token123".to_string());
            headers
        },
        body: serde_json::json!({
            "event": "payment.failed",
            "payment_id": "pay_345678",
            "amount": 199.99,
            "error": "Insufficient funds"
        }).to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        retry_count: 1,
        max_retries: 5,
        priority: "high".to_string(),
    };

    let retry_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "webhooks.retry".to_string(),
            exchange_name: "webhooks.retry".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("webhook.retry.high".to_string()),
        }
    ).await?;

    // Calculate retry delay (exponential backoff)
    let retry_delay_ms = 1000 * (2_u64.pow(failed_webhook.retry_count));
    println!("   🕐 Scheduling retry in {}ms (attempt {}/{})",
             retry_delay_ms,
             failed_webhook.retry_count + 1,
             failed_webhook.max_retries);

    // Create retry message with delay
    let retry_message = QueueMessage::builder()
        .payload(serde_json::to_value(failed_webhook)?)
        .expect("Failed to serialize retry webhook")
        .priority(QueuePriority::High)
        .routing_key("webhook.retry.high")
        .build();

    let retry_id = retry_queue.enqueue(retry_message).await?;
    println!("   ✓ Webhook scheduled for retry: {}", retry_id);

    // Simulate retry processing
    tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;

    let success_rate = 0.7; // 70% success rate on retry
    let success = rand::random::<f64>() < success_rate;

    if success {
        println!("   ✓ Webhook retry succeeded!");
        retry_queue.ack(&retry_id).await?;
    } else {
        println!("   ❌ Webhook retry failed, will try again...");
        // In real implementation, would re-enqueue with increased retry count
        retry_queue.nack(&retry_id, Some(5)).await?;
    }

    Ok(())
}

async fn process_priority_webhooks() -> Result<(), Box<dyn std::error::Error>> {
    println!("   ⚡ Processing priority webhooks...");

    let priority_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "webhooks.high_priority".to_string(),
            exchange_name: "webhooks.priority".to_string(),
            exchange_type: ExchangeType::Direct,
            routing_key: Some("webhook.priority".to_string()),
        }
    ).await?;

    // Create critical webhook (security alert)
    let critical_webhook = WebhookPayload {
        id: "webhook_critical_001".to_string(),
        url: "https://security.company.com/alerts".to_string(),
        method: "POST".to_string(),
        headers: {
            let mut headers = HashMap::new();
            headers.insert("Priority".to_string(), "critical".to_string());
            headers.insert("X-Alert-Type".to_string(), "security_breach".to_string());
            headers
        },
        body: serde_json::json!({
            "alert_type": "SECURITY_BREACH",
            "severity": "CRITICAL",
            "source_ip": "192.168.1.100",
            "user_id": 12345,
            "action": "account_locked",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }).to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        retry_count: 0,
        max_retries: 10, // More retries for critical webhooks
        priority: "critical".to_string(),
    };

    let critical_message = QueueMessage::builder()
        .payload(serde_json::to_value(critical_webhook)?)
        .expect("Failed to serialize critical webhook")
        .priority(QueuePriority::Critical)
        .routing_key("webhook.priority")
        .build();

    let critical_id = priority_queue.enqueue(critical_message).await?;
    println!("   🚨 Critical webhook queued: {}", critical_id);

    // Simulate immediate processing of critical webhook
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("   ⚡ Critical webhook processed immediately");

    // Create high-priority business webhook
    let business_webhook = WebhookPayload {
        id: "webhook_business_002".to_string(),
        url: "https://api.partner.com/order_notification".to_string(),
        method: "POST".to_string(),
        headers: {
            let mut headers = HashMap::new();
            headers.insert("Priority".to_string(), "high".to_string());
            headers.insert("X-Event-Type".to_string(), "order_completed".to_string());
            headers
        },
        body: serde_json::json!({
            "event": "order.completed",
            "order_id": "ord_999999",
            "customer_id": "cust_888888",
            "amount": 1500.00,
            "items": ["laptop", "mouse", "keyboard"]
        }).to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        retry_count: 0,
        max_retries: 5,
        priority: "high".to_string(),
    };

    let business_message = QueueMessage::builder()
        .payload(serde_json::to_value(business_webhook)?)
        .expect("Failed to serialize business webhook")
        .priority(QueuePriority::High)
        .routing_key("webhook.priority")
        .build();

    let business_id = priority_queue.enqueue(business_message).await?;
    println!("   💼 High-priority business webhook queued: {}", business_id);

    // Acknowledge both messages
    priority_queue.ack(&critical_id).await?;
    priority_queue.ack(&business_id).await?;
    println!("   ✓ All priority webhooks processed and acknowledged");

    Ok(())
}

async fn batch_process_webhooks() -> Result<(), Box<dyn std::error::Error>> {
    println!("   📦 Batch processing webhooks...");

    let webhook_queue = RabbitMQQueueSimple::new(
        dev_config("webhooks.processing", "webhooks.direct")
    ).await?;

    // Create batch of webhooks (e.g., from bulk operation)
    let mut webhook_batch = Vec::new();

    for i in 1..=50 {
        let webhook = WebhookPayload {
            id: format!("webhook_batch_{:03}", i),
            url: format!("https://api.example.com/webhooks/batch_{}", i),
            method: "POST".to_string(),
            headers: {
                let mut headers = HashMap::new();
                headers.insert("X-Batch-ID".to_string(), "batch_123".to_string());
                headers.insert("Content-Type".to_string(), "application/json".to_string());
                headers
            },
            body: serde_json::json!({
                "batch_id": "batch_123",
                "webhook_number": i,
                "data": format!("Payload data for webhook {}", i),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }).to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            retry_count: 0,
            max_retries: 3,
            priority: "normal".to_string(),
        };

        let message = QueueMessage::builder()
            .payload(serde_json::to_value(webhook)?)
            .expect("Failed to serialize batch webhook")
            .priority(QueuePriority::Normal)
            .routing_key("webhook.process")
            .build();

        webhook_batch.push(message);
    }

    println!("   📊 Preparing batch of {} webhooks", webhook_batch.len());

    // Process the batch
    let start_time = std::time::Instant::now();
    let message_ids = webhook_queue.enqueue_batch(webhook_batch).await?;
    let processing_time = start_time.elapsed();

    println!("   ✓ Batch processed in {:?}", processing_time);
    println!("   ✓ Webhooks per second: {:.2}", message_ids.len() as f64 / processing_time.as_secs_f64());
    println!("   ✓ Generated {} message IDs", message_ids.len());

    // Simulate batch processing results
    let success_rate = 0.95; // 95% success rate
    let successful = (message_ids.len() as f64 * success_rate) as usize;
    let failed = message_ids.len() - successful;

    println!("   📈 Batch Processing Results:");
    println!("     - Total Webhooks: {}", message_ids.len());
    println!("     - Successful: {} ({:.1}%)", successful, success_rate * 100.0);
    println!("     - Failed: {} ({:.1}%)", failed, (1.0 - success_rate) * 100.0);

    // Acknowledge all messages (successful ones)
    for id in &message_ids[..successful.min(message_ids.len())] {
        webhook_queue.ack(id).await?;
    }

    // Failed ones would go to retry queue
    for id in &message_ids[successful..] {
        webhook_queue.nack(id, Some(10)).await?;
    }

    Ok(())
}

async fn monitor_webhook_processing() -> Result<(), Box<dyn std::error::Error>> {
    println!("   📊 Monitoring webhook processing...");

    let webhook_queue = RabbitMQQueueSimple::new(
        dev_config("webhooks.processing", "webhooks.direct")
    ).await?;

    // Health check
    let health = webhook_queue.health_check().await?;
    println!("   💓 Webhook Queue Health:");
    println!("     - Status: {:?}", health.status);
    println!("     - Queue Size: {}", health.queue_size);
    println!("     - Error Rate: {:.2}%", health.error_rate * 100.0);

    // Statistics
    let stats = webhook_queue.get_stats().await?;
    println!("\n   📈 Webhook Processing Statistics:");
    println!("     - Total Processed: {}", stats.total_processed);
    println!("     - Total Failed: {}", stats.total_failed);
    println!("     - Success Rate: {:.2}%", stats.success_rate() * 100.0);
    println!("     - Current Queue Size: {}", stats.total_messages);

    // Queue metrics
    let queue_size = webhook_queue.size().await?;
    let is_empty = webhook_queue.is_empty().await?;

    println!("\n   📏 Current Queue Metrics:");
    println!("     - Messages in Queue: {}", queue_size);
    println!("     - Queue Empty: {}", if is_empty { "Yes" } else { "No" });

    // Simulate monitoring different queue types
    println!("\n   🔍 Monitoring Multiple Queue Types:");

    let queue_types = vec![
        ("webhooks.processing", "Processing Queue"),
        ("webhooks.high_priority", "Priority Queue"),
        ("webhooks.retry", "Retry Queue"),
        ("webhooks.dead_letter", "Dead Letter Queue"),
    ];

    for (queue_name, description) in queue_types {
        let config = dev_config(queue_name, "webhooks.direct");
        if let Ok(queue) = RabbitMQQueueSimple::new(config).await {
            let size = queue.size().await?;
            println!("     - {}: {} messages", description, size);
        }
    }

    println!("\n   ✅ Webhook monitoring complete");

    Ok(())
}

// Add this to your Cargo.toml dependencies:
// rand = "0.8"
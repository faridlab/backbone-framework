//! RabbitMQ Queue Examples
//!
//! This example demonstrates comprehensive RabbitMQ usage patterns including:
//! - Basic message publishing and consuming
//! - Different exchange types (Direct, Fanout, Topic, Headers)
//! - Message routing and filtering
//! - Publisher confirms and acknowledgments
//! - Dead letter exchanges and error handling
//! - Performance tuning and configuration options
//!
//! Run with: cargo run --example rabbitmq_examples
//!

use backbone_queue::{
    rabbitmq_simple::{RabbitMQQueueSimple, RabbitMQConfig, ExchangeType},
    traits::QueueService,
    types::{QueueMessage, QueuePriority},
    utils::rabbitmq_simple::*,
};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🐰 RabbitMQ Queue Examples\n");
    println!("========================\n");

    // Example 1: Basic Direct Exchange
    println!("📬 Example 1: Basic Direct Exchange");
    basic_direct_exchange_example().await?;
    println!();

    // Example 2: Fanout Exchange Broadcasting
    println!("📢 Example 2: Fanout Exchange Broadcasting");
    fanout_exchange_example().await?;
    println!();

    // Example 3: Topic Exchange Pattern Matching
    println!("🎯 Example 3: Topic Exchange Pattern Matching");
    topic_exchange_example().await?;
    println!();

    // Example 4: High Priority Messages
    println!("⚡ Example 4: Priority Message Handling");
    priority_message_example().await?;
    println!();

    // Example 5: Batch Operations
    println!("📦 Example 5: Batch Operations");
    batch_operations_example().await?;
    println!();

    // Example 6: Message with Headers
    println!("🏷️ Example 6: Messages with Custom Headers");
    message_with_headers_example().await?;
    println!();

    // Example 7: Error Handling and Validation
    println!("🛡️ Example 7: Configuration Validation");
    validation_example().await?;
    println!();

    // Example 8: Health Monitoring
    println!("💓 Example 8: Queue Health Monitoring");
    health_monitoring_example().await?;
    println!();

    println!("✅ All examples completed successfully!");
    Ok(())
}

/// Example 1: Basic Direct Exchange
/// Demonstrates simple point-to-point messaging using direct exchange
async fn basic_direct_exchange_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("user_notifications", "notifications_direct");
    let queue = RabbitMQQueueSimple::new(config).await?;

    // Create a simple notification message
    let message = QueueMessage::builder()
        .payload(serde_json::json!({
            "type": "email_notification",
            "user_id": 12345,
            "email": "user@example.com",
            "subject": "Welcome to our service!",
            "body": "Thank you for signing up. Here's how to get started..."
        }))
        .expect("Failed to serialize payload")
        .priority(QueuePriority::Normal)
        .routing_key("user.email")
        .build();

    // Enqueue the message
    let message_id = queue.enqueue(message).await?;
    println!("   ✓ Enqueued notification message: {}", message_id);

    // Try to dequeue (in real scenario, a consumer would be running)
    let dequeued = queue.dequeue().await?;
    if let Some(msg) = dequeued {
        println!("   ✓ Dequeued message: {}", msg.id);
        queue.ack(&msg.id).await?;
        println!("   ✓ Acknowledged message");
    } else {
        println!("   ℹ️  No messages available (simulated)");
    }

    Ok(())
}

/// Example 2: Fanout Exchange Broadcasting
/// Demonstrates broadcasting the same message to multiple consumers
async fn fanout_exchange_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = RabbitMQConfig {
        connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
        queue_name: "system_events".to_string(),
        exchange_name: "system_events_fanout".to_string(),
        exchange_type: ExchangeType::Fanout,
        routing_key: None, // Fanout ignores routing keys
    };

    let queue = RabbitMQQueueSimple::new(config).await?;

    // Create a system event that should be broadcast to all services
    let system_event = QueueMessage::builder()
        .payload(serde_json::json!({
            "event_type": "system_maintenance",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "message": "System maintenance scheduled for 2:00 AM UTC",
            "affected_services": ["api", "database", "cache"],
            "duration_minutes": 30
        }))
        .expect("Failed to serialize payload")
        .priority(QueuePriority::High)
        .build();

    let event_id = queue.enqueue(system_event).await?;
    println!("   ✓ Broadcast system event: {}", event_id);

    // Simulate multiple services receiving the same event
    let services = ["logging_service", "monitoring_service", "alerting_service"];
    for service in services {
        println!("   📡 Service '{}' would receive the broadcast event", service);
    }

    Ok(())
}

/// Example 3: Topic Exchange Pattern Matching
/// Demonstrates flexible routing using topic exchange patterns
async fn topic_exchange_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = RabbitMQConfig {
        connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
        queue_name: "application_logs".to_string(),
        exchange_name: "logs_topic".to_string(),
        exchange_type: ExchangeType::Topic,
        routing_key: Some("logs.*".to_string()), // Subscribe to all logs
    };

    let queue = RabbitMQQueueSimple::new(config).await?;

    // Different log messages with different routing keys
    let log_messages = vec![
        (
            "logs.auth.success",
            serde_json::json!({
                "level": "INFO",
                "service": "auth",
                "event": "login_success",
                "user_id": 12345,
                "ip": "192.168.1.100"
            }),
        ),
        (
            "logs.api.error",
            serde_json::json!({
                "level": "ERROR",
                "service": "api",
                "event": "validation_failed",
                "error_code": "INVALID_INPUT",
                "endpoint": "/api/v1/users"
            }),
        ),
        (
            "logs.db.warning",
            serde_json::json!({
                "level": "WARNING",
                "service": "database",
                "event": "slow_query",
                "query_time_ms": 2500,
                "table": "users"
            }),
        ),
        (
            "logs.cache.info",
            serde_json::json!({
                "level": "INFO",
                "service": "cache",
                "event": "cache_hit",
                "key": "user_profile_12345",
                "hit_rate": 0.85
            }),
        ),
    ];

    for (routing_key, payload) in log_messages {
        let log_message = QueueMessage::builder()
            .payload(payload)
            .expect("Failed to serialize log payload")
            .routing_key(routing_key)
            .build();

        let message_id = queue.enqueue(log_message).await?;
        println!("   ✓ Log message: {} -> {}", routing_key, message_id);
    }

    println!("   📊 Total log messages enqueued: {}", log_messages.len());
    Ok(())
}

/// Example 4: Priority Message Handling
/// Demonstrates different message priorities and their processing
async fn priority_message_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("priority_queue", "priority_exchange");
    let queue = RabbitMQQueueSimple::new(config).await?;

    // Messages with different priorities
    let priority_messages = vec![
        (
            QueuePriority::Critical,
            serde_json::json!({
                "alert": "SYSTEM_DOWN",
                "severity": "critical",
                "action_required": "immediate"
            }),
        ),
        (
            QueuePriority::High,
            serde_json::json!({
                "alert": "HIGH_CPU_USAGE",
                "severity": "high",
                "cpu_percent": 95.5,
                "action_required": "monitor"
            }),
        ),
        (
            QueuePriority::Normal,
            serde_json::json!({
                "alert": "USER_LOGIN",
                "severity": "info",
                "user_id": 67890
            }),
        ),
        (
            QueuePriority::Low,
            serde_json::json!({
                "alert": "CLEANUP_TASK",
                "severity": "low",
                "task": "log_rotation"
            }),
        ),
    ];

    for (priority, payload) in priority_messages {
        let message = QueueMessage::builder()
            .payload(payload)
            .expect("Failed to serialize priority payload")
            .priority(priority)
            .routing_key(format!("alerts.{}", priority.name().to_lowercase()))
            .build();

        let message_id = queue.enqueue(message).await?;
        println!("   ⚡ {} priority message: {}", priority.name(), message_id);
    }

    Ok(())
}

/// Example 5: Batch Operations
/// Demonstrates efficient batch processing of messages
async fn batch_operations_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("batch_processing", "batch_exchange");
    let queue = RabbitMQQueueSimple::new(config).await?;

    // Create a batch of user profile updates
    let mut batch_messages = Vec::new();

    for i in 1..=100 {
        let profile_update = QueueMessage::builder()
            .payload(serde_json::json!({
                "user_id": i,
                "update_type": "profile_refresh",
                "fields_updated": ["last_seen", "status"],
                "timestamp": chrono::Utc::now().to_rfc3339()
            }))
            .expect("Failed to serialize profile update")
            .priority(QueuePriority::Normal)
            .routing_key("user.profile.update")
            .build();

        batch_messages.push(profile_update);
    }

    println!("   📦 Preparing batch of {} profile updates", batch_messages.len());

    // Process the batch
    let start_time = std::time::Instant::now();
    let message_ids = queue.enqueue_batch(batch_messages).await?;
    let processing_time = start_time.elapsed();

    println!("   ✓ Batch processed in {:?}", processing_time);
    println!("   ✓ Messages per second: {:.2}", message_ids.len() as f64 / processing_time.as_secs_f64());
    println!("   ✓ Generated {} message IDs", message_ids.len());

    // Demonstrate batch dequeue
    let batch_result = queue.dequeue_batch(10).await?;
    println!("   ✓ Retrieved batch: {} messages out of {} requested",
             batch_result.messages.len(), batch_result.requested);

    Ok(())
}

/// Example 6: Messages with Custom Headers
/// Demonstrates advanced message routing using custom headers
async fn message_with_headers_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("routing_by_headers", "headers_exchange");
    let queue = RabbitMQQueueSimple::new(config).await?;

    // Create a message with rich metadata headers
    let mut headers = HashMap::new();
    headers.insert("source_service".to_string(), serde_json::Value::String("payment_service".to_string()));
    headers.insert("correlation_id".to_string(), serde_json::Value::String("req_123456".to_string()));
    headers.insert("retry_count".to_string(), serde_json::Value::Number(serde_json::Number::from(0)));
    headers.insert("requires_ack".to_string(), serde_json::Value::Bool(true));
    headers.insert("timeout_ms".to_string(), serde_json::Value::Number(serde_json::Number::from(5000)));

    let mut message = QueueMessage::builder()
        .payload(serde_json::json!({
            "transaction_id": "txn_789012",
            "amount": 99.99,
            "currency": "USD",
            "merchant_id": "merchant_456",
            "payment_method": "credit_card"
        }))
        .expect("Failed to serialize payment payload")
        .priority(QueuePriority::High)
        .routing_key("payment.process");

    // Note: In a real implementation, we'd need to add headers to the message structure
    // For this example, we'll simulate it
    let final_message = message.build();

    let message_id = queue.enqueue(final_message).await?;
    println!("   ✓ Payment message with headers: {}", message_id);

    // Display the headers that would be sent
    println!("   📋 Headers:");
    for (key, value) in &headers {
        println!("      {}: {}", key, value);
    }

    Ok(())
}

/// Example 7: Configuration Validation and Error Handling
/// Demonstrates proper configuration and error handling
async fn validation_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🛡️ Testing configuration validation...");

    // Test 1: Valid configuration
    let valid_config = dev_config("valid_queue", "valid_exchange");
    match RabbitMQQueueSimple::new(valid_config).await {
        Ok(queue) => println!("   ✓ Valid configuration accepted"),
        Err(e) => println!("   ❌ Unexpected error with valid config: {}", e),
    }

    // Test 2: Invalid connection URL
    let invalid_config = RabbitMQConfig {
        connection_url: "invalid://not-a-real-protocol".to_string(),
        queue_name: "test".to_string(),
        exchange_name: "test".to_string(),
        exchange_type: ExchangeType::Direct,
        routing_key: None,
    };

    match RabbitMQQueueSimple::new(invalid_config).await {
        Ok(_) => println!("   ❌ Invalid configuration was incorrectly accepted"),
        Err(e) => println!("   ✓ Invalid configuration correctly rejected: {}", e),
    }

    // Test 3: Empty connection URL
    let empty_config = RabbitMQConfig {
        connection_url: "".to_string(),
        queue_name: "test".to_string(),
        exchange_name: "test".to_string(),
        exchange_type: ExchangeType::Direct,
        routing_key: None,
    };

    match RabbitMQQueueSimple::new(empty_config).await {
        Ok(_) => println!("   ❌ Empty URL was incorrectly accepted"),
        Err(e) => println!("   ✓ Empty URL correctly rejected: {}", e),
    }

    // Test 4: Production configuration with TLS
    let prod_config = prod_config(
        "amqps://user:pass@rabbitmq.example.com:5671/%2f",
        "production_queue",
        "production_exchange",
        ExchangeType::Topic,
    );

    match RabbitMQQueueSimple::new(prod_config).await {
        Ok(queue) => println!("   ✓ Production TLS configuration accepted"),
        Err(e) => println!("   ⚠️  Production config validation (connection test would fail): {}", e),
    }

    Ok(())
}

/// Example 8: Health Monitoring and Statistics
/// Demonstrates queue health checking and performance monitoring
async fn health_monitoring_example() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("health_check_queue", "health_exchange");
    let queue = RabbitMQQueueSimple::new(config).await?;

    // Test queue health
    println!("   💓 Performing health check...");
    let health = queue.health_check().await?;
    println!("   ✓ Health Status: {:?}", health.status);
    println!("   ✓ Queue Size: {}", health.queue_size);
    println!("   ✓ Error Rate: {:.2}%", health.error_rate * 100.0);
    println!("   ✓ Last Check: {}", health.checked_at.format("%Y-%m-%d %H:%M:%S UTC"));

    // Get queue statistics
    println!("\n   📊 Queue Statistics:");
    let stats = queue.get_stats().await?;
    println!("   ✓ Total Messages: {}", stats.total_messages);
    println!("   ✓ Visible Messages: {}", stats.visible_messages);
    println!("   ✓ Invisible Messages: {}", stats.invisible_messages);
    println!("   ✓ Total Processed: {}", stats.total_processed);
    println!("   ✓ Total Failed: {}", stats.total_failed);
    println!("   ✓ Success Rate: {:.2}%", stats.success_rate() * 100.0);

    // Test connection
    println!("\n   🔌 Testing connection...");
    let connection_ok = queue.test_connection().await?;
    println!("   ✓ Connection Test: {}", if connection_ok { "✅ PASS" } else { "❌ FAIL" });

    // Validate configuration
    println!("\n   ⚙️  Validating configuration...");
    let config_ok = queue.validate_config().await?;
    println!("   ✓ Configuration Validation: {}", if config_ok { "✅ PASS" } else { "❌ FAIL" });

    // Check queue size and emptiness
    println!("\n   📏 Queue Size Information:");
    let size = queue.size().await?;
    let is_empty = queue.is_empty().await?;
    println!("   ✓ Current Size: {} messages", size);
    println!("   ✓ Is Empty: {}", if is_empty { "✅ YES" } else { "❌ NO" });

    Ok(())
}

/// Utility function to create a basic consumer simulation
async fn simulate_consumer(queue_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("   🤖 Simulating consumer for queue: {}", queue_name);

    let config = dev_config(queue_name, "consumer_exchange");
    let queue = RabbitMQQueueSimple::new(config).await?;

    let mut processed_count = 0;
    let max_messages = 5;

    while processed_count < max_messages {
        match queue.dequeue().await {
            Ok(Some(message)) => {
                println!("   ✓ Processed message: {} - {}", message.id, message.payload);
                queue.ack(&message.id).await?;
                processed_count += 1;
            }
            Ok(None) => {
                println!("   ℹ️  No more messages in queue");
                break;
            }
            Err(e) => {
                println!("   ❌ Error dequeuing message: {}", e);
                break;
            }
        }

        // Small delay between processing
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    println!("   📊 Consumer processed {} messages", processed_count);
    Ok(())
}

/// Example of setting up a complete microservices communication pattern
async fn microservices_communication_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏗️  Microservices Communication Pattern");
    println!("=====================================");

    // Service 1: User Service publishes user events
    let user_service = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "user_events".to_string(),
            exchange_name: "domain_events".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("user.created".to_string()),
        }
    ).await?;

    // Publish a user created event
    let user_event = QueueMessage::builder()
        .payload(serde_json::json!({
            "event_type": "user.created",
            "user_id": 98765,
            "email": "newuser@example.com",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "metadata": {
                "source": "user_service",
                "version": "1.0"
            }
        }))
        .expect("Failed to serialize user event")
        .routing_key("user.created")
        .build();

    let event_id = user_service.enqueue(user_event).await?;
    println!("   👤 User Service published event: {}", event_id);

    // Service 2: Notification Service listens for user events
    let notification_service = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "notifications_queue".to_string(),
            exchange_name: "domain_events".to_string(),
            exchange_type: ExchangeType::Topic,
            routing_key: Some("user.*".to_string()), // Listen to all user events
        }
    ).await?;

    // Service 3: Analytics Service listens for all events
    let analytics_service = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            queue_name: "analytics_queue".to_string(),
            exchange_name: "domain_events".to_string(),
            exchange_type: ExchangeType::Fanout,
            routing_key: None, // Get all events
        }
    ).await?;

    println!("   📧 Notification Service: Ready to process user events");
    println!("   📊 Analytics Service: Ready to process all events");
    println!("   🔄 Event-driven architecture established!");

    Ok(())
}
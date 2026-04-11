# RabbitMQ Integration Guide

This guide covers comprehensive RabbitMQ integration with the Backbone Queue module.

## 🐰 Overview

RabbitMQ is a powerful message broker that implements the AMQP (Advanced Message Queuing Protocol) protocol. It provides robust messaging capabilities with features like:

- **Exchange Types**: Direct, Fanout, Topic, Headers
- **Message Routing**: Flexible routing with patterns and headers
- **Reliability**: Publisher confirms, acknowledgments, persistence
- **Scalability**: Clustering, load balancing, high availability
- **Security**: TLS/SSL support, access control, authentication

## 🚀 Quick Start

### Basic Setup

```rust
use backbone_queue::{
    RabbitMQQueueSimple, RabbitMQConfig, ExchangeType,
    QueueMessage, QueuePriority, QueueService,
    utils::rabbitmq_simple::*,
};

// Create a RabbitMQ queue with default configuration
let config = dev_config("my_queue", "my_exchange");
let queue = RabbitMQQueueSimple::new(config).await?;

// Send a message
let message = QueueMessage::builder()
    .payload("Hello, RabbitMQ!")
    .priority(QueuePriority::Normal)
    .routing_key("my.routing.key")
    .build();

let message_id = queue.enqueue(message).await?;
println!("Message sent with ID: {}", message_id);
```

### Production Setup

```rust
// Production configuration with TLS
let config = prod_config(
    "amqps://user:pass@rabbitmq.example.com:5671/%2f",
    "production_queue",
    "production_exchange",
    ExchangeType::Topic
);

let queue = RabbitMQQueueSimple::new(config).await?;
```

## 📡 Exchange Types

### Direct Exchange
Point-to-point messaging using exact routing key matches:

```rust
let config = RabbitMQConfig {
    exchange_type: ExchangeType::Direct,
    routing_key: Some("user.notifications".to_string()),
    // ... other fields
};

// Message will only be delivered to queues bound with "user.notifications"
```

### Fanout Exchange
Broadcast messages to all bound queues:

```rust
let config = RabbitMQConfig {
    exchange_type: ExchangeType::Fanout,
    routing_key: None, // Fanout ignores routing keys
    // ... other fields
};

// Message goes to ALL bound queues
```

### Topic Exchange
Pattern-based routing with wildcards:

```rust
let config = RabbitMQConfig {
    exchange_type: ExchangeType::Topic,
    routing_key: Some("logs.*.error".to_string()),
    // ... other fields
};

// Routes to:
// - logs.auth.error
// - logs.api.error
// - logs.db.error
// Wildcard patterns: * (any word), # (any word)
```

## 🎯 Message Routing

### Basic Routing

```rust
let message = QueueMessage::builder()
    .payload(serde_json::json!({
        "event": "user_created",
        "user_id": 12345
    }))
    .expect("Failed to serialize")
    .routing_key("users.created")  // Routing key determines delivery
    .build();
```

### Advanced Routing with Headers

```rust
let mut headers = HashMap::new();
headers.insert("source".to_string(), serde_json::Value::String("api".to_string()));
headers.insert("priority".to_string(), serde_json::Value::Number(1.into()));

// In full implementation, headers would be included in AMQP properties
let message = QueueMessage::builder()
    .payload(payload)
    .routing_key("events.user")
    .build();
```

## 🔧 Configuration Options

### Connection Settings

```rust
let config = RabbitMQConfig {
    // Connection
    connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),

    // Basic settings
    queue_name: "my_queue".to_string(),
    exchange_name: "my_exchange".to_string(),
    exchange_type: ExchangeType::Topic,
    routing_key: Some("events.*".to_string()),

    // Advanced features
    durable: true,                    // Persistent messages
    message_ttl: Some(3600000),        // 1 hour TTL
    dead_letter_exchange: Some("dlx".to_string()),
    max_retries: 5,

    // Performance tuning
    heartbeat: 60,                     // Heartbeat interval in seconds
    connection_timeout: 30,             // Connection timeout

    // Custom arguments
    queue_arguments: HashMap::new(),
    exchange_arguments: HashMap::new(),
};
```

### Advanced Queue Arguments

```rust
let mut queue_args = HashMap::new();

// Message TTL (time-to-live)
queue_args.insert("x-message-ttl".into(), AMQPValue::LongUInt(3600000));

// Dead letter exchange
queue_args.insert("x-dead-letter-exchange".into(), AMQPValue::LongString("dlx".into()));

// Queue length limit
queue_args.insert("x-max-length".into(), AMQPValue::LongUInt(10000));

// Maximum memory usage
queue_args.insert("x-max-length-bytes".into(), AMQPValue::LongLongUint(100000000));

config.queue_arguments = queue_args;
```

## 📊 Monitoring & Health Checks

### Health Monitoring

```rust
// Basic health check
let health = queue.health_check().await?;
println!("Health Status: {:?}", health.status);
println!("Queue Size: {}", health.queue_size);
println!("Error Rate: {:.2}%", health.error_rate * 100.0);

// Detailed statistics
let stats = queue.get_stats().await?;
println!("Total Messages: {}", stats.total_messages);
println!("Processed: {}", stats.total_processed);
println!("Success Rate: {:.2}%", stats.success_rate() * 100.0);

// Queue size information
let size = queue.size().await?;
let is_empty = queue.is_empty().await?;
```

### Configuration Validation

```rust
// Validate configuration
let is_valid = queue.validate_config().await?;
println!("Configuration valid: {}", is_valid);

// Test actual connection
let connection_ok = queue.test_connection().await?;
println!("Connection test: {}", if connection_ok { "✅ PASS" } else { "❌ FAIL" });
```

## 🔄 Message Patterns

### Producer-Consumer Pattern

```rust
// Producer (sends messages)
async fn produce_messages() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("events", "events_exchange");
    let producer = RabbitMQQueueSimple::new(config).await?;

    for i in 1..=100 {
        let event = serde_json::json!({
            "id": format!("event_{}", i),
            "type": "user_activity",
            "timestamp": chrono::Utc::now()
        });

        let message = QueueMessage::builder()
            .payload(event)
            .expect("Failed to serialize")
            .priority(QueuePriority::Normal)
            .routing_key("events.activity")
            .build();

        let id = producer.enqueue(message).await?;
        println!("Produced event: {}", id);
    }

    Ok(())
}

// Consumer (receives messages)
async fn consume_messages() -> Result<(), Box<dyn std::error::Error>> {
    let config = dev_config("events", "events_exchange");
    let consumer = RabbitMQQueueSimple::new(config).await?;

    loop {
        match consumer.dequeue().await {
            Ok(Some(message)) => {
                println!("Consumed: {}", message.payload);

                // Process message...

                // Acknowledge successful processing
                consumer.ack(&message.id).await?;
            }
            Ok(None) => {
                // No messages available
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                eprintln!("Error consuming message: {}", e);
                break;
            }
        }
    }
}
```

### Publish-Subscribe Pattern

```rust
// Publisher (broadcasts to multiple consumers)
let publisher_config = RabbitMQConfig {
    exchange_type: ExchangeType::Fanout,
    // ... other fields
};

let publisher = RabbitMQQueueSimple::new(publisher_config).await?;

let broadcast_message = QueueMessage::builder()
    .payload(serde_json::json!({
        "alert": "SYSTEM_ANNOUNCEMENT",
        "message": "Scheduled maintenance at 2AM UTC"
    }))
    .expect("Failed to serialize")
    .build();

let message_id = publisher.enqueue(broadcast_message).await?;
println!("Broadcast sent: {}", message_id);

// Multiple subscribers receive the same message
let subscriber_configs = vec![
    dev_config("logging_service", "announcements"),
    dev_config("monitoring_service", "announcements"),
    dev_config("alerting_service", "announcements"),
];

for config in subscriber_configs {
    let subscriber = RabbitMQQueueSimple::new(config).await?;
    // Each subscriber would process the same message
}
```

### Request-Reply Pattern

```rust
// RPC-style request-response
async fn send_rpc_request() -> Result<(), Box<dyn std::error::Error>> {
    let request_queue = RabbitMQQueueSimple::new(
        dev_config("rpc.requests", "rpc.direct")
    ).await?;

    let reply_queue = RabbitMQQueueSimple::new(
        RabbitMQConfig {
            queue_name: format!("rpc.reply_{}", uuid::Uuid::new_v4()),
            exchange_name: "rpc.direct",
            exchange_type: ExchangeType::Direct,
            routing_key: Some("rpc.reply"),
            ..Default::default()
        }
    ).await?;

    // Send request
    let request = QueueMessage::builder()
        .payload(serde_json::json!({
            "method": "get_user",
            "params": { "user_id": 12345 },
            "reply_to": reply_queue.config.queue_name
        }))
        .expect("Failed to serialize")
        .routing_key("rpc.request")
        .build();

    let request_id = request_queue.enqueue(request).await?;

    // Wait for reply with timeout
    let timeout = Duration::from_secs(5);
    let start_time = std::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            return Err("Request timeout".into());
        }

        match reply_queue.dequeue().await {
            Ok(Some(reply)) => {
                println!("Reply received: {}", reply.payload);
                reply_queue.ack(&reply.id).await?;
                break;
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}
```

## 🔐 Security Best Practices

### TLS/SSL Configuration

```rust
// Secure connection with TLS
let secure_config = RabbitMQConfig {
    connection_url: "amqps://username:password@rabbitmq.example.com:5671/%2f".to_string(),
    // ... other fields
};

let secure_queue = RabbitMQQueueSimple::new(secure_config).await?;
```

### Authentication

```rust
// Authentication in connection URL
let auth_config = RabbitMQConfig {
    connection_url: "amqp://username:password@rabbitmq.example.com:5671/%2f".to_string(),
    // ... other fields
};
```

### Access Control

```rust
// Create queues with limited permissions
let limited_config = RabbitMQConfig {
    queue_name: "user_123_inbox".to_string(),
    exchange_name: "user.direct".to_string(),
    exchange_type: ExchangeType::Direct,
    routing_key: Some("user.123.messages".to_string()),
    // ... other fields
};
```

## ⚡ Performance Tuning

### Publisher Confirms

```rust
let config = RabbitMQConfig {
    // ... other fields
    publisher_confirms: true,  // Wait for broker confirmation
};

// Message will not be considered delivered until confirmed
let message_id = queue.enqueue(message).await?;
// Now the message is confirmed as delivered
```

### Quality of Service (QoS)

```rust
let config = RabbitMQConfig {
    // ... other fields
    qos_config: QosConfig {
        prefetch_count: 10,    // Number of unacknowledged messages
        prefetch_size: 0,     // Unlimited message size
        global: false,        // Per-channel QoS
    },
};
```

### Connection Pooling

```rust
// In the implementation, connections are reused
// High-throughput applications benefit from connection reuse
let queue = RabbitMQQueueSimple::new(config).await?;

// Multiple operations share the same underlying connection
for i in 1..=1000 {
    let message = create_message(i);
    queue.enqueue(message).await?;
}
```

## 🚨 Error Handling

### Connection Failures

```rust
match RabbitMQQueueSimple::new(config).await {
    Ok(queue) => {
        println!("Successfully connected to RabbitMQ");
        // Use the queue...
    }
    Err(e) => {
        eprintln!("Failed to connect to RabbitMQ: {}", e);
        // Implement retry logic or fallback
        return Err(e.into());
    }
}
```

### Message Processing Errors

```rust
match queue.dequeue().await {
    Ok(Some(message)) => {
        match process_message(&message).await {
            Ok(()) => {
                // Success - acknowledge the message
                queue.ack(&message.id).await?;
            }
            Err(e) => {
                // Processing failed - negative acknowledge
                queue.nack(&message.id, Some(30)).await?;
            }
        }
    }
    Ok(None) => {
        // No messages available
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(e) => {
        eprintln!("Error dequeuing: {}", e);
    }
}
```

## 🧪 Testing

### Running Examples

```bash
# Basic RabbitMQ example
cargo run --example rabbitmq_examples

# Webhook processing system
cargo run --example rabbitmq_webhook_processor

# Real-time chat system
cargo run --example rabbitmq_realtime_chat
```

### Unit Tests

```rust
#[tokio::test]
async fn test_rabbitmq_basic_operations() {
    let config = RabbitMQConfig::default();
    let queue = RabbitMQQueueSimple::new(config).await.unwrap();

    // Test enqueue
    let message = QueueMessage::builder()
        .payload("test message")
        .expect("Failed to serialize")
        .build();

    let message_id = queue.enqueue(message).await.unwrap();
    assert!(!message_id.is_empty());

    // Test dequeue
    let dequeued = queue.dequeue().await.unwrap();
    assert!(dequeued.is_none()); // Queue starts empty
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_rabbitmq_with_routing() {
    // Test different exchange types and routing patterns
    test_direct_exchange_routing().await?;
    test_fanout_exchange_broadcasting().await?;
    test_topic_exchange_patterns().await?;

    Ok(())
}
```

## 📚 Advanced Examples

### Microservices Communication

```rust
// Service A publishes events
pub async fn publish_user_event(event: UserEvent) -> Result<(), QueueError> {
    let config = RabbitMQConfig {
        exchange_name: "domain_events".to_string(),
        exchange_type: ExchangeType::Topic,
        routing_key: Some(format!("user.{}", event.event_type)),
        ..Default::default()
    };

    let queue = RabbitMQQueueSimple::new(config).await?;

    let message = QueueMessage::builder()
        .payload(serde_json::to_value(event)?)
        .expect("Failed to serialize event")
        .build();

    queue.enqueue(message).await
}

// Service B subscribes to events
pub async fn consume_user_events() -> Result<(), QueueError> {
    let config = RabbitMQConfig {
        exchange_name: "domain_events".to_string(),
        exchange_type: ExchangeExchange::Topic,
        routing_key: Some("user.*".to_string()),
        ..Default::default()
    };

    let queue = RabbitMQQueueSimple::new(config).await?;

    // Process events continuously
    while let Ok(Some(message)) = queue.dequeue().await {
        process_user_event(&message).await?;
        queue.ack(&message.id).await?;
    }

    Ok(())
}
```

### Dead Letter Exchange Setup

```rust
let dlq_config = RabbitMQConfig {
    queue_name: "messages.dead_letter".to_string(),
    exchange_name: "messages.dlq".to_string(),
    exchange_type: ExchangeType::Fanout,
    routing_key: None,
    ..Default::default()
};

let main_config = RabbitMQConfig {
    queue_name: "messages.main".to_string(),
    exchange_name: "messages.main".to_string(),
    exchange_type: ExchangeType::Direct,
    routing_key: Some("messages.process".to_string()),

    // Configure dead letter exchange
    dead_letter_exchange: Some("messages.dlq".to_string()),
    dead_letter_routing_key: Some("failed".to_string()),

    max_retries: 3,
    ..Default::default()
};
```

## 🔧 Configuration Validation

### URL Validation

```rust
use backbone_queue::utils::rabbitmq_simple::test_connection_url;

// Valid URLs
assert!(test_connection_url("amqp://guest:guest@localhost:5672/%2f").is_ok());
assert!(test_connection_url("amqps://user:pass@rabbitmq.example.com:5671/vhost").is_ok());

// Invalid URLs
assert!(test_connection_url("").is_err()); // Empty
assert!(test_connection_url("http://invalid.com").is_err()); // Wrong protocol
assert!(test_connection_url("ftp://server.com").is_err()); // Unsupported protocol
```

## 📋 Best Practices

### 1. Use Appropriate Exchange Types
- **Direct**: For point-to-point communication
- **Fanout**: For broadcasting to multiple consumers
- **Topic**: For flexible routing patterns
- **Headers**: For complex routing logic

### 2. Implement Proper Error Handling
- Always check return values from queue operations
- Implement retry logic for transient failures
- Use dead letter exchanges for failed messages

### 3. Monitor Performance
- Track message processing times
- Monitor queue sizes and growth rates
- Set up alerts for unusual patterns

### 4. Security Considerations
- Use TLS/SSL for production environments
- Implement proper authentication and authorization
- Validate all input data

### 5. Resource Management
- Set appropriate message TTLs
- Configure queue length limits
- Monitor memory and disk usage

## 🐛 Troubleshooting

### Common Issues

1. **Connection Failures**: Check RabbitMQ server status and network connectivity
2. **Message Loss**: Verify publisher confirms and acknowledgments
3. **Performance Issues**: Check queue sizes and prefetch settings
4. **Routing Problems**: Verify exchange types and routing keys

### Debug Mode

```rust
// Enable detailed logging
use tracing::{info, error, debug};

info!("Connecting to RabbitMQ at: {}", config.connection_url);
debug!("Publishing message with ID: {}", message.id);
error!("Failed to publish message: {}", error);
```

## 📚 Reference

- [RabbitMQ Official Documentation](https://www.rabbitmq.com/documentation.html)
- [AMQP Protocol Specification](https://www.amqp.org/specification.html)
- [Lapin Rust Library](https://github.com/Copierien/lapin)
- [Backbone Queue Module](../README.md)

---

This guide provides comprehensive coverage of RabbitMQ integration with the Backbone Queue module. For additional examples or specific use cases, check the `examples/` directory or the main documentation.
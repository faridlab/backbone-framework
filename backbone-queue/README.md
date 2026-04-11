# Backbone Queue Module

A high-performance, asynchronous queue implementation for the Backbone Framework with support for Redis, AWS SQS, and RabbitMQ backends.

## Overview

The Backbone Queue Module provides a unified, high-level interface for message queuing across multiple backend technologies. Built with async/await support and comprehensive error handling, it abstracts the complexity of different queue systems while maintaining performance and flexibility.

### Key Features

- ✅ **Multiple Backends**: Redis, AWS SQS, and RabbitMQ support
- ✅ **Priority Queues**: Four priority levels (Low, Normal, High, Critical)
- ✅ **Batch Operations**: Efficient bulk processing of messages
- ✅ **Dead Letter Queues**: Automatic handling of failed messages
- ✅ **Message Expiration**: Time-to-live (TTL) support
- ✅ **Visibility Timeouts**: Prevent concurrent processing
- ✅ **Message Attributes**: Custom metadata and headers support
- ✅ **FIFO Support**: First-in, first-out queue semantics (SQS)
- ✅ **Exchange Types**: RabbitMQ routing with Direct, Fanout, Topic, and Headers exchanges
- ✅ **Compression**: Automatic compression for large messages
- ✅ **Async/Await**: Full tokio async support
- ✅ **Health Monitoring**: Built-in health checks and statistics
- ✅ **Type Safety**: Strong typing with comprehensive error handling

### Architecture

```
┌─────────────────────────────────────┐
│        Application Layer            │
│  (Your Business Logic)              │
└─────────────┬───────────────────────┘
              │
┌─────────────▼───────────────────────┐
│     Backbone Queue Module           │
│  ┌─────────────────────────────┐    │
│  │      QueueService Trait     │    │
│  └─────────────┬───────────────┘    │
│                │                    │
│  ┌─────────────▼───────────────┐    │
│  │    Backend Implementations  │    │
│  │  ┌─────────┬─────────────┐  │    │
│  │  │ Redis   │    SQS      │  │    │
│  │  │         │             │  │    │
│  │  │ RabbitMQ│             │  │    │
│  │  └─────────┴─────────────┘  │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘
              │
┌─────────────▼───────────────────────┐
│      Infrastructure Layer           │
│  ┌─────────┬─────────────┐         │
│  │ Redis   │    SQS      │         │
│  │ Server  │  AWS        │         │
│  │         │             │         │
│  │ RabbitMQ│             │         │
│  │ Server  │             │         │
│  └─────────┴─────────────┘         │
└─────────────────────────────────────┘
```

### Supported Backends

| Backend | Use Case | Performance | Features |
|---------|----------|-------------|----------|
| **Redis** | High-speed in-memory queuing | Very High | Simple pub/sub, priority queues |
| **AWS SQS** | Cloud-native, scalable | High | Durability, at-least-once delivery |
| **RabbitMQ** | Complex routing, enterprise | High | AMQP, exchange routing, acknowledgments |

## Usage

### Quick Start Examples

#### Redis Queue

```rust
use backbone_queue::{QueueService, RedisQueue, QueueMessage, QueuePriority};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create Redis queue
    let queue = RedisQueue::builder()
        .url("redis://localhost:6379")
        .queue_name("my_queue")
        .build()
        .await?;

    // Send message
    let message = QueueMessage::builder()
        .payload("Hello, World!")
        .priority(QueuePriority::High)
        .build();

    let message_id = queue.enqueue(message).await?;
    println!("Enqueued message: {}", message_id);

    // Receive message
    if let Some(received_message) = queue.dequeue().await? {
        println!("Received: {}", received_message.payload);
        queue.ack(&received_message.id).await?;
    }

    Ok(())
}
```

#### AWS SQS Queue

```rust
use backbone_queue::{QueueService, SqsQueue, QueueMessage, QueuePriority};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create SQS queue
    let queue = SqsQueue::builder()
        .queue_url("https://sqs.us-east-1.amazonaws.com/123456789012/my-queue")
        .region("us-east-1")
        .build()
        .await?;

    // Send message
    let message = QueueMessage::builder()
        .payload("Hello, SQS!")
        .priority(QueuePriority::Normal)
        .build();

    let message_id = queue.enqueue(message).await?;
    println!("Enqueued message: {}", message_id);

    // Receive message
    if let Some(received_message) = queue.dequeue().await? {
        println!("Received: {}", received_message.payload);
        queue.ack(&received_message.id).await?;
    }

    Ok(())
}
```

#### RabbitMQ Queue

```rust
use backbone_queue::{
    QueueService, RabbitMQQueueSimple, QueueMessage, QueuePriority,
    rabbitmq_simple::{RabbitMQConfig, ExchangeType, utils::dev_config}
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create RabbitMQ queue with development configuration
    let config = dev_config("my_queue", "my_exchange");
    let queue = RabbitMQQueueSimple::new(config).await?;

    // Send message with routing key
    let message = QueueMessage::builder()
        .payload("Hello, RabbitMQ!")
        .priority(QueuePriority::Normal)
        .routing_key("events.user.created")
        .build();

    let message_id = queue.enqueue(message).await?;
    println!("Enqueued message: {}", message_id);

    // Receive message
    if let Some(received_message) = queue.dequeue().await? {
        println!("Received: {}", received_message.payload);
        queue.ack(&received_message.id).await?;
    }

    Ok(())
}
```

### Backend Selection Guide

Choose the right backend based on your requirements:

#### Redis
- **Best for**: High-speed in-memory queuing, simple use cases
- **Pros**: Fastest performance, simple setup, low latency
- **Cons**: Limited durability, memory-based, limited routing

```rust
// Use Redis when you need:
// - Maximum speed
// - Simple pub/sub patterns
// - Temporary queues
// - Low-latency processing
```

#### AWS SQS
- **Best for**: Cloud-native applications, durability at scale
- **Pros**: Highly durable, managed service, auto-scaling
- **Cons**: Higher latency, AWS-specific, limited routing

```rust
// Use SQS when you need:
// - Cloud-native solution
// - High durability guarantees
// - Automatic scaling
// - Serverless architecture
```

#### RabbitMQ
- **Best for**: Complex routing, enterprise messaging, microservices
- **Pros**: Rich routing, AMQP protocol, acknowledgments, exchanges
- **Cons**: More complex, requires infrastructure management

```rust
// Use RabbitMQ when you need:
// - Complex message routing
// - Exchange patterns (direct, fanout, topic)
// - Publisher confirms
// - Microservices communication
```

### Advanced Configuration

#### Redis Configuration

```rust
use backbone_queue::redis::RedisQueueBuilder;

let queue = RedisQueueBuilder::new()
    .url("redis://localhost:6379")
    .queue_name("production_queue")
    .key_prefix("myapp:queue")
    .pool_size(10)
    .build()
    .await?;
```

#### SQS Configuration

```rust
use backbone_queue::sqs::SqsQueueBuilder;

let queue = SqsQueueBuilder::new()
    .queue_url("https://sqs.us-east-1.amazonaws.com/123456789012/my-queue")
    .region("us-east-1")
    .credentials("your-access-key", "your-secret-key")
    .visibility_timeout(30)
    .wait_time_seconds(5)
    .build()
    .await?;
```

#### RabbitMQ Configuration

```rust
use backbone_queue::rabbitmq_simple::{RabbitMQConfig, ExchangeType, utils::dev_config, utils::prod_config};

// Development configuration
let dev_config = dev_config("my_queue", "my_exchange");

// Production configuration with TLS
let prod_config = prod_config(
    "amqps://user:password@rabbitmq.example.com:5671/%2f",
    "production_queue",
    "production_exchange",
    ExchangeType::Topic
);

// Manual configuration
let custom_config = RabbitMQConfig {
    connection_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
    queue_name: "custom_queue".to_string(),
    exchange_name: "custom_exchange".to_string(),
    exchange_type: ExchangeType::Direct,
    routing_key: Some("custom.routing".to_string()),
};

let queue = RabbitMQQueueSimple::new(custom_config).await?;
```

### RabbitMQ Exchange Types

```rust
use backbone_queue::rabbitmq_simple::{RabbitMQConfig, ExchangeType};

// Direct Exchange - Exact routing key matches
let direct_config = RabbitMQConfig {
    exchange_type: ExchangeType::Direct,
    routing_key: Some("users.created".to_string()),
    // ... other fields
};

// Fanout Exchange - Broadcast to all bound queues
let fanout_config = RabbitMQConfig {
    exchange_type: ExchangeType::Fanout,
    routing_key: None, // Ignored for fanout
    // ... other fields
};

// Topic Exchange - Pattern-based routing with wildcards
let topic_config = RabbitMQConfig {
    exchange_type: ExchangeType::Topic,
    routing_key: Some("logs.*.error".to_string()),
    // ... other fields
};
```

### Message Creation

#### Basic Message

```rust
use backbone_queue::{QueueMessage, QueuePriority};

let message = QueueMessage::builder()
    .payload("Simple message")
    .priority(QueuePriority::Normal)
    .build();
```

#### Message with Headers

```rust
use std::collections::HashMap;
use backbone_queue::{QueueMessage, QueuePriority};

let mut headers = HashMap::new();
headers.insert("source".to_string(), serde_json::Value::String("api".to_string()));
headers.insert("user_id".to_string(), serde_json::Value::Number(12345.into()));

let message = QueueMessage::builder()
    .payload("Message with metadata")
    .priority(QueuePriority::High)
    .headers(headers)
    .build();
```

#### RabbitMQ Message with Routing

```rust
use backbone_queue::{QueueMessage, QueuePriority};

let message = QueueMessage::builder()
    .payload("User created event")
    .priority(QueuePriority::Normal)
    .routing_key("events.user.created")
    .header("event_type", "user_created")
    .header("user_id", 12345)
    .build();
```

#### Message with Expiration

```rust
use backbone_queue::QueueMessage;

let message = QueueMessage::builder()
    .payload("Expires in 1 hour")
    .visibility_timeout(3600) // seconds
    .build();
```

#### FIFO Message (SQS)

```rust
use backbone_queue::QueueMessage;

let message = QueueMessage::builder()
    .payload("FIFO message")
    .message_group_id("group-123")
    .message_deduplication_id("dedup-456")
    .build();
```

#### JSON Payload

```rust
use backbone_queue::QueueMessage;
use serde_json::json;

let message = QueueMessage::builder()
    .payload(json!({
        "event": "user_registered",
        "user_id": 12345,
        "email": "user@example.com",
        "timestamp": chrono::Utc::now()
    }))
    .expect("Failed to serialize JSON")
    .routing_key("events.user.registered")
    .priority(QueuePriority::High)
    .build();
```

## Priority Levels

Messages are processed in priority order:

```rust
use backbone_queue::QueuePriority;

// Priority levels (higher number = higher priority)
QueuePriority::Low       // 1
QueuePriority::Normal    // 5 (default)
QueuePriority::High      // 10
QueuePriority::Critical  // 20
```

## Queue Operations

### Basic Operations

```rust
// Enqueue single message
let message_id = queue.enqueue(message).await?;

// Dequeue single message
if let Some(message) = queue.dequeue().await? {
    // Process message
    queue.ack(&message.id).await?; // Mark as processed
}

// Get message by ID
if let Some(message) = queue.get_message(&message_id).await? {
    println!("Found message: {}", message.payload);
}
```

### Batch Operations

```rust
// Enqueue multiple messages
let messages = vec![
    QueueMessage::builder().payload("Message 1").build(),
    QueueMessage::builder().payload("Message 2").build(),
    QueueMessage::builder().payload("Message 3").build(),
];

let message_ids = queue.enqueue_batch(messages).await?;
println!("Enqueued {} messages", message_ids.len());

// Dequeue batch
let batch_result = queue.dequeue_batch(10).await?;
println!("Received {} messages", batch_result.messages.len());

// Batch acknowledge
let message_ids: Vec<String> = batch_result.messages
    .into_iter()
    .map(|m| m.id)
    .collect();

let ack_count = queue.ack_batch(message_ids).await?;
println!("Acknowledged {} messages", ack_count);
```

### Queue Management

```rust
// Get queue statistics
let stats = queue.get_stats().await?;
println!("Visible messages: {}", stats.visible_messages);
println!("Invisible messages: {}", stats.invisible_messages);
println!("Total processed: {}", stats.total_processed);

// Get queue size
let size = queue.size().await?;
println!("Queue size: {}", size);

// Check if empty
if queue.is_empty().await? {
    println!("Queue is empty");
}

// Purge all messages
let purged_count = queue.purge().await?;
println!("Purged {} messages", purged_count);
```

### Health Monitoring

```rust
// Health check
let health = queue.health_check().await?;
println!("Health status: {:?}", health.status);
println!("Queue size: {}", health.queue_size);
println!("Error rate: {:.2}%", health.error_rate * 100.0);

// Test connection
let is_connected = queue.test_connection().await?;
println!("Connected: {}", is_connected);
```

## Error Handling

```rust
use backbone_queue::QueueError;

match queue.enqueue(message).await {
    Ok(message_id) => println!("Enqueued: {}", message_id),
    Err(QueueError::MessageTooLarge { size, max }) => {
        eprintln!("Message too large: {} bytes (max: {})", size, max);
    }
    Err(QueueError::RedisConnection(msg)) => {
        eprintln!("Redis connection failed: {}", msg);
    }
    Err(QueueError::SqsError(msg)) => {
        eprintln!("SQS error: {}", msg);
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Advanced Usage

### Dead Letter Queue Handling

```rust
use backbone_queue::QueueMessage;

let message = QueueMessage::builder()
    .payload("Important message")
    .max_receive_count(3) // Send to DLQ after 3 failed attempts
    .build();

queue.enqueue(message).await?;

// Process message
if let Some(message) = queue.dequeue().await? {
    match process_message(&message).await {
        Ok(_) => {
            queue.ack(&message.id).await?;
        }
        Err(_) => {
            // Failed processing, message will be retried
            queue.nack(&message.id, Some(60)).await?; // Retry after 60 seconds
        }
    }
}

// Check dead letter queue
let stats = queue.get_stats().await?;
if stats.dead_letter_messages > 0 {
    println!("{} messages in dead letter queue", stats.dead_letter_messages);
}
```

### Message Compression

```rust
use backbone_queue::QueueMessage;

let large_payload = "x".repeat(1_000_000); // 1MB payload

let message = QueueMessage::builder()
    .payload(large_payload)
    .compress(true) // Enable compression
    .build();

let message_id = queue.enqueue(message).await?;
```

### Visibility Timeouts

```rust
use backbone_queue::QueueMessage;

let message = QueueMessage::builder()
    .payload("Long-running task")
    .visibility_timeout(300) // 5 minutes
    .build();

queue.enqueue(message).await?;

if let Some(message) = queue.dequeue().await? {
    // Message is invisible for 5 minutes

    // If processing fails early, make visible again
    queue.nack(&message.id, Some(0)).await?;
}
```

## Monitoring and Metrics

### Queue Statistics

```rust
let stats = queue.get_stats().await?;

println!("=== Queue Statistics ===");
println!("Visible Messages: {}", stats.visible_messages);
println!("Invisible Messages: {}", stats.invisible_messages);
println!("Dead Letter Messages: {}", stats.dead_letter_messages);
println!("Total Messages: {}", stats.total_messages);
println!("Total Processed: {}", stats.total_processed);
println!("Total Failed: {}", stats.total_failed);
```

### Health Check

```rust
let health = queue.health_check().await?;

println!("=== Health Status ===");
println!("Status: {:?}", health.status);
println!("Queue Size: {}", health.queue_size);
println!("Avg Processing Time: {}ms",
           health.avg_processing_time_ms.unwrap_or(0));
println!("Messages/Second: {:.2}",
           health.messages_per_second.unwrap_or(0.0));
println!("Error Rate: {:.2}%", health.error_rate * 100.0);
println!("Last Activity: {:?}", health.last_activity);
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific backend tests
cargo test redis
cargo test sqs

# Run integration tests (requires Redis/SQS setup)
cargo test --test integration_tests -- --ignored

# Run performance tests
cargo test performance -- --ignored
```

### Test Configuration

For integration tests, set these environment variables:

```bash
export REDIS_TEST_URL="redis://localhost:6379"
export AWS_REGION="us-east-1"
export SQS_QUEUE_URL="https://sqs.us-east-1.amazonaws.com/123456789012/test-queue"
```

## Performance

### Redis Performance

- **Enqueue**: ~10,000 messages/second
- **Dequeue**: ~8,000 messages/second
- **Batch Operations**: ~50,000 messages/second

### SQS Performance

- **Enqueue**: ~1,000 messages/second
- **Dequeue**: ~800 messages/second
- **Batch Operations**: ~5,000 messages/second

*Performance varies based on message size, network latency, and AWS region.*

## Best Practices

### Message Design

```rust
// ✅ Good: Small, focused messages
let message = QueueMessage::builder()
    .payload(json!({
        "user_id": 123,
        "action": "send_email",
        "email": "user@example.com"
    }))
    .priority(QueuePriority::Normal)
    .build();

// ❌ Avoid: Large payloads in message
let message = QueueMessage::builder()
    .payload(large_binary_data) // Use S3 instead
    .build();
```

### Error Handling

```rust
// ✅ Good: Proper error handling and retries
if let Some(message) = queue.dequeue().await? {
    match process_message(&message).await {
        Ok(_) => queue.ack(&message.id).await?,
        Err(e) if is_retryable(e) => {
            queue.nack(&message.id, Some(calculate_backoff())).await?
        }
        Err(e) => {
            log::error!("Non-retryable error: {}", e);
            queue.ack(&message.id).await? // Acknowledge to prevent retries
        }
    }
}
```

### Resource Management

```rust
// ✅ Good: Use connection pooling
let queue = RedisQueueBuilder::new()
    .url("redis://localhost:6379")
    .pool_size(10) // Match to your concurrency needs
    .build()
    .await?;

// ✅ Good: Batch operations when possible
let messages = create_messages();
queue.enqueue_batch(messages).await?;
```

### Monitoring

```rust
// ✅ Good: Regular health checks
let health = queue.health_check().await?;
match health.status {
    QueueHealth::Healthy => /* Normal operation */,
    QueueHealth::Degraded => /* Log warning, continue */,
    QueueHealth::Unhealthy => /* Alert, potentially fail fast */,
}
```

## Troubleshooting

### Common Issues

#### Redis Connection Errors

```rust
// Check connection
if !queue.test_connection().await? {
    eprintln!("Cannot connect to Redis");
}
```

#### SQS Permission Errors

Ensure your AWS credentials have these permissions:
- `sqs:SendMessage`
- `sqs:ReceiveMessage`
- `sqs:DeleteMessage`
- `sqs:GetQueueAttributes`

#### Large Message Errors

```rust
// Check message size
let message = QueueMessage::builder().payload(data).build();
if let Ok(size) = message.size_bytes() {
    if size > 256_000 { // SQS limit
        eprintln!("Message too large: {} bytes", size);
    }
}
```

## Technical Details

### Core Architecture

The Backbone Queue Module is built around a unified trait-based architecture that provides consistent behavior across all backends:

```rust
use backbone_queue::traits::QueueService;

// All backends implement the same QueueService trait
async fn process_messages<T: QueueService>(queue: &T) -> Result<(), QueueError> {
    // Works with Redis, SQS, and RabbitMQ!
    if let Some(message) = queue.dequeue().await? {
        process_message(&message).await?;
        queue.ack(&message.id).await?;
    }
    Ok(())
}
```

#### QueueService Trait

```rust
#[async_trait]
pub trait QueueService: Send + Sync {
    // Core operations
    async fn enqueue(&self, message: QueueMessage) -> QueueResult<String>;
    async fn dequeue(&self) -> QueueResult<Option<QueueMessage>>;
    async fn ack(&self, message_id: &str) -> QueueResult<bool>;
    async fn nack(&self, message_id: &str, delay_seconds: Option<u64>) -> QueueResult<bool>;

    // Batch operations
    async fn enqueue_batch(&self, messages: Vec<QueueMessage>) -> QueueResult<Vec<String>>;
    async fn dequeue_batch(&self, max_messages: usize) -> QueueResult<BatchReceiveResult>;

    // Queue management
    async fn size(&self) -> QueueResult<u64>;
    async fn purge(&self) -> QueueResult<u64>;
    async fn get_stats(&self) -> QueueResult<QueueStats>;

    // Health and configuration
    async fn health_check(&self) -> QueueResult<QueueHealthCheck>;
    async fn validate_config(&self) -> QueueResult<bool>;
    fn backend_type(&self) -> QueueBackend;
}
```

### Message Flow

```mermaid
graph TD
    A[Producer creates QueueMessage] --> B[QueueService.enqueue]
    B --> C[Backend Processing]
    C --> D[Queue Storage]
    D --> E[Consumer calls QueueService.dequeue]
    E --> F[Backend retrieves message]
    F --> G[Message processing]
    G --> H{Processing success?}
    H -->|Yes| I[QueueService.ack]
    H -->|No| J[QueueService.nack]
    I --> K[Message removed]
    J --> L[Message requeued/DLQ]
```

### Backend Implementations

#### Redis Backend
- **Protocol**: Redis RESP protocol
- **Storage**: Sorted sets for priority queues
- **Connection**: Connection pooling with bb8
- **Durability**: In-memory with optional persistence
- **Scalability**: Cluster support with Redis Cluster

```rust
// Redis data structure
ZADD queue:pending <priority_score> <message_id>
HSET queue:messages <message_id> <serialized_message>
```

#### AWS SQS Backend
- **Protocol**: AWS SDK HTTP API
- **Storage**: Managed AWS infrastructure
- **Durability**: 99.999999999% durability
- **Scalability**: Auto-scaling to any throughput
- **Compliance**: SOC, ISO, HIPAA compliant

```rust
// SQS message flow
SendMessage API -> SQS Queue -> ReceiveMessage API -> DeleteMessage API
```

#### RabbitMQ Backend
- **Protocol**: AMQP 0.9.1
- **Storage**: Managed RabbitMQ server
- **Routing**: Exchanges and binding patterns
- **Reliability**: Publisher confirms and consumer acknowledgments
- **Cluster**: High availability with clustering

```rust
// AMQP flow
Publisher -> Exchange -> Binding -> Queue -> Consumer
```

### Error Handling Strategy

```rust
use backbone_queue::QueueError;

pub enum QueueError {
    // Network and connectivity
    NetworkError(String),
    ConnectionLost(String),

    // Configuration issues
    ConfigError(String),
    InvalidConfiguration(String),

    // Message issues
    MessageTooLarge { size: usize, max: usize },
    Serialization(String),

    // Backend-specific errors
    RedisConnection(String),
    SqsError(String),
    RabbitMQError(String),

    // Rate limiting and quotas
    RateLimitExceeded,
    QuotaExceeded(String),
}
```

### Performance Characteristics

#### Throughput Benchmarks
| Backend | Enqueue | Dequeue | Batch Ops |
|---------|---------|---------|-----------|
| Redis | 10,000 msg/s | 8,000 msg/s | 50,000 msg/s |
| SQS | 1,000 msg/s | 800 msg/s | 5,000 msg/s |
| RabbitMQ | 8,000 msg/s | 6,000 msg/s | 20,000 msg/s |

#### Latency (P99)
| Backend | Enqueue | Dequeue |
|---------|---------|---------|
| Redis | <1ms | <1ms |
| SQS | 10-50ms | 10-50ms |
| RabbitMQ | 2-5ms | 2-5ms |

### Configuration Reference

#### Redis Configuration

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `url` | `String` | `redis://localhost:6379` | Redis connection URL |
| `queue_name` | `String` | `default_queue` | Queue name |
| `key_prefix` | `String` | `backbone:queue` | Redis key prefix |
| `pool_size` | `u32` | `10` | Connection pool size |

#### SQS Configuration

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `queue_url` | `String` | Required | SQS queue URL |
| `region` | `String` | `us-east-1` | AWS region |
| `visibility_timeout` | `i32` | `30` | Message visibility timeout (seconds) |
| `wait_time_seconds` | `i32` | `5` | Long polling wait time |
| `max_number_of_messages` | `i32` | `10` | Max messages per receive |

#### RabbitMQ Configuration

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `connection_url` | `String` | `amqp://guest:guest@localhost:5672/%2f` | AMQP connection URL |
| `queue_name` | `String` | `default_queue` | Queue name |
| `exchange_name` | `String` | `default_exchange` | Exchange name |
| `exchange_type` | `ExchangeType` | `Direct` | Exchange type (Direct, Fanout, Topic) |
| `routing_key` | `Option<String>` | `None` | Default routing key |

### Testing

#### Unit Tests
```bash
# Run all tests
cargo test

# Run backend-specific tests
cargo test redis
cargo test sqs
cargo test rabbitmq
```

#### Integration Tests
```bash
# Run integration tests (requires infrastructure)
cargo test --test integration_tests -- --ignored

# Run performance tests
cargo test performance -- --ignored
```

#### Test Coverage
- ✅ All QueueService trait methods
- ✅ Configuration validation
- ✅ Error handling scenarios
- ✅ Priority processing
- ✅ Batch operations
- ✅ Health checks
- ✅ Connection failure scenarios

### Dependencies

```toml
[dependencies]
# Core
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"

# Backends
redis = { version = "0.24", features = ["tokio-comp"] }
aws-config = { version = "1.0" }
aws-sdk-sqs = "1.0"
lapin = "2.5"  # RabbitMQ
futures-lite = "2.0"

# Utilities
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
thiserror = "1.0"
```

### Real-World Examples

#### Microservices Communication
```rust
// User service publishes events
let event_queue = RabbitMQQueueSimple::new(dev_config(
    "user_events", "domain_events"
)).await?;

let user_created = QueueMessage::builder()
    .payload(json!({
        "event": "user_created",
        "user_id": 12345,
        "email": "user@example.com"
    }))
    .expect("Failed to serialize")
    .routing_key("events.user.created")
    .priority(QueuePriority::High)
    .build();

event_queue.enqueue(user_created).await?;
```

#### Webhook Processing
```rust
// Process incoming webhooks asynchronously
let webhook_queue = RedisQueue::builder()
    .url("redis://localhost:6379")
    .queue_name("webhooks")
    .build()
    .await?;

// Enqueue webhook for processing
let webhook = QueueMessage::builder()
    .payload(json!({
        "url": "https://api.example.com/webhook",
        "data": webhook_data,
        "attempt": 1
    }))
    .expect("Failed to serialize")
    .priority(QueuePriority::Normal)
    .build();

webhook_queue.enqueue(webhook).await?;
```

#### Background Job Processing
```rust
// Queue background tasks
let job_queue = SqsQueue::builder()
    .queue_url("https://sqs.us-east-1.amazonaws.com/account/background-jobs")
    .build()
    .await?;

let job = QueueMessage::builder()
    .payload(json!({
        "job_type": "send_welcome_email",
        "user_id": 12345,
        "scheduled_at": chrono::Utc::now() + chrono::Duration::hours(1)
    }))
    .expect("Failed to serialize")
    .build();

job_queue.enqueue(job).await?;
```

## Migration Guide

### Between Backends

```rust
use backbone_queue::{QueueService, QueueBackend};

// Factory function for backend creation
async fn create_queue(backend: QueueBackend, name: &str) -> Box<dyn QueueService> {
    match backend {
        QueueBackend::Redis => Box::new(
            RedisQueue::builder()
                .url("redis://localhost:6379")
                .queue_name(name)
                .build()
                .await
                .unwrap()
        ),
        QueueBackend::Sqs => Box::new(
            SqsQueue::builder()
                .queue_url(&format!("https://sqs.us-east-1.amazonaws.com/account/{}", name))
                .build()
                .await
                .unwrap()
        ),
        QueueBackend::RabbitMQ => Box::new(
            RabbitMQQueueSimple::new(dev_config(name, name))
                .await
                .unwrap()
        ),
    }
}

// Use with any backend
let queue = create_queue(QueueBackend::Redis, "notifications").await;
process_messages(&*queue).await?;
```

## License

This project is part of the Backbone Framework and follows the same licensing terms.

## Contributing

Please read the contribution guidelines before submitting pull requests.

## Support

For questions and support:
- Create an issue on GitHub
- Check the documentation
- Review the test examples
- See [RabbitMQ Integration Guide](docs/RABBITMQ_GUIDE.md) for detailed RabbitMQ setup
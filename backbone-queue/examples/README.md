# Backbone Queue Examples

This directory contains comprehensive examples demonstrating various aspects of the Backbone Queue module.

## Prerequisites

### Common Requirements

- **Rust 1.70+**
- **Redis Server** (for Redis examples)
- **AWS Account & Credentials** (for SQS examples)
- **Tokio runtime**

### Redis Setup

```bash
# Install Redis (macOS)
brew install redis

# Start Redis server
redis-server

# Or use Docker
docker run -d -p 6379:6379 redis:7-alpine
```

### AWS SQS Setup

1. **Create AWS SQS Queue:**
   ```bash
   aws sqs create-queue --queue-name test-queue --region us-east-1
   aws sqs create-queue --queue-name test-queue.fifo --region us-east-1 --attributes FifoQueue=true
   ```

2. **Configure AWS Credentials:**
   ```bash
   export AWS_ACCESS_KEY_ID=your_access_key
   export AWS_SECRET_ACCESS_KEY=your_secret_key
   export AWS_DEFAULT_REGION=us-east-1
   ```

   Or use AWS CLI: `aws configure`

## Running Examples

### Basic Examples

#### 1. Basic Redis Queue
```bash
cargo run --example basic_redis_queue
```

**What it demonstrates:**
- Redis queue connection and configuration
- Basic enqueue/dequeue operations
- Priority-based message ordering
- Batch operations
- Message attributes and metadata
- Health monitoring and statistics

#### 2. SQS Queue Examples
```bash
export SQS_QUEUE_URL="https://sqs.us-east-1.amazonaws.com/123456789012/test-queue"
cargo run --example sqs_queue_examples
```

**What it demonstrates:**
- SQS queue connection and configuration
- Priority message handling with SQS attributes
- Custom message attributes
- Batch operations
- FIFO queue setup (if configured)
- Error handling and AWS service limitations

### Advanced Examples

#### 3. Worker Pool
```bash
cargo run --example worker_pool
```

**What it demonstrates:**
- Concurrent message processing
- Worker pool with configurable concurrency
- Message retry logic and error handling
- Graceful shutdown handling
- Performance statistics and monitoring
- Real-time processing feedback

#### 4. Monitoring Dashboard
```bash
cargo run --example monitoring_dashboard
```

**What it demonstrates:**
- Real-time queue monitoring
- Metrics collection and time-series data
- Alert system with configurable thresholds
- Health checks and status reporting
- Text-based graphs and visualizations
- Long-running monitoring with updates

## Environment Variables

### Redis Examples
```bash
export REDIS_TEST_URL="redis://localhost:6379"
```

### SQS Examples
```bash
export SQS_QUEUE_URL="https://sqs.us-east-1.amazonaws.com/123456789012/test-queue"
export AWS_REGION="us-east-1"
export AWS_ACCESS_KEY_ID="your_access_key"
export AWS_SECRET_ACCESS_KEY="your_secret_key"
```

## Example Output

### Basic Redis Queue
```
🚀 Basic Redis Queue Example
============================
📡 Connecting to Redis...
✅ Connected to Redis successfully
🧹 Clearing existing messages...
✅ Queue cleared

📬 Example 1: Basic Message Operations
-------------------------------------
📤 Enqueuing message: Hello, Redis Queue!
✅ Message enqueued with ID: 550e8400-e29b-41d4-a716-446655440000
📊 Queue size: 1
📥 Dequeuing message...
✅ Received message: Hello, Redis Queue!
🔑 Message ID: 550e8400-e29b-41d4-a716-446655440000
⚖️ Priority: Normal
📅 Enqueued at: 2024-01-15T10:30:00Z
✅ Message acknowledged
```

### Worker Pool
```
🚀 Starting queue worker pool
📊 Configuration:
  - Max concurrent tasks: 5
  - Poll interval: 50ms
  - Max retries: 2
  - Visibility timeout: 30 seconds
📤 Populating queue with test messages...
  ✅ Enqueued: Send welcome email notification
  ✅ Enqueued: Generate daily report
  ✅ Enqueued: Process user cleanup task
...
📚 Worker Statistics:
  - Messages processed: 8
  - Messages failed: 1
  - Messages retried: 2
  - Avg processing time: 250ms
  - Success rate: 88.9%
```

### Monitoring Dashboard
```
🖥️  Queue Monitoring Dashboard
=============================
🏥 Health Status: 🟢 Healthy

📊 Queue Metrics:
  📦 Total messages: 150
  ✅ Processed: 145
  ❌ Failed: 5
  💀 Dead letter: 0
  📈 Success rate: 96.7%

⚡ Performance:
  🚀 Messages/sec: 12.5
  ⚠️  Error rate: 3.3%
  ⏱️  Avg processing time: 180.2ms

📈 Queue Size (last 20 points):
███████████████│
─────────────────
```

## Customization

### Modifying Worker Pool

```rust
let worker_config = WorkerConfig {
    max_concurrent_tasks: 20,        // Increase concurrency
    poll_interval: Duration::from_millis(10),  // Faster polling
    max_retries: 5,                  // More retries
    visibility_timeout: 60,          // Longer visibility
    shutdown_timeout: Duration::from_secs(60), // Graceful shutdown
};
```

### Custom Alert Thresholds

```rust
let alert_thresholds = AlertThresholds {
    max_queue_size: 10000,           // Higher threshold
    max_error_rate: 0.05,            // 5% error rate
    max_processing_time: 3000.0,     // 3 seconds
    min_messages_per_second: 10.0,   // Minimum throughput
};
```

## Troubleshooting

### Connection Issues

**Redis Connection Failed:**
```bash
# Check if Redis is running
redis-cli ping

# Check Redis logs
tail -f /usr/local/var/log/redis.log
```

**SQS Connection Failed:**
```bash
# Verify AWS credentials
aws sts get-caller-identity

# Check SQS queue exists
aws sqs get-queue-attributes --queue-url $SQS_QUEUE_URL
```

### Permission Issues

**Required AWS IAM Permissions:**
```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "sqs:SendMessage",
                "sqs:ReceiveMessage",
                "sqs:DeleteMessage",
                "sqs:GetQueueAttributes"
            ],
            "Resource": "arn:aws:sqs:*:*:*"
        }
    ]
}
```

### Performance Issues

**Redis Performance:**
- Check connection pool size
- Monitor memory usage
- Consider Redis persistence settings

**SQS Performance:**
- Use batch operations for high throughput
- Consider long polling (`wait_time_seconds`)
- Monitor AWS service limits

## Best Practices

### Production Usage

1. **Connection Pooling**: Use appropriate pool sizes
2. **Error Handling**: Implement comprehensive error handling
3. **Monitoring**: Set up alerting and health checks
4. **Graceful Shutdown**: Handle signals and cleanup properly
5. **Configuration**: Externalize configuration
6. **Logging**: Use structured logging with correlation IDs

### Security

1. **Redis**: Enable authentication and TLS
2. **SQS**: Use IAM roles and least privilege access
3. **Credentials**: Never commit credentials to version control
4. **Encryption**: Use queue-level encryption for sensitive data

## Additional Resources

- [Backbone Queue Documentation](../README.md)
- [Redis Documentation](https://redis.io/documentation)
- [AWS SQS Developer Guide](https://docs.aws.amazon.com/sqs/)
- [Tokio Documentation](https://tokio.rs/tokio/tutorial)

## Contributing

To add new examples:

1. Create a new `.rs` file in this directory
2. Add comprehensive comments and documentation
3. Include error handling and logging
4. Update this README with your example
5. Test with both Redis and SQS when applicable

## Support

For issues or questions about these examples:
1. Check the troubleshooting section
2. Review the main README documentation
3. Create an issue in the repository
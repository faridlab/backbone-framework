# Backbone Jobs

A comprehensive job scheduling library for the Backbone Framework that provides robust cron-based task scheduling with PostgreSQL persistence and seamless integration with backbone-queue.

## 🚀 Features

- **Cron Expression Scheduling**: Full support for standard 5-field cron expressions with timezone handling
- **PostgreSQL Persistence**: Reliable job storage with PostgreSQL as the primary database
- **Queue Integration**: Seamless integration with backbone-queue (Redis, RabbitMQ, AWS SQS)
- **Job Lifecycle Management**: Complete CRUD operations for scheduled jobs
- **Retry Policies**: Configurable retry mechanisms with exponential backoff
- **Predefined Job Types**: Built-in templates for common scheduling tasks
- **Monitoring & Statistics**: Comprehensive job execution tracking and metrics
- **Builder Pattern**: Flexible, type-safe configuration
- **Async/Await**: Full tokio async support for high-performance scheduling
- **Error Handling**: Robust error recovery and logging

## 📦 Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
backbone-jobs = { version = "0.1.0", features = ["postgres", "redis"] }
tokio = { version = "1.0", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
```

### Optional Features

- `postgres`: PostgreSQL job storage (default)
- `redis`: Redis queue support
- `rabbitmq`: RabbitMQ queue support
- `monitoring`: Enhanced monitoring capabilities
- `pg_cron`: PostgreSQL pg_cron extension integration

## 🏃‍♂️ Quick Start

```rust
use backbone_jobs::{JobScheduler, JobBuilder, JobSchedulerBuilder};
use backbone_jobs::job_storage::InMemoryJobStorage;
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create scheduler with in-memory storage
    let storage = Arc::new(InMemoryJobStorage::new());
    let scheduler = JobSchedulerBuilder::new()
        .with_storage(storage)
        .build()?;

    // Start the scheduler
    scheduler.start().await?;

    // Schedule a job that runs every minute
    let job = JobBuilder::new()
        .id("health_check".to_string())
        .name("System Health Check")
        .description("Check system health and status")
        .cron("*/1 * * * *") // Every minute
        .queue("health_queue".to_string())
        .payload(json!({
            "type": "health_check",
            "services": ["database", "cache", "api"]
        }))
        .build()?;

    scheduler.schedule_job(job).await?;
    println!("✅ Job scheduled successfully!");

    // Keep running
    tokio::signal::ctrl_c().await?;
    scheduler.stop().await?;

    Ok(())
}
```

## 📋 Predefined Job Types

Backbone Jobs comes with several predefined job templates for common scenarios:

### Using Predefined Jobs

```rust
use backbone_jobs::job_types::*;

// Daily database backup at 2 AM
let backup_job = daily_backup()?;

// Weekly log cleanup on Sunday at 3 AM
let log_cleanup = weekly_log_cleanup()?;

// Hourly data synchronization
let data_sync = hourly_data_sync()?;

// Monthly analytics report
let monthly_report = monthly_report()?;

// Session cleanup every 6 hours
let session_cleanup = session_cleanup()?;

// Email campaigns on weekdays at 9 AM
let email_campaigns = email_campaign_schedule()?;

// Database maintenance weekly on Sunday at 1 AM
let db_maintenance = database_maintenance()?;

// Cache warming every 30 minutes
let cache_warming = cache_warming()?;

// Schedule all jobs
scheduler.schedule_job(backup_job).await?;
scheduler.schedule_job(log_cleanup).await?;
scheduler.schedule_job(data_sync).await?;
// ... schedule other jobs
```

### Available Predefined Jobs

| Job Type | Schedule | Description |
|----------|----------|-------------|
| `daily_backup()` | `0 2 * * *` | Daily database backup at 2 AM |
| `weekly_log_cleanup()` | `0 3 * * 0` | Weekly log cleanup on Sunday at 3 AM |
| `hourly_data_sync()` | `0 * * * *` | Hourly data synchronization |
| `monthly_report()` | `0 6 1 * *` | Monthly analytics report on 1st at 6 AM |
| `session_cleanup()` | `0 */6 * * *` | Session cleanup every 6 hours |
| `email_campaign_schedule()` | `0 9 * * 1-5` | Email campaigns on weekdays at 9 AM |
| `database_maintenance()` | `0 1 * * 0` | Database maintenance weekly on Sunday at 1 AM |
| `cache_warming()` | `*/30 * * * *` | Cache warming every 30 minutes |

## 🔧 Advanced Usage

### Custom Job Configuration

```rust
use backbone_jobs::{JobBuilder, RetryPolicy, JobPriority};
use std::time::Duration;

let custom_job = JobBuilder::new()
    .id("custom_job".to_string())
    .name("Custom Processing Job")
    .description("Process data with custom configuration")
    .cron("*/15 * * * *") // Every 15 minutes
    .queue("processing_queue".to_string())
    .payload(json!({
        "type": "data_processing",
        "batch_size": 1000,
        "parallel_workers": 4
    }))
    .priority(JobPriority::High)
    .timeout(3600) // 1 hour timeout
    .retry_policy(RetryPolicy::exponential(3, Duration::from_secs(300))) // 3 retries, 5min start
    .timezone("America/New_York")
    .max_attempts(5)
    .build()?;
```

### Scheduler Configuration

```rust
use backbone_jobs::{JobSchedulerBuilder, SchedulerConfig};
use std::time::Duration;

let config = SchedulerConfig {
    poll_interval: Duration::from_secs(30),     // Check every 30 seconds
    max_concurrent_jobs: 20,                    // Run up to 20 jobs concurrently
    default_timeout: Duration::from_secs(1800), // Default 30 minute timeout
    auto_start: true,
    default_timezone: "UTC".to_string(),
    cleanup_old_attempts: true,
    cleanup_attempts_older_than_days: 30,
};

let scheduler = JobSchedulerBuilder::new()
    .with_config(config)
    .with_storage(storage)
    .build()?;
```

### Job Lifecycle Management

```rust
// Schedule a job
let job = create_job()?;
scheduler.schedule_job(job).await?;

// List all jobs
let jobs = scheduler.list_jobs().await?;
for job in jobs {
    println!("Job: {} ({})", job.name, job.status);
}

// Get job by ID
let job = scheduler.get_job(&job_id).await?;

// Update job
scheduler.update_job(&updated_job).await?;

// Pause a job
scheduler.pause_job(&job_id).await?;

// Resume a paused job
scheduler.resume_job(&job_id).await?;

// Cancel a job
scheduler.cancel_job(&job_id).await?;

// Trigger immediate execution
scheduler.trigger_job(&job_id).await?;

// Delete a job
scheduler.unschedule_job(&job_id).await?;
```

### Job Statistics and Monitoring

```rust
// Get scheduler statistics
let stats = scheduler.get_statistics().await?;
println!("Total jobs: {}", stats.total_jobs);
println!("Success rate: {:.2}%", stats.success_rate);
println!("Active workers: {}", stats.active_workers);
println!("Uptime: {} seconds", stats.uptime_seconds);

// Get job execution history
let history = scheduler.get_job_history(&job_id, Some(10)).await?;
for attempt in history {
    println!("Attempt {}: {} ({})",
        attempt.attempt_number,
        attempt.result,
        attempt.started_at
    );
}
```

## 🕐 Cron Expressions

Backbone Jobs supports standard 5-field cron expressions:

```
* * * * *
│ │ │ │ │
│ │ │ │ └─── Day of Week (0-7, Sunday=0 or 7)
│ │ │ └───── Month (1-12)
│ │ └─────── Day of Month (1-31)
│ └───────── Hour (0-23)
└─────────── Minute (0-59)
```

### Common Patterns

| Pattern | Description |
|---------|-------------|
| `* * * * *` | Every minute |
| `*/15 * * * *` | Every 15 minutes |
| `0 * * * *` | Every hour at minute 0 |
| `0 2 * * *` | Daily at 2 AM |
| `0 9 * * 1-5` | Weekdays at 9 AM |
| `0 0 * * 0` | Weekly on Sunday at midnight |
| `0 0 1 * *` | Monthly on 1st at midnight |
| `0 9-17 * * 1-5` | Every hour from 9 AM to 5 PM on weekdays |

### Advanced Examples

```rust
// Complex scheduling examples
let jobs = vec![
    // Business hours monitoring (9 AM - 5 PM, Monday-Friday)
    ("business_monitoring", "0 9-17 * * 1-5"),

    // End of month financial processing
    ("financial_close", "0 18 28-31 * *"),

    // Quarterly reports
    ("quarterly_report", "0 8 1 1,4,7,10 *"),

    // Every 2 hours during weekdays, every 4 hours on weekends
    ("weekend_diff_schedule", "0 */2 * * 1-5", "0 */4 * * 6,0"),
];
```

## 🔄 Retry Policies

Configure custom retry behavior for failed jobs:

```rust
use backbone_jobs::RetryPolicy;
use std::time::Duration;

// Exponential backoff with jitter
let exponential_retry = RetryPolicy::exponential(5, Duration::from_secs(60))
    .with_max_delay(Duration::from_secs(3600))
    .with_jitter(0.1); // 10% jitter

// Fixed delay retries
let fixed_retry = RetryPolicy::fixed(3, Duration::from_secs(300));

// Linear backoff
let linear_retry = RetryPolicy::linear(4, Duration::from_secs(60));

// No retries
let no_retry = RetryPolicy::none();

// Apply to job
let job = JobBuilder::new()
    .id("retry_job".to_string())
    .name("Job with Custom Retry")
    .cron("0 */1 * * *")
    .queue("retry_queue".to_string())
    .payload(json!({"type": "processing"}))
    .retry_policy(exponential_retry)
    .build()?;
```

## 🗄️ Storage Backends

### In-Memory Storage (for testing)

```rust
use backbone_jobs::job_storage::InMemoryJobStorage;

let storage = Arc::new(InMemoryJobStorage::new());
let scheduler = JobSchedulerBuilder::new()
    .with_storage(storage)
    .build()?;
```

### PostgreSQL Storage (production)

```rust
// PostgreSQL storage will be implemented with full feature support
// Including:
// - Connection pooling
// - Migration support
// - Transaction handling
// - Performance optimization
// - Backup/restore capabilities

// Example (coming soon):
let storage = PostgreSQLJobStorage::new(database_url).await?;
let scheduler = JobSchedulerBuilder::new()
    .with_storage(Arc::new(storage))
    .build()?;
```

## 📊 Queue Integration

### Redis Queue

```rust
use backbone_queue::RedisQueueService;
use backbone_jobs::job_executor::JobExecutor;

let queue_service = Arc::new(RedisQueueService::new(redis_url).await?);
let executor = JobExecutor::new(queue_service, Duration::from_secs(300));

let scheduler = JobSchedulerBuilder::new()
    .with_storage(storage)
    .with_executor(executor)
    .build()?;
```

### RabbitMQ Queue

```rust
use backbone_queue::RabbitMQQueueService;

let queue_service = Arc::new(RabbitMQQueueService::new(amqp_url).await?);
let executor = JobExecutor::new(queue_service, Duration::from_secs(300));
```

### AWS SQS Queue

```rust
use backbone_queue::SqsQueueService;

let queue_service = Arc::new(SqsQueueService::new(aws_config, queue_url).await?);
let executor = JobExecutor::new(queue_service, Duration::from_secs(300));
```

## 🔍 Error Handling

Backbone Jobs provides comprehensive error handling:

```rust
use backbone_jobs::JobError;

match scheduler.schedule_job(job).await {
    Ok(_) => println!("Job scheduled successfully"),
    Err(JobError::JobAlreadyExists(id)) => {
        eprintln!("Job with ID {} already exists", id);
    }
    Err(JobError::Validation(msg)) => {
        eprintln!("Job validation failed: {}", msg);
    }
    Err(e) => {
        eprintln!("Failed to schedule job: {}", e);
    }
}
```

### Error Types

- `JobError::JobAlreadyExists`: Job with same ID already exists
- `JobError::JobNotFound`: Job not found
- `JobError::Validation`: Job validation failed
- `JobError::Execution`: Job execution failed
- `JobError::Storage`: Storage operation failed
- `JobError::Configuration`: Configuration error
- `JobError::CronParse`: Cron expression parsing failed

## 📈 Monitoring and Logging

### Structured Logging

```rust
use tracing::{info, warn, error};

// Enable debug logging
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();

// Logs will include:
// - Job scheduling events
// - Execution start/completion
// - Retry attempts
// - Error details
// - Performance metrics
```

### Custom Metrics

```rust
// Get real-time statistics
let stats = scheduler.get_statistics().await?;

// Export to monitoring systems
prometheus::register_gauge!(
    "backbone_jobs_total",
    "Total number of scheduled jobs"
).set(stats.total_jobs as f64);

prometheus::register_gauge!(
    "backbone_jobs_success_rate",
    "Job success rate percentage"
).set(stats.success_rate);
```

## 🧪 Testing

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use backbone_jobs::job_storage::InMemoryJobStorage;

    #[tokio::test]
    async fn test_job_scheduling() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let scheduler = JobSchedulerBuilder::new()
            .with_storage(storage)
            .build()
            .unwrap();

        scheduler.start().await.unwrap();

        let job = JobBuilder::new()
            .id("test_job".to_string())
            .name("Test Job")
            .cron("*/1 * * * *")
            .queue("test_queue".to_string())
            .payload(json!({"test": true}))
            .build()
            .unwrap();

        assert!(scheduler.schedule_job(job).await.is_ok());

        let jobs = scheduler.list_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);
    }
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_job_execution() {
    // Set up test environment
    let storage = Arc::new(InMemoryJobStorage::new());
    let queue_service = Arc::new(MockQueueService::new());

    let scheduler = JobSchedulerBuilder::new()
        .with_storage(storage)
        .build()
        .unwrap();

    scheduler.start().await.unwrap();

    // Schedule job that should execute quickly
    let job = JobBuilder::new()
        .id("quick_job".to_string())
        .name("Quick Test Job")
        .cron("*/1 * * * *") // Every minute
        .queue("test_queue".to_string())
        .payload(json!({"immediate": true}))
        .build()
        .unwrap();

    scheduler.schedule_job(job).await.unwrap();

    // Wait for execution
    tokio::time::sleep(Duration::from_secs(70)).await;

    // Verify job executed
    let stats = scheduler.get_statistics().await.unwrap();
    assert!(stats.jobs_processed > 0);
}
```

## 🏢 Production Deployment

### Configuration

```yaml
# application.yml
scheduler:
  poll_interval: 30s
  max_concurrent_jobs: 50
  default_timeout: 1800s
  default_timezone: "UTC"
  cleanup_old_attempts: true
  cleanup_attempts_older_than_days: 30

database:
  url: "postgresql://user:pass@localhost/backbone_jobs"
  max_connections: 20
  min_connections: 5

queue:
  type: "redis"  # redis, rabbitmq, sqs
  url: "redis://localhost:6379"

monitoring:
  enabled: true
  metrics_port: 9090
  health_check_interval: 30s
```

### Docker Deployment

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/your-app /usr/local/bin/
EXPOSE 3000
CMD ["your-app"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: backbone-jobs-scheduler
spec:
  replicas: 2
  selector:
    matchLabels:
      app: backbone-jobs-scheduler
  template:
    metadata:
      labels:
        app: backbone-jobs-scheduler
    spec:
      containers:
      - name: scheduler
        image: your-app:latest
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secret
              key: url
        - name: REDIS_URL
          value: "redis://redis:6379"
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
```

## 🔧 Best Practices

### Job Design

1. **Idempotent Jobs**: Design jobs to be safe to run multiple times
2. **Time Limits**: Set appropriate timeouts to prevent hanging jobs
3. **Retry Strategies**: Use exponential backoff for transient failures
4. **Monitoring**: Log important events and metrics
5. **Resource Limits**: Consider memory and CPU usage for batch jobs

### Cron Expressions

1. **Test Before Deploy**: Use online cron testers to verify expressions
2. **Timezone Awareness**: Always specify timezones for distributed systems
3. **Avoid Overlap**: Space out resource-intensive jobs
4. **Maintenance Windows**: Schedule heavy tasks during low-traffic periods

### Performance

1. **Batch Processing**: Process items in batches rather than one-by-one
2. **Connection Pooling**: Use database connection pools
3. **Async Operations**: Use async/await for I/O operations
4. **Monitoring**: Track job execution times and success rates

## 🚀 Examples

See the `examples/` directory for comprehensive examples:

- [`basic_scheduler.rs`](examples/basic_scheduler.rs) - Basic scheduler setup and job scheduling
- [`cron_jobs.rs`](examples/cron_jobs.rs) - Advanced cron patterns and real-world scenarios
- [`database_cleanup.rs`](examples/database_cleanup.rs) - Database maintenance automation

### Running Examples

```bash
# Basic scheduler example
cargo run --example basic_scheduler

# Advanced cron patterns
cargo run --example cron_jobs

# Database cleanup automation
cargo run --example database_cleanup
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related Projects

- [Backbone Queue](../backbone-queue/README.md) - Message queue abstraction layer
- [Backbone Framework](../../docs/technical/FRAMEWORK.md) - Complete framework documentation
- [Backbone Framework Quick Start](../../docs/technical/QUICKSTART.md) - Quick start guide

## 📞 Support

- Create an issue for bug reports or feature requests
- Check the [examples](examples/) directory for usage patterns
- Review the [API documentation](docs/api/) for detailed reference
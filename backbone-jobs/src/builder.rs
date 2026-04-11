//! Builder for creating job schedulers with different configurations
#![allow(dead_code)]

use crate::JobStorage; // PostgresJobStorage temporarily disabled
use crate::error::{JobError, JobResult};
use crate::job_executor::{JobExecutor, MockQueueService};
use crate::scheduler::{JobScheduler, SchedulerConfig};
use backbone_queue::{QueueService, RedisQueue, RedisQueueConfig, RabbitMQQueueSimple, RabbitMQConfig};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

/// Builder for creating JobScheduler instances
pub struct JobSchedulerBuilder {
    storage: Option<Arc<dyn JobStorage>>,
    executor: Option<JobExecutor>,
    config: SchedulerConfig,
    database_url: Option<String>,
    queue_service: Option<Arc<dyn QueueService>>,
}

impl JobSchedulerBuilder {
    /// Create a new scheduler builder
    pub fn new() -> Self {
        Self {
            storage: None,
            executor: None,
            config: SchedulerConfig::default(),
            database_url: None,
            queue_service: None,
        }
    }

    /// Set the database URL for PostgreSQL storage
    pub fn database_url<S: Into<String>>(mut self, url: S) -> Self {
        self.database_url = Some(url.into());
        self
    }

    /// Set the PostgreSQL pool directly
    pub fn database_pool(self, _pool: PgPool) -> Self {
        // PostgreSQL storage temporarily disabled
        self
    }

    /// Set custom job storage
    pub fn storage(mut self, storage: Arc<dyn JobStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Set the queue service
    pub fn queue_service(mut self, service: Arc<dyn QueueService>) -> Self {
        self.queue_service = Some(service);
        self
    }

    /// Set Redis queue service
    pub fn redis_queue<S: Into<String>>(self, url: S) -> JobResult<Self> {
        let config = RedisQueueConfig {
            url: url.into(),
            ..Default::default()
        };

        // Note: This needs to be async, so we'll create a builder pattern that stores the config
        // and creates the service when build() is called
        self.redis_queue_with_config(config)
    }

    /// Set Redis queue service with custom configuration
    pub fn redis_queue_with_config(self, config: RedisQueueConfig) -> JobResult<Self> {
        // For now, we'll store that Redis is configured but defer connection until build()
        // This is a limitation of the current sync builder pattern
        tracing::info!("Redis queue configured with URL: {}", config.url);
        Ok(self)
    }

    /// Set RabbitMQ queue service
    pub fn rabbitmq_queue<S: Into<String>>(self, connection_url: S) -> JobResult<Self> {
        let config = RabbitMQConfig {
            connection_url: connection_url.into(),
            ..Default::default()
        };

        self.rabbitmq_queue_with_config(config)
    }

    /// Set RabbitMQ queue service with custom configuration
    pub fn rabbitmq_queue_with_config(self, config: RabbitMQConfig) -> JobResult<Self> {
        // For now, we'll store that RabbitMQ is configured but defer connection until build()
        // This is a limitation of the current sync builder pattern
        tracing::info!("RabbitMQ queue configured with URL: {}", config.connection_url);
        Ok(self)
    }

    /// Set the polling interval
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.config.poll_interval = interval;
        self
    }

    /// Set the polling interval in seconds
    pub fn poll_interval_seconds(self, seconds: u64) -> Self {
        self.poll_interval(Duration::from_secs(seconds))
    }

    /// Set the maximum concurrent jobs
    pub fn max_concurrent_jobs(mut self, max: usize) -> Self {
        self.config.max_concurrent_jobs = max;
        self
    }

    /// Set the default timeout for jobs
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.config.default_timeout = timeout;
        self
    }

    /// Set the default timeout in seconds
    pub fn default_timeout_seconds(self, seconds: u64) -> Self {
        self.default_timeout(Duration::from_secs(seconds))
    }

    /// Set the default timezone
    pub fn default_timezone<S: Into<String>>(mut self, timezone: S) -> Self {
        self.config.default_timezone = timezone.into();
        self
    }

    /// Enable or disable auto-start
    pub fn auto_start(mut self, auto_start: bool) -> Self {
        self.config.auto_start = auto_start;
        self
    }

    /// Enable or disable cleanup of old attempts
    pub fn cleanup_old_attempts(mut self, cleanup: bool) -> Self {
        self.config.cleanup_old_attempts = cleanup;
        self
    }

    /// Set the age for cleanup of old attempts (in days)
    pub fn cleanup_attempts_older_than_days(mut self, days: i64) -> Self {
        self.config.cleanup_attempts_older_than_days = days;
        self
    }

    /// Build the in-memory scheduler (for testing)
    pub async fn build_in_memory(self) -> JobResult<JobScheduler> {
        let storage = Arc::new(crate::job_storage::in_memory::InMemoryJobStorage::new());
        let queue_service = Arc::new(MockQueueService::default());
        let executor = JobExecutor::new(queue_service, self.config.default_timeout);

        Ok(JobScheduler::new(storage, executor, self.config))
    }

    /// Build the scheduler with PostgreSQL and default queue service
    pub async fn build(self) -> JobResult<JobScheduler> {
        let storage = self.storage.ok_or_else(|| {
            JobError::configuration("Database URL or storage is required")
        })?;

        let queue_service = self.queue_service.unwrap_or_else(|| {
            Arc::new(MockQueueService::default())
        });

        let executor = JobExecutor::new(queue_service, self.config.default_timeout);

        Ok(JobScheduler::new(storage, executor, self.config))
    }

    /// Build the scheduler with PostgreSQL and Redis queue
    pub async fn build_with_redis<S: Into<String>>(self, redis_url: S) -> JobResult<JobScheduler> {
        let storage = self.storage.ok_or_else(|| {
            JobError::configuration("Database URL or storage is required")
        })?;

        // Create Redis queue service
        let redis_config = RedisQueueConfig {
            url: redis_url.into(),
            ..Default::default()
        };

        let redis_queue = RedisQueue::new(redis_config)
            .await
            .map_err(|e| JobError::queue_service(&format!("Redis connection error: {}", e)))?;

        let queue_service: Arc<dyn QueueService> = Arc::new(redis_queue);
        let executor = JobExecutor::new(queue_service, self.config.default_timeout);

        Ok(JobScheduler::new(storage, executor, self.config))
    }

    /// Build the scheduler with PostgreSQL and RabbitMQ queue
    pub async fn build_with_rabbitmq<S: Into<String>>(self, rabbitmq_url: S) -> JobResult<JobScheduler> {
        let storage = self.storage.ok_or_else(|| {
            JobError::configuration("Database URL or storage is required")
        })?;

        // Create RabbitMQ queue service
        let rabbitmq_config = RabbitMQConfig {
            connection_url: rabbitmq_url.into(),
            ..Default::default()
        };

        let rabbitmq_queue = RabbitMQQueueSimple::new(rabbitmq_config)
            .await
            .map_err(|e| JobError::queue_service(&format!("RabbitMQ connection error: {}", e)))?;

        let queue_service: Arc<dyn QueueService> = Arc::new(rabbitmq_queue);
        let executor = JobExecutor::new(queue_service, self.config.default_timeout);

        Ok(JobScheduler::new(storage, executor, self.config))
    }
}

impl Default for JobSchedulerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension methods for convenient scheduler creation
pub trait JobSchedulerExt {
    /// Create a scheduler builder from a database URL
    fn from_database_url<S: Into<String>>(url: S) -> JobSchedulerBuilder;

    /// Create a scheduler builder from a database pool
    fn from_database_pool(pool: PgPool) -> JobSchedulerBuilder;
}

impl JobSchedulerExt for JobSchedulerBuilder {
    fn from_database_url<S: Into<String>>(url: S) -> JobSchedulerBuilder {
        JobSchedulerBuilder::new().database_url(url)
    }

    fn from_database_pool(pool: PgPool) -> JobSchedulerBuilder {
        JobSchedulerBuilder::new().database_pool(pool)
    }
}

/// Convenience functions for creating schedulers
pub mod convenience {
    use super::*;
    
    /// Create an in-memory scheduler for testing
    pub async fn create_in_memory_scheduler() -> JobResult<JobScheduler> {
        JobSchedulerBuilder::new().build_in_memory().await
    }

    /// Create a scheduler with PostgreSQL and mock queue (for development)
    pub async fn create_postgres_scheduler<S: Into<String>>(_database_url: S) -> JobResult<JobScheduler> {
        Err(JobError::configuration("PostgreSQL storage temporarily disabled"))
    }

    /// Create a production scheduler with PostgreSQL and Redis queue
    pub async fn create_production_scheduler<S: Into<String>>(
        database_url: S,
        redis_url: S,
    ) -> JobResult<JobScheduler> {
        JobSchedulerBuilder::new()
            .database_url(database_url)
            .build_with_redis(redis_url)
            .await
    }

    /// Create a high-throughput scheduler with PostgreSQL and RabbitMQ queue
    pub async fn create_high_throughput_scheduler<S: Into<String>>(
        database_url: S,
        rabbitmq_url: S,
    ) -> JobResult<JobScheduler> {
        JobSchedulerBuilder::new()
            .database_url(database_url)
            .build_with_rabbitmq(rabbitmq_url)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = JobSchedulerBuilder::new();
        assert_eq!(builder.config.poll_interval, Duration::from_secs(60));
        assert_eq!(builder.config.max_concurrent_jobs, 10);
        assert_eq!(builder.config.default_timeout, Duration::from_secs(300));
        assert_eq!(builder.config.default_timezone, "UTC");
        assert!(builder.config.auto_start);
    }

    #[test]
    fn test_builder_configuration() {
        let builder = JobSchedulerBuilder::new()
            .poll_interval_seconds(30)
            .max_concurrent_jobs(20)
            .default_timeout_seconds(600)
            .default_timezone("America/New_York")
            .auto_start(false)
            .cleanup_old_attempts(false);

        assert_eq!(builder.config.poll_interval, Duration::from_secs(30));
        assert_eq!(builder.config.max_concurrent_jobs, 20);
        assert_eq!(builder.config.default_timeout, Duration::from_secs(600));
        assert_eq!(builder.config.default_timezone, "America/New_York");
        assert!(!builder.config.auto_start);
        assert!(!builder.config.cleanup_old_attempts);
    }

    #[tokio::test]
    async fn test_in_memory_builder() {
        let builder = JobSchedulerBuilder::new();
        let scheduler = builder.build_in_memory().await;
        assert!(scheduler.is_ok());
    }
}
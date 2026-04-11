//! Configuration management for job scheduler

use crate::error::{JobError, JobResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the job scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct JobSchedulerConfig {
    /// Database configuration
    pub database: DatabaseConfig,

    /// Queue configuration
    pub queue: QueueConfig,

    /// Scheduler settings
    pub scheduler: SchedulerSettings,

    /// Monitoring configuration
    pub monitoring: MonitoringConfig,

    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: String,

    /// Maximum number of database connections
    pub max_connections: u32,

    /// Minimum number of database connections
    pub min_connections: u32,

    /// Connection timeout in seconds
    pub connect_timeout: u64,

    /// Idle timeout in seconds
    pub idle_timeout: u64,

    /// Whether to enable connection SSL
    pub ssl_enabled: bool,

    /// SSL certificate path (if required)
    pub ssl_cert_path: Option<String>,

    /// Database schema version
    pub schema_version: String,
}

/// Queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Queue type (redis, rabbitmq, aws_sqs, in_memory)
    pub queue_type: String,

    /// Queue connection URL
    pub url: String,

    /// Default queue name
    pub default_queue: String,

    /// Message timeout in seconds
    pub message_timeout: u64,

    /// Maximum retry attempts
    pub max_retries: u32,

    /// Whether to use dead letter queue
    pub use_dead_letter_queue: bool,

    /// Dead letter queue name
    pub dead_letter_queue: Option<String>,

    /// Message compression
    pub compress_messages: bool,
}

/// Scheduler settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerSettings {
    /// Polling interval in seconds
    pub poll_interval_seconds: u64,

    /// Maximum concurrent jobs
    pub max_concurrent_jobs: usize,

    /// Default job timeout in seconds
    pub default_job_timeout: u64,

    /// Default timezone for cron expressions
    pub default_timezone: String,

    /// Whether to auto-start scheduler
    pub auto_start: bool,

    /// Whether to cleanup old execution attempts
    pub cleanup_old_attempts: bool,

    /// Age of attempts to cleanup (in days)
    pub cleanup_attempts_older_than_days: i64,

    /// Job execution timeout in seconds
    pub job_execution_timeout: u64,

    /// Whether to enable job prioritization
    pub enable_prioritization: bool,

    /// Maximum job queue size
    pub max_job_queue_size: usize,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Whether to enable metrics collection
    pub enable_metrics: bool,

    /// Metrics collection interval in seconds
    pub metrics_interval_seconds: u64,

    /// Whether to enable health checks
    pub enable_health_checks: bool,

    /// Health check port
    pub health_check_port: u16,

    /// Whether to enable prometheus metrics
    pub enable_prometheus: bool,

    /// Prometheus metrics port
    pub prometheus_port: u16,

    /// Alert thresholds
    pub alerts: AlertThresholds,
}

/// Alert thresholds for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Maximum failure rate percentage before alerting
    pub max_failure_rate: f64,

    /// Maximum job execution time in seconds
    pub max_execution_time: u64,

    /// Maximum queue size before alerting
    pub max_queue_size: usize,

    /// Maximum database connection usage percentage
    pub max_db_connection_usage: f64,

    /// Minimum scheduler uptime percentage
    pub min_uptime_percentage: f64,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,

    /// Whether to log job executions
    pub log_job_executions: bool,

    /// Whether to log job payloads (may contain sensitive data)
    pub log_job_payloads: bool,

    /// Whether to log performance metrics
    pub log_performance_metrics: bool,

    /// Log format (json, pretty, compact)
    pub format: String,

    /// Whether to log to file
    pub log_to_file: bool,

    /// Log file path (if logging to file)
    pub log_file_path: Option<String>,

    /// Maximum log file size in MB
    pub max_log_file_size_mb: u64,

    /// Number of log files to keep
    pub log_file_count: u32,
}


impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgresql://root:password@localhost:5432/backbone_jobs".to_string(),
            max_connections: 20,
            min_connections: 5,
            connect_timeout: 30,
            idle_timeout: 300,
            ssl_enabled: false,
            ssl_cert_path: None,
            schema_version: "1.0.0".to_string(),
        }
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            queue_type: "redis".to_string(),
            url: "redis://localhost:6379".to_string(),
            default_queue: "backbone_jobs".to_string(),
            message_timeout: 300,
            max_retries: 5,
            use_dead_letter_queue: true,
            dead_letter_queue: Some("backbone_jobs_dlq".to_string()),
            compress_messages: false,
        }
    }
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            poll_interval_seconds: 60,
            max_concurrent_jobs: 10,
            default_job_timeout: 300,
            default_timezone: "UTC".to_string(),
            auto_start: true,
            cleanup_old_attempts: true,
            cleanup_attempts_older_than_days: 30,
            job_execution_timeout: 300,
            enable_prioritization: true,
            max_job_queue_size: 1000,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_interval_seconds: 30,
            enable_health_checks: true,
            health_check_port: 8080,
            enable_prometheus: false,
            prometheus_port: 9090,
            alerts: AlertThresholds::default(),
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_failure_rate: 10.0,
            max_execution_time: 3600,
            max_queue_size: 500,
            max_db_connection_usage: 80.0,
            min_uptime_percentage: 99.0,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            log_job_executions: true,
            log_job_payloads: false,
            log_performance_metrics: true,
            format: "json".to_string(),
            log_to_file: false,
            log_file_path: None,
            max_log_file_size_mb: 100,
            log_file_count: 5,
        }
    }
}

impl JobSchedulerConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> JobResult<Self> {
        let mut config = Self::default();

        // Database configuration
        if let Ok(url) = std::env::var("DATABASE_URL") {
            config.database.url = url;
        }
        if let Ok(max_conn) = std::env::var("DB_MAX_CONNECTIONS") {
            config.database.max_connections = max_conn.parse()
                .map_err(|e| JobError::configuration(&format!("Invalid DB_MAX_CONNECTIONS: {}", e)))?;
        }
        if let Ok(min_conn) = std::env::var("DB_MIN_CONNECTIONS") {
            config.database.min_connections = min_conn.parse()
                .map_err(|e| JobError::configuration(&format!("Invalid DB_MIN_CONNECTIONS: {}", e)))?;
        }

        // Queue configuration
        if let Ok(queue_type) = std::env::var("QUEUE_TYPE") {
            config.queue.queue_type = queue_type;
        }
        if let Ok(queue_url) = std::env::var("QUEUE_URL") {
            config.queue.url = queue_url;
        }

        // Scheduler configuration
        if let Ok(poll_interval) = std::env::var("SCHEDULER_POLL_INTERVAL") {
            config.scheduler.poll_interval_seconds = poll_interval.parse()
                .map_err(|e| JobError::configuration(&format!("Invalid SCHEDULER_POLL_INTERVAL: {}", e)))?;
        }
        if let Ok(max_jobs) = std::env::var("SCHEDULER_MAX_CONCURRENT_JOBS") {
            config.scheduler.max_concurrent_jobs = max_jobs.parse()
                .map_err(|e| JobError::configuration(&format!("Invalid SCHEDULER_MAX_CONCURRENT_JOBS: {}", e)))?;
        }

        // Monitoring configuration
        if let Ok(monitoring) = std::env::var("MONITORING_ENABLED") {
            config.monitoring.enable_metrics = monitoring.parse()
                .map_err(|e| JobError::configuration(&format!("Invalid MONITORING_ENABLED: {}", e)))?;
        }
        if let Ok(health_port) = std::env::var("HEALTH_CHECK_PORT") {
            config.monitoring.health_check_port = health_port.parse()
                .map_err(|e| JobError::configuration(&format!("Invalid HEALTH_CHECK_PORT: {}", e)))?;
        }

        // Logging configuration
        if let Ok(log_level) = std::env::var("LOG_LEVEL") {
            config.logging.level = log_level;
        }

        Ok(config)
    }

    /// Load configuration from a file
    pub fn from_file(path: &str) -> JobResult<Self> {
        let config_content = std::fs::read_to_string(path)
            .map_err(|e| JobError::configuration(&format!("Failed to read config file {}: {}", path, e)))?;

        // Try TOML format first
        if path.ends_with(".toml") {
            toml::from_str(&config_content)
                .map_err(|e| JobError::configuration(&format!("Failed to parse TOML config: {}", e)))
        }
        // Try YAML format
        else if path.ends_with(".yml") || path.ends_with(".yaml") {
            serde_yaml::from_str(&config_content)
                .map_err(|e| JobError::configuration(&format!("Failed to parse YAML config: {}", e)))
        }
        // Try JSON format
        else if path.ends_with(".json") {
            serde_json::from_str(&config_content)
                .map_err(|e| JobError::configuration(&format!("Failed to parse JSON config: {}", e)))
        }
        else {
            Err(JobError::configuration(&format!("Unsupported config file format: {}", path)))
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> JobResult<()> {
        // Validate database URL
        if self.database.url.is_empty() {
            return Err(JobError::configuration("Database URL cannot be empty"));
        }

        // Validate queue configuration
        if self.queue.url.is_empty() {
            return Err(JobError::configuration("Queue URL cannot be empty"));
        }

        // Validate queue type
        if !["redis", "rabbitmq", "aws_sqs", "in_memory"].contains(&self.queue.queue_type.as_str()) {
            return Err(JobError::configuration(&format!(
                "Invalid queue type: {}. Must be one of: redis, rabbitmq, aws_sqs, in_memory",
                self.queue.queue_type
            )));
        }

        // Validate scheduler settings
        if self.scheduler.poll_interval_seconds == 0 {
            return Err(JobError::configuration("Poll interval must be greater than 0"));
        }
        if self.scheduler.max_concurrent_jobs == 0 {
            return Err(JobError::configuration("Max concurrent jobs must be greater than 0"));
        }
        if self.scheduler.default_job_timeout == 0 {
            return Err(JobError::configuration("Default job timeout must be greater than 0"));
        }

        // Validate timezone
        if self.scheduler.default_timezone.is_empty() {
            return Err(JobError::configuration("Default timezone cannot be empty"));
        }

        // Validate monitoring configuration
        if self.monitoring.health_check_port == 0 {
            return Err(JobError::configuration("Health check port must be greater than 0"));
        }
        if self.monitoring.prometheus_port == 0 {
            return Err(JobError::configuration("Prometheus port must be greater than 0"));
        }

        // Validate logging configuration
        let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_log_levels.contains(&self.logging.level.as_str()) {
            return Err(JobError::configuration(&format!(
                "Invalid log level: {}. Must be one of: trace, debug, info, warn, error",
                self.logging.level
            )));
        }

        let valid_log_formats = ["json", "pretty", "compact"];
        if !valid_log_formats.contains(&self.logging.format.as_str()) {
            return Err(JobError::configuration(&format!(
                "Invalid log format: {}. Must be one of: json, pretty, compact",
                self.logging.format
            )));
        }

        Ok(())
    }

    /// Get database connection pool size
    pub fn db_pool_size(&self) -> u32 {
        self.database.max_connections
    }

    /// Get polling interval as Duration
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.scheduler.poll_interval_seconds)
    }

    /// Get default job timeout as Duration
    pub fn default_job_timeout(&self) -> Duration {
        Duration::from_secs(self.scheduler.default_job_timeout)
    }

    /// Get metrics collection interval as Duration
    pub fn metrics_interval(&self) -> Duration {
        Duration::from_secs(self.monitoring.metrics_interval_seconds)
    }

    /// Get job execution timeout as Duration
    pub fn job_execution_timeout(&self) -> Duration {
        Duration::from_secs(self.scheduler.job_execution_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_default_config() {
        let config = JobSchedulerConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(config.queue.queue_type, "redis");
        assert_eq!(config.scheduler.poll_interval_seconds, 60);
        assert_eq!(config.monitoring.health_check_port, 8080);
        assert_eq!(config.logging.level, "info");
    }

    #[test]
    fn test_config_from_env() {
        env::set_var("DATABASE_URL", "postgresql://test:pass@localhost/testdb");
        env::set_var("SCHEDULER_POLL_INTERVAL", "30");
        env::set_var("LOG_LEVEL", "debug");

        let config = JobSchedulerConfig::from_env().unwrap();
        assert_eq!(config.database.url, "postgresql://test:pass@localhost/testdb");
        assert_eq!(config.scheduler.poll_interval_seconds, 30);
        assert_eq!(config.logging.level, "debug");

        env::remove_var("DATABASE_URL");
        env::remove_var("SCHEDULER_POLL_INTERVAL");
        env::remove_var("LOG_LEVEL");
    }

    #[test]
    fn test_config_validation() {
        let mut config = JobSchedulerConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid database URL should fail
        config.database.url = "".to_string();
        assert!(config.validate().is_err());

        // Reset and test invalid queue type
        config.database.url = "postgresql://test:pass@localhost/testdb".to_string();
        config.queue.queue_type = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_duration_helpers() {
        let config = JobSchedulerConfig::default();

        assert_eq!(config.poll_interval(), Duration::from_secs(60));
        assert_eq!(config.default_job_timeout(), Duration::from_secs(300));
        assert_eq!(config.metrics_interval(), Duration::from_secs(30));
        assert_eq!(config.job_execution_timeout(), Duration::from_secs(300));
    }
}
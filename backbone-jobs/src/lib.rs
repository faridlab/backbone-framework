//! # Backbone Jobs - Job Scheduling and Cron Management
//!
//! A comprehensive job scheduling library for the Backbone Framework that provides:
//! - Cron expression parsing and scheduling
//! - PostgreSQL persistence for scheduled jobs
//! - Integration with backbone-queue for job execution
//! - pg_cron integration for database-level scheduling
//! - Job lifecycle management (CRUD operations)
//! - Monitoring and health checks
//! - Retry policies and error handling

use std::time::Duration;

/// Current library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default job execution timeout in seconds
pub const DEFAULT_JOB_TIMEOUT: u64 = 300; // 5 minutes

/// Maximum retry attempts for failed jobs
pub const MAX_RETRY_ATTEMPTS: u32 = 5;

/// Default polling interval for job scheduler in seconds
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Common job types that can be used as templates
pub mod job_types {
    use super::job::{Job, JobBuilder};
    use super::types::JobId;
    use anyhow::Result;
    use chrono::Utc;
    use serde_json::json;

    /// Create a daily backup job
    pub fn daily_backup() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::from_string(format!("daily_backup_{}", Utc::now().date_naive())))
            .name("Daily Database Backup")
            .description("Backup all database tables")
            .cron("0 2 * * *") // Daily at 2 AM
            .queue("backup_queue")
            .payload(json!({
                "type": "backup",
                "all_tables": true,
                "compression": true
            }))
            .timezone("UTC")
            .timeout(3600) // 1 hour timeout
            .build()?)
    }

    /// Create a weekly log cleanup job
    pub fn weekly_log_cleanup() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("weekly_log_cleanup"))
            .name("Weekly Log Cleanup")
            .description("Clean up old log files and database entries")
            .cron("0 3 * * 0") // Sunday at 3 AM
            .queue("maintenance_queue")
            .payload(json!({
                "type": "cleanup",
                "target": "logs",
                "older_than_days": 30
            }))
            .timezone("UTC")
            .build()?)
    }

    /// Create an hourly data sync job
    pub fn hourly_data_sync() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("hourly_data_sync"))
            .name("Hourly Data Synchronization")
            .description("Synchronize data between services")
            .cron("0 * * * *") // Every hour at minute 0
            .queue("sync_queue")
            .payload(json!({
                "type": "sync",
                "services": ["users", "orders", "products"]
            }))
            .timezone("UTC")
            .build()?)
    }

    /// Create a monthly report generation job
    pub fn monthly_report() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("monthly_report"))
            .name("Monthly Analytics Report")
            .description("Generate monthly analytics and business reports")
            .cron("0 6 1 * *") // 1st of every month at 6 AM
            .queue("reports_queue")
            .payload(json!({
                "type": "report",
                "period": "monthly",
                "formats": ["pdf", "csv", "excel"]
            }))
            .timezone("UTC")
            .build()?)
    }

    /// Create a user session cleanup job (runs every 6 hours)
    pub fn session_cleanup() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("session_cleanup"))
            .name("Session Cleanup")
            .description("Clean up expired user sessions")
            .cron("0 */6 * * *") // Every 6 hours
            .queue("maintenance_queue")
            .payload(json!({
                "type": "cleanup",
                "target": "sessions",
                "older_than_hours": 24
            }))
            .timezone("UTC")
            .build()?)
    }

    /// Create an email campaign scheduler job
    pub fn email_campaign_schedule() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("email_campaign_schedule"))
            .name("Email Campaign Scheduler")
            .description("Schedule and send email campaigns")
            .cron("0 9 * * 1-5") // Weekdays at 9 AM
            .queue("email_queue")
            .payload(json!({
                "type": "campaign",
                "check_pending": true,
                "send_limit": 1000
            }))
            .timezone("UTC")
            .build()?)
    }

    /// Create a database maintenance job (runs weekly)
    pub fn database_maintenance() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("database_maintenance"))
            .name("Database Maintenance")
            .description("Run database optimization and maintenance tasks")
            .cron("0 1 * * 0") // Sunday at 1 AM
            .queue("maintenance_queue")
            .payload(json!({
                "type": "maintenance",
                "tasks": ["vacuum", "analyze", "reindex", "update_statistics"]
            }))
            .timezone("UTC")
            .timeout(7200) // 2 hours timeout
            .build()?)
    }

    /// Create a cache warming job (runs every 30 minutes)
    pub fn cache_warming() -> Result<Job> {
        Ok(JobBuilder::new()
            .id(JobId::parse("cache_warming"))
            .name("Cache Warming")
            .description("Pre-warm frequently accessed cache items")
            .cron("*/30 * * * *") // Every 30 minutes
            .queue("cache_queue")
            .payload(json!({
                "type": "cache_warm",
                "patterns": ["popular_products", "user_preferences", "recent_orders"]
            }))
            .timezone("UTC")
            .build()?)
    }
}

pub mod builder;
pub mod config;
pub mod cron;
pub mod error;
pub mod job;
pub mod job_executor;
pub mod job_storage;
pub mod monitoring;
pub mod pg_cron;
pub mod scheduler;
pub mod types;

// Re-export commonly used types for convenience
pub use job_storage::{JobStorage};

// Re-export main types for convenience
pub use builder::JobSchedulerBuilder;
pub use config::JobSchedulerConfig;
pub use error::{JobError, JobResult};
pub use job::{Job, JobBuilder};
pub use scheduler::JobScheduler;

#[cfg(test)]
mod tests {
    use super::*;
    use job_types::*;

    #[test]
    fn test_job_type_creation() {
        assert!(daily_backup().is_ok());
        assert!(weekly_log_cleanup().is_ok());
        assert!(hourly_data_sync().is_ok());
        assert!(monthly_report().is_ok());
        assert!(session_cleanup().is_ok());
        assert!(email_campaign_schedule().is_ok());
        assert!(database_maintenance().is_ok());
        assert!(cache_warming().is_ok());
    }

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
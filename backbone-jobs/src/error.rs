//! Error types for backbone-jobs

use thiserror::Error;

/// Result type alias for backbone-jobs operations
pub type JobResult<T> = Result<T, JobError>;

/// Main error type for job scheduling operations
#[derive(Error, Debug)]
pub enum JobError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Cron expression parsing error: {0}")]
    CronParsing(String),

    #[error("Job execution error: {0}")]
    Execution(String),

    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: String },

    #[error("Job already exists: {job_id}")]
    JobAlreadyExists { job_id: String },

    #[error("Invalid job configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Queue service error: {0}")]
    QueueService(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("UUID generation error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("Time zone error: {0}")]
    TimeZone(String),

    #[error("Scheduler not running")]
    SchedulerNotRunning,

    #[error("Scheduler already running")]
    SchedulerAlreadyRunning,

    #[error("Scheduler shutdown error: {0}")]
    SchedulerShutdown(String),

    #[error("Job timeout exceeded: {job_id} after {timeout_seconds}s")]
    JobTimeout {
        job_id: String,
        timeout_seconds: u64,
    },

    #[error("Job retry limit exceeded: {job_id} after {attempts} attempts")]
    RetryLimitExceeded {
        job_id: String,
        attempts: u32,
    },

    #[error("Invalid job status transition: {from} -> {to}")]
    InvalidStatusTransition {
        from: String,
        to: String,
    },

    #[error("pg_cron error: {0}")]
    PgCron(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP client error: {0}")]
    HttpClient(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl JobError {
    /// Create a new database error
    pub fn database(msg: &str) -> Self {
        Self::Database(sqlx::Error::Protocol(format!(
            "Database error: {}",
            msg
        )))
    }

    /// Create a new cron parsing error
    pub fn cron_parsing(msg: &str) -> Self {
        Self::CronParsing(msg.to_string())
    }

    /// Create a new job execution error
    pub fn execution(msg: &str) -> Self {
        Self::Execution(msg.to_string())
    }

    /// Create a new job not found error
    pub fn job_not_found(job_id: &str) -> Self {
        Self::JobNotFound {
            job_id: job_id.to_string(),
        }
    }

    /// Create a new job already exists error
    pub fn job_already_exists(job_id: &str) -> Self {
        Self::JobAlreadyExists {
            job_id: job_id.to_string(),
        }
    }

    /// Create a new invalid configuration error
    pub fn invalid_configuration(msg: &str) -> Self {
        Self::InvalidConfiguration(msg.to_string())
    }

    /// Create a new queue service error
    pub fn queue_service(msg: &str) -> Self {
        Self::QueueService(msg.to_string())
    }

    /// Create a new time zone error
    pub fn time_zone(msg: &str) -> Self {
        Self::TimeZone(msg.to_string())
    }

    /// Create a new job timeout error
    pub fn job_timeout(job_id: &str, timeout_seconds: u64) -> Self {
        Self::JobTimeout {
            job_id: job_id.to_string(),
            timeout_seconds,
        }
    }

    /// Create a new retry limit exceeded error
    pub fn retry_limit_exceeded(job_id: &str, attempts: u32) -> Self {
        Self::RetryLimitExceeded {
            job_id: job_id.to_string(),
            attempts,
        }
    }

    /// Create a new invalid status transition error
    pub fn invalid_status_transition(from: &str, to: &str) -> Self {
        Self::InvalidStatusTransition {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    /// Create a new pg_cron error
    pub fn pg_cron(msg: &str) -> Self {
        Self::PgCron(msg.to_string())
    }

    /// Create a new configuration error
    pub fn configuration(msg: &str) -> Self {
        Self::Configuration(msg.to_string())
    }

    /// Create a new HTTP client error
    pub fn http_client(msg: &str) -> Self {
        Self::HttpClient(msg.to_string())
    }

    /// Create a new authentication error
    pub fn authentication(msg: &str) -> Self {
        Self::Authentication(msg.to_string())
    }

    /// Create a new permission denied error
    pub fn permission_denied(msg: &str) -> Self {
        Self::PermissionDenied(msg.to_string())
    }

    /// Create a new rate limit exceeded error
    pub fn rate_limit_exceeded(msg: &str) -> Self {
        Self::RateLimitExceeded(msg.to_string())
    }

    /// Create a new validation error
    pub fn validation(msg: &str) -> Self {
        Self::Validation(msg.to_string())
    }

    /// Create a new internal error
    pub fn internal(msg: &str) -> Self {
        Self::Internal(msg.to_string())
    }

    /// Check if this is a transient error that should be retried
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            JobError::Database(_)
                | JobError::QueueService(_)
                | JobError::HttpClient(_)
                | JobError::RateLimitExceeded(_)
                | JobError::JobTimeout { .. }
                | JobError::SchedulerNotRunning
        )
    }

    /// Check if this is a permanent error that should not be retried
    pub fn is_permanent(&self) -> bool {
        !self.is_transient()
    }

    /// Get error category for monitoring and alerting
    pub fn category(&self) -> &'static str {
        match self {
            JobError::Database(_) => "database",
            JobError::CronParsing(_) => "cron",
            JobError::Execution(_) => "execution",
            JobError::JobNotFound { .. } => "not_found",
            JobError::JobAlreadyExists { .. } => "conflict",
            JobError::InvalidConfiguration(_) => "configuration",
            JobError::QueueService(_) => "queue",
            JobError::Serialization(_) => "serialization",
            JobError::Uuid(_) => "uuid",
            JobError::TimeZone(_) => "timezone",
            JobError::SchedulerNotRunning => "scheduler",
            JobError::SchedulerAlreadyRunning => "scheduler",
            JobError::SchedulerShutdown(_) => "scheduler",
            JobError::JobTimeout { .. } => "timeout",
            JobError::RetryLimitExceeded { .. } => "retry",
            JobError::InvalidStatusTransition { .. } => "state",
            JobError::PgCron(_) => "pg_cron",
            JobError::Configuration(_) => "configuration",
            JobError::Io(_) => "io",
            JobError::HttpClient(_) => "http",
            JobError::Authentication(_) => "auth",
            JobError::PermissionDenied(_) => "auth",
            JobError::RateLimitExceeded(_) => "rate_limit",
            JobError::Validation(_) => "validation",
            JobError::Internal(_) => "internal",
        }
    }

    /// Get a user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            JobError::Database(_) => "Database operation failed".to_string(),
            JobError::CronParsing(msg) => format!("Invalid cron expression: {}", msg),
            JobError::Execution(msg) => format!("Job execution failed: {}", msg),
            JobError::JobNotFound { job_id } => format!("Job '{}' not found", job_id),
            JobError::JobAlreadyExists { job_id } => format!("Job '{}' already exists", job_id),
            JobError::InvalidConfiguration(msg) => format!("Invalid configuration: {}", msg),
            JobError::QueueService(_) => "Queue service unavailable".to_string(),
            JobError::Serialization(_) => "Data serialization failed".to_string(),
            JobError::Uuid(_) => "ID generation failed".to_string(),
            JobError::TimeZone(msg) => format!("Time zone error: {}", msg),
            JobError::SchedulerNotRunning => "Job scheduler is not running".to_string(),
            JobError::SchedulerAlreadyRunning => "Job scheduler is already running".to_string(),
            JobError::SchedulerShutdown(_) => "Scheduler shutdown failed".to_string(),
            JobError::JobTimeout { job_id, timeout_seconds } => {
                format!("Job '{}' timed out after {} seconds", job_id, timeout_seconds)
            }
            JobError::RetryLimitExceeded { job_id, attempts } => {
                format!("Job '{}' failed after {} retry attempts", job_id, attempts)
            }
            JobError::InvalidStatusTransition { from, to } => {
                format!("Invalid job status transition: {} -> {}", from, to)
            }
            JobError::PgCron(msg) => format!("Database scheduling error: {}", msg),
            JobError::Configuration(msg) => format!("Configuration error: {}", msg),
            JobError::Io(msg) => format!("File system error: {}", msg),
            JobError::HttpClient(msg) => format!("HTTP client error: {}", msg),
            JobError::Authentication(msg) => format!("Authentication failed: {}", msg),
            JobError::PermissionDenied(msg) => format!("Permission denied: {}", msg),
            JobError::RateLimitExceeded(msg) => format!("Rate limit exceeded: {}", msg),
            JobError::Validation(msg) => format!("Validation failed: {}", msg),
            JobError::Internal(msg) => format!("Internal error: {}", msg),
        }
    }
}

/// Convert from backbone-queue errors
impl From<backbone_queue::QueueError> for JobError {
    fn from(error: backbone_queue::QueueError) -> Self {
        JobError::queue_service(&format!("Queue error: {}", error))
    }
}
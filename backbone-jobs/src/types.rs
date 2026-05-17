//! Common types and utilities for backbone-jobs

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Job ID wrapper type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);

impl JobId {
    /// Create a new job ID with a random UUID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create a job ID from a string
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// Create a job ID from a string reference
    pub fn parse(id: &str) -> Self {
        Self(id.to_string())
    }

    /// Get the job ID as a string reference
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for JobId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for JobId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Job status enumeration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobStatus {
    /// Job is scheduled but not yet running
    Scheduled,
    /// Job is currently running
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed but may be retried
    Failed,
    /// Job was cancelled
    Cancelled,
    /// Job is paused (will not be executed)
    Paused,
}

impl JobStatus {
    /// Check if the job is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    /// Check if the job can be executed
    pub fn is_executable(&self) -> bool {
        matches!(self, Self::Scheduled)
    }

    /// Check if the job is currently active
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Scheduled | Self::Running)
    }

    /// Get all possible status transitions
    pub fn valid_transitions(&self) -> Vec<Self> {
        match self {
            Self::Scheduled => vec![Self::Running, Self::Cancelled, Self::Paused],
            Self::Running => vec![Self::Completed, Self::Failed, Self::Cancelled],
            Self::Failed => vec![Self::Scheduled, Self::Cancelled],
            Self::Paused => vec![Self::Scheduled, Self::Cancelled],
            Self::Completed | Self::Cancelled => vec![], // Terminal states
        }
    }

    /// Check if a transition to another status is valid
    pub fn can_transition_to(&self, target: &Self) -> bool {
        self.valid_transitions().contains(target)
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scheduled => write!(f, "scheduled"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Paused => write!(f, "paused"),
        }
    }
}

/// Job priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum JobPriority {
    /// Lowest priority (background tasks)
    Low = 0,
    /// Normal priority (regular jobs)
    #[default]
    Normal = 1,
    /// High priority (important jobs)
    High = 2,
    /// Critical priority (urgent jobs)
    Critical = 3,
}

impl JobPriority {
    /// Get all priority levels
    pub fn all() -> Vec<Self> {
        vec![Self::Low, Self::Normal, Self::High, Self::Critical]
    }

    /// Get priority as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Get priority from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "normal" => Some(Self::Normal),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

impl std::fmt::Display for JobPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Job execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecutionContext {
    /// Job ID
    pub job_id: JobId,
    /// Execution attempt number
    pub attempt: u32,
    /// Maximum allowed attempts
    pub max_attempts: u32,
    /// Job timeout in seconds
    pub timeout: u64,
    /// Execution start time
    pub started_at: DateTime<Utc>,
    /// Execution metadata
    pub metadata: HashMap<String, String>,
    /// Previous execution attempts (if any)
    pub previous_attempts: Vec<JobExecutionAttempt>,
}

impl JobExecutionContext {
    /// Create a new execution context
    pub fn new(
        job_id: JobId,
        attempt: u32,
        max_attempts: u32,
        timeout: u64,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            job_id,
            attempt,
            max_attempts,
            timeout,
            started_at: Utc::now(),
            metadata,
            previous_attempts: Vec::new(),
        }
    }

    /// Add a previous execution attempt
    pub fn add_previous_attempt(&mut self, attempt: JobExecutionAttempt) {
        self.previous_attempts.push(attempt);
    }

    /// Check if this is the last allowed attempt
    pub fn is_last_attempt(&self) -> bool {
        self.attempt >= self.max_attempts
    }

    /// Check if the job has timed out
    pub fn is_timed_out(&self) -> bool {
        let elapsed = Utc::now() - self.started_at;
        elapsed > Duration::seconds(self.timeout as i64)
    }

    /// Get remaining time in seconds
    pub fn remaining_time(&self) -> i64 {
        let elapsed = Utc::now() - self.started_at;
        self.timeout as i64 - elapsed.num_seconds()
    }
}

/// Record of a previous job execution attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecutionAttempt {
    /// Attempt number
    pub attempt: u32,
    /// Start time
    pub started_at: DateTime<Utc>,
    /// End time
    pub ended_at: DateTime<Utc>,
    /// Execution result
    pub result: JobExecutionResult,
    /// Error message (if failed)
    pub error_message: Option<String>,
    /// Duration in seconds
    pub duration_seconds: i64,
}

impl JobExecutionAttempt {
    /// Create a new successful attempt record
    pub fn success(attempt: u32, started_at: DateTime<Utc>) -> Self {
        let ended_at = Utc::now();
        Self {
            attempt,
            started_at,
            ended_at,
            result: JobExecutionResult::Success,
            error_message: None,
            duration_seconds: (ended_at - started_at).num_seconds(),
        }
    }

    /// Create a new failed attempt record
    pub fn failure(
        attempt: u32,
        started_at: DateTime<Utc>,
        error_message: String,
    ) -> Self {
        let ended_at = Utc::now();
        Self {
            attempt,
            started_at,
            ended_at,
            result: JobExecutionResult::Failure,
            error_message: Some(error_message),
            duration_seconds: (ended_at - started_at).num_seconds(),
        }
    }

    /// Create a new timeout attempt record
    pub fn timeout(attempt: u32, started_at: DateTime<Utc>, timeout_seconds: u64) -> Self {
        let ended_at = started_at + Duration::seconds(timeout_seconds as i64);
        Self {
            attempt,
            started_at,
            ended_at,
            result: JobExecutionResult::Timeout,
            error_message: Some(format!("Job timed out after {} seconds", timeout_seconds)),
            duration_seconds: timeout_seconds as i64,
        }
    }
}

/// Job execution result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobExecutionResult {
    /// Job completed successfully
    Success,
    /// Job failed with an error
    Failure,
    /// Job timed out
    Timeout,
    /// Job was cancelled
    Cancelled,
}

impl JobExecutionResult {
    /// Check if the execution was successful
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Check if the execution failed
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failure | Self::Timeout)
    }

    /// Get result as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for JobExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay in seconds
    pub initial_delay: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Maximum delay in seconds
    pub max_delay: u64,
    /// Whether to use jitter
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: 60, // 1 minute
            backoff_multiplier: 2.0,
            max_delay: 3600, // 1 hour
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy
    pub fn new(max_attempts: u32, initial_delay: u64) -> Self {
        Self {
            max_attempts,
            initial_delay,
            backoff_multiplier: 2.0,
            max_delay: 3600,
            jitter: true,
        }
    }

    /// Create a policy with exponential backoff
    pub fn exponential(max_attempts: u32, initial_delay: u64) -> Self {
        Self {
            max_attempts,
            initial_delay,
            backoff_multiplier: 2.0,
            max_delay: 3600,
            jitter: true,
        }
    }

    /// Create a policy with fixed delay
    pub fn fixed(max_attempts: u32, delay: u64) -> Self {
        Self {
            max_attempts,
            initial_delay: delay,
            backoff_multiplier: 1.0,
            max_delay: delay,
            jitter: false,
        }
    }

    /// Calculate delay for a specific attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::seconds(0);
        }

        let base_delay = self.initial_delay as f64 * self.backoff_multiplier.powi(attempt as i32 - 1);
        let delay = base_delay.min(self.max_delay as f64);

        // The first retry uses the initial delay verbatim. Jitter only kicks
        // in for subsequent attempts so callers see a predictable first wait.
        let final_delay = if self.jitter && attempt > 1 {
            let jitter_range = delay * 0.25;
            delay + (rand::random::<f64>() - 0.5) * 2.0 * jitter_range
        } else {
            delay
        };

        Duration::seconds(final_delay.round() as i64)
    }

    /// Check if a retry should be attempted
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }
}

/// Job statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatistics {
    /// Total number of jobs
    pub total_jobs: u64,
    /// Number of jobs by status
    pub jobs_by_status: HashMap<String, u64>,
    /// Number of jobs by priority
    pub jobs_by_priority: HashMap<String, u64>,
    /// Average execution time in seconds
    pub avg_execution_time: f64,
    /// Success rate as percentage
    pub success_rate: f64,
    /// Number of jobs executed in the last 24 hours
    pub jobs_last_24h: u64,
    /// Number of failed jobs in the last 24 hours
    pub failed_jobs_last_24h: u64,
    /// Scheduler uptime in seconds
    pub uptime_seconds: u64,
    /// Number of active workers
    pub active_workers: u32,
    /// Number of queued jobs
    pub queued_jobs: u64,
}

impl Default for JobStatistics {
    fn default() -> Self {
        Self {
            total_jobs: 0,
            jobs_by_status: HashMap::new(),
            jobs_by_priority: HashMap::new(),
            avg_execution_time: 0.0,
            success_rate: 0.0,
            jobs_last_24h: 0,
            failed_jobs_last_24h: 0,
            uptime_seconds: 0,
            active_workers: 0,
            queued_jobs: 0,
        }
    }
}

/// Common timezone identifiers
pub mod timezones {
    /// UTC timezone
    pub const UTC: &str = "UTC";
    /// US Eastern timezone
    pub const US_EASTERN: &str = "America/New_York";
    /// US Central timezone
    pub const US_CENTRAL: &str = "America/Chicago";
    /// US Mountain timezone
    pub const US_MOUNTAIN: &str = "America/Denver";
    /// US Pacific timezone
    pub const US_PACIFIC: &str = "America/Los_Angeles";
    /// European timezone
    pub const EUROPEAN: &str = "Europe/London";
    /// Asian timezone
    pub const ASIAN: &str = "Asia/Tokyo";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_id() {
        let id = JobId::new();
        assert!(!id.as_str().is_empty());

        let id2 = JobId::parse("test-job");
        assert_eq!(id2.as_str(), "test-job");

        let id3: JobId = "another-test".into();
        assert_eq!(id3.as_str(), "another-test");
    }

    #[test]
    fn test_job_status_transitions() {
        let scheduled = JobStatus::Scheduled;
        assert!(scheduled.can_transition_to(&JobStatus::Running));
        assert!(scheduled.can_transition_to(&JobStatus::Cancelled));
        assert!(!scheduled.can_transition_to(&JobStatus::Completed));

        let running = JobStatus::Running;
        assert!(running.can_transition_to(&JobStatus::Completed));
        assert!(running.can_transition_to(&JobStatus::Failed));
        assert!(!running.can_transition_to(&JobStatus::Scheduled));
    }

    #[test]
    fn test_retry_policy() {
        let policy = RetryPolicy::exponential(3, 10);

        assert_eq!(policy.delay_for_attempt(0).num_seconds(), 0);
        assert_eq!(policy.delay_for_attempt(1).num_seconds(), 10);

        let delay2 = policy.delay_for_attempt(2).num_seconds();
        assert!(delay2 >= 15 && delay2 <= 25); // 20 ± 25% jitter

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn test_job_execution_context() {
        let mut ctx = JobExecutionContext::new(
            JobId::parse("test-job"),
            1,
            3,
            300,
            HashMap::new(),
        );

        assert!(!ctx.is_last_attempt());
        assert!(!ctx.is_timed_out());
        assert!(ctx.remaining_time() > 0);

        ctx.attempt = 3;
        assert!(ctx.is_last_attempt());
    }
}
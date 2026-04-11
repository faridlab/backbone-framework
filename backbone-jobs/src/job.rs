//! Job definition and builder

use crate::error::{JobError, JobResult};
use crate::types::{JobId, JobPriority, JobStatus, RetryPolicy};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A scheduled job with all its configuration and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job identifier
    pub id: JobId,
    /// Human-readable job name
    pub name: String,
    /// Optional job description
    pub description: Option<String>,
    /// Cron expression for scheduling
    pub cron_expression: String,
    /// Timezone for the cron expression
    pub timezone: String,
    /// Target queue for job execution
    pub queue: String,
    /// Job payload data
    pub payload: serde_json::Value,
    /// Current job status
    pub status: JobStatus,
    /// Job execution priority
    pub priority: JobPriority,
    /// Job execution timeout in seconds
    pub timeout: u64,
    /// Maximum retry attempts
    pub max_attempts: u32,
    /// Retry policy configuration
    pub retry_policy: RetryPolicy,
    /// Additional job metadata
    pub metadata: HashMap<String, String>,
    /// Job creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Last execution timestamp
    pub last_run_time: Option<DateTime<Utc>>,
    /// Next scheduled execution time
    pub next_run_time: Option<DateTime<Utc>>,
    /// Total number of times the job has run
    pub run_count: u32,
    /// Number of successful executions
    pub success_count: u32,
    /// Number of failed executions
    pub failure_count: u32,
}

impl Job {
    /// Create a new job with minimal required fields
    pub fn new(
        id: JobId,
        name: String,
        cron_expression: String,
        queue: String,
        payload: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            description: None,
            cron_expression,
            timezone: "UTC".to_string(),
            queue,
            payload,
            status: JobStatus::Scheduled,
            priority: JobPriority::Normal,
            timeout: 300, // 5 minutes default
            max_attempts: 5,
            retry_policy: RetryPolicy::default(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            last_run_time: None,
            next_run_time: None,
            run_count: 0,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Check if the job is ready to be executed
    pub fn is_ready_to_run(&self) -> bool {
        if !self.is_scheduled() {
            return false;
        }

        if let Some(next_run) = self.next_run_time {
            Utc::now() >= next_run
        } else {
            false
        }
    }

    /// Check if the job is in a scheduled state
    pub fn is_scheduled(&self) -> bool {
        self.status == JobStatus::Scheduled
    }

    /// Check if the job is currently running
    pub fn is_running(&self) -> bool {
        self.status == JobStatus::Running
    }

    /// Check if the job can be retried after a failure
    pub fn can_retry(&self) -> bool {
        self.status == JobStatus::Failed && self.failure_count < self.max_attempts
    }

    /// Get the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.run_count == 0 {
            0.0
        } else {
            (self.success_count as f64 / self.run_count as f64) * 100.0
        }
    }

    /// Mark the job as running and update timestamps
    pub fn mark_as_running(&mut self) -> JobResult<()> {
        if self.status != JobStatus::Scheduled {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "running",
            ));
        }

        self.status = JobStatus::Running;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark the job as completed successfully
    pub fn mark_as_completed(&mut self) -> JobResult<()> {
        if self.status != JobStatus::Running {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "completed",
            ));
        }

        self.status = JobStatus::Completed;
        self.last_run_time = Some(Utc::now());
        self.updated_at = Utc::now();
        self.run_count += 1;
        self.success_count += 1;
        Ok(())
    }

    /// Mark the job as failed
    pub fn mark_as_failed(&mut self) -> JobResult<()> {
        if self.status != JobStatus::Running {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "failed",
            ));
        }

        self.status = JobStatus::Failed;
        self.last_run_time = Some(Utc::now());
        self.updated_at = Utc::now();
        self.run_count += 1;
        self.failure_count += 1;
        Ok(())
    }

    /// Reset the job to scheduled state (for retry)
    pub fn reset_for_retry(&mut self) -> JobResult<()> {
        if self.status != JobStatus::Failed {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "scheduled",
            ));
        }

        self.status = JobStatus::Scheduled;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Cancel the job
    pub fn cancel(&mut self) -> JobResult<()> {
        if self.status == JobStatus::Completed || self.status == JobStatus::Cancelled {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "cancelled",
            ));
        }

        self.status = JobStatus::Cancelled;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Pause the job
    pub fn pause(&mut self) -> JobResult<()> {
        if self.status == JobStatus::Completed || self.status == JobStatus::Cancelled {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "paused",
            ));
        }

        self.status = JobStatus::Paused;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Resume a paused job
    pub fn resume(&mut self) -> JobResult<()> {
        if self.status != JobStatus::Paused {
            return Err(JobError::invalid_status_transition(
                &self.status.to_string(),
                "scheduled",
            ));
        }

        self.status = JobStatus::Scheduled;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Update the next run time
    pub fn set_next_run_time(&mut self, next_run: DateTime<Utc>) {
        self.next_run_time = Some(next_run);
        self.updated_at = Utc::now();
    }

    /// Add metadata
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.updated_at = Utc::now();
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Validate the job configuration
    pub fn validate(&self) -> JobResult<()> {
        // Validate cron expression
        if self.cron_expression.is_empty() {
            return Err(JobError::validation("Cron expression cannot be empty"));
        }

        // Basic cron expression validation (more thorough validation would be in the scheduler)
        let parts: Vec<&str> = self.cron_expression.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(JobError::validation(
                "Cron expression must have exactly 5 fields",
            ));
        }

        // Validate queue name
        if self.queue.is_empty() {
            return Err(JobError::validation("Queue name cannot be empty"));
        }

        // Validate timeout
        if self.timeout == 0 {
            return Err(JobError::validation("Timeout must be greater than 0"));
        }

        // Validate max attempts
        if self.max_attempts == 0 {
            return Err(JobError::validation("Max attempts must be greater than 0"));
        }

        Ok(())
    }
}

/// Builder for creating jobs with a fluent interface
pub struct JobBuilder {
    id: Option<JobId>,
    name: String,
    description: Option<String>,
    cron_expression: String,
    timezone: String,
    queue: String,
    payload: serde_json::Value,
    priority: JobPriority,
    timeout: u64,
    max_attempts: u32,
    retry_policy: RetryPolicy,
    metadata: HashMap<String, String>,
}

impl JobBuilder {
    /// Create a new job builder
    pub fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: None,
            cron_expression: String::new(),
            timezone: "UTC".to_string(),
            queue: String::new(),
            payload: serde_json::Value::Null,
            priority: JobPriority::Normal,
            timeout: 300,
            max_attempts: 5,
            retry_policy: RetryPolicy::default(),
            metadata: HashMap::new(),
        }
    }

    /// Set the job ID (will generate one if not set)
    pub fn id(mut self, id: JobId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the job name
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = name.into();
        self
    }

    /// Set the job description
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the cron expression
    pub fn cron<S: Into<String>>(mut self, cron_expression: S) -> Self {
        self.cron_expression = cron_expression.into();
        self
    }

    /// Set the timezone
    pub fn timezone<S: Into<String>>(mut self, timezone: S) -> Self {
        self.timezone = timezone.into();
        self
    }

    /// Set the target queue
    pub fn queue<S: Into<String>>(mut self, queue: S) -> Self {
        self.queue = queue.into();
        self
    }

    /// Set the job payload
    pub fn payload<T: Into<serde_json::Value>>(mut self, payload: T) -> Self {
        self.payload = payload.into();
        self
    }

    /// Set the job priority
    pub fn priority(mut self, priority: JobPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the timeout in seconds
    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum retry attempts
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Set the retry policy
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Add metadata
    pub fn metadata<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add multiple metadata items
    pub fn metadata_map(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata.extend(metadata);
        self
    }

    /// Build the job
    pub fn build(self) -> JobResult<Job> {
        let id = self.id.unwrap_or_default();

        let mut job = Job::new(id, self.name, self.cron_expression, self.queue, self.payload);
        job.description = self.description;
        job.timezone = self.timezone;
        job.priority = self.priority;
        job.timeout = self.timeout;
        job.max_attempts = self.max_attempts;
        job.retry_policy = self.retry_policy;
        job.metadata = self.metadata;

        // Validate the job
        job.validate()?;

        Ok(job)
    }
}

impl Default for JobBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_job_builder() {
        let job = JobBuilder::new()
            .name("Test Job")
            .description("A test job")
            .cron("0 12 * * *")
            .queue("test_queue")
            .payload(json!({"test": true}))
            .priority(JobPriority::High)
            .timeout(600)
            .max_attempts(3)
            .metadata("env", "test")
            .build()
            .unwrap();

        assert_eq!(job.name, "Test Job");
        assert_eq!(job.description, Some("A test job".to_string()));
        assert_eq!(job.cron_expression, "0 12 * * *");
        assert_eq!(job.queue, "test_queue");
        assert_eq!(job.priority, JobPriority::High);
        assert_eq!(job.timeout, 600);
        assert_eq!(job.max_attempts, 3);
        assert_eq!(job.get_metadata("env"), Some(&"test".to_string()));
    }

    #[test]
    fn test_job_validation() {
        let mut job = Job::new(
            JobId::new(),
            "Test".to_string(),
            "invalid cron".to_string(),
            "test".to_string(),
            json!({}),
        );

        assert!(job.validate().is_err());

        job.cron_expression = "0 12 * * *".to_string();
        job.queue = "".to_string(); // Invalid queue
        assert!(job.validate().is_err());

        job.queue = "test_queue".to_string();
        assert!(job.validate().is_ok());
    }

    #[test]
    fn test_job_state_transitions() {
        let mut job = Job::new(
            JobId::new(),
            "Test".to_string(),
            "0 12 * * *".to_string(),
            "test".to_string(),
            json!({}),
        );

        // Should be able to start running
        assert!(job.mark_as_running().is_ok());
        assert_eq!(job.status, JobStatus::Running);

        // Should not be able to mark as running again
        assert!(job.mark_as_running().is_err());

        // Should be able to complete
        assert!(job.mark_as_completed().is_ok());
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.run_count, 1);
        assert_eq!(job.success_count, 1);

        // Should not be able to complete again
        assert!(job.mark_as_completed().is_err());
    }

    #[test]
    fn test_job_retry_logic() {
        let mut job = Job::new(
            JobId::new(),
            "Test".to_string(),
            "0 12 * * *".to_string(),
            "test".to_string(),
            json!({}),
        );

        job.max_attempts = 3;

        // Fail once
        job.mark_as_running().unwrap();
        job.mark_as_failed().unwrap();
        assert!(job.can_retry());

        // Fail twice
        job.reset_for_retry().unwrap();
        job.mark_as_running().unwrap();
        job.mark_as_failed().unwrap();
        assert!(job.can_retry());

        // Fail third time (max attempts reached)
        job.reset_for_retry().unwrap();
        job.mark_as_running().unwrap();
        job.mark_as_failed().unwrap();
        assert!(!job.can_retry());
    }

    #[test]
    fn test_success_rate() {
        let mut job = Job::new(
            JobId::new(),
            "Test".to_string(),
            "0 12 * * *".to_string(),
            "test".to_string(),
            json!({}),
        );

        assert_eq!(job.success_rate(), 0.0);

        // Complete successfully
        job.mark_as_running().unwrap();
        job.mark_as_completed().unwrap();
        assert_eq!(job.success_rate(), 100.0);

        // Fail once
        job.reset_for_retry().unwrap();
        job.mark_as_running().unwrap();
        job.mark_as_failed().unwrap();
        assert_eq!(job.success_rate(), 50.0); // 1 success out of 2 runs
    }
}
//! Job storage trait definition

pub mod in_memory;

use crate::error::JobResult;
use crate::job::Job;
use crate::types::{JobExecutionAttempt, JobId, JobStatistics, JobStatus};
use async_trait::async_trait;

/// Trait for job storage operations
#[async_trait]
pub trait JobStorage: Send + Sync {
    /// Store a new job
    async fn create_job(&self, job: &Job) -> JobResult<()>;

    /// Update an existing job
    async fn update_job(&self, job: &Job) -> JobResult<()>;

    /// Get a job by ID
    async fn get_job(&self, job_id: &JobId) -> JobResult<Option<Job>>;

    /// Delete a job by ID
    async fn delete_job(&self, job_id: &JobId) -> JobResult<()>;

    /// List all jobs
    async fn list_jobs(&self) -> JobResult<Vec<Job>>;

    /// List jobs by status
    async fn list_jobs_by_status(&self, status: JobStatus) -> JobResult<Vec<Job>>;

    /// Get jobs scheduled to run before the given time
    async fn get_pending_jobs(&self, before: chrono::DateTime<chrono::Utc>) -> JobResult<Vec<Job>>;

    /// Update job status
    async fn update_job_status(&self, job_id: &JobId, status: JobStatus) -> JobResult<()>;

    /// Update next run time for a job
    async fn update_next_run_time(&self, job_id: &JobId, next_run: chrono::DateTime<chrono::Utc>) -> JobResult<()>;

    /// Record job execution attempt
    async fn record_execution_attempt(&self, job_id: &JobId, attempt: &JobExecutionAttempt) -> JobResult<()>;

    /// Get job execution history
    async fn get_execution_history(&self, job_id: &JobId, limit: Option<u32>) -> JobResult<Vec<JobExecutionAttempt>>;

    /// Get job statistics
    async fn get_statistics(&self) -> JobResult<JobStatistics>;

    /// Clean up old execution attempts
    async fn cleanup_old_attempts(&self, older_than: chrono::DateTime<chrono::Utc>) -> JobResult<u64>;
}

// Re-export for convenience
pub use in_memory::InMemoryJobStorage;
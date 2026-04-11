//! Main job scheduler implementation

use crate::cron::CronScheduler;
use crate::error::{JobError, JobResult};
use crate::job::Job;
use crate::job_executor::{JobExecutor, JobExecutionCallback};
use crate::job_storage::JobStorage;
use crate::types::{JobId, JobStatistics, JobStatus};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

/// Main job scheduler that coordinates all operations
pub struct JobScheduler {
    storage: Arc<dyn JobStorage>,
    executor: JobExecutor,
    config: SchedulerConfig,
    state: Arc<RwLock<SchedulerState>>,
    callback: Option<Arc<dyn JobExecutionCallback>>,
}

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Polling interval for checking pending jobs
    pub poll_interval: Duration,
    /// Maximum number of concurrent jobs
    pub max_concurrent_jobs: usize,
    /// Default job timeout
    pub default_timeout: Duration,
    /// Whether to automatically start jobs on schedule
    pub auto_start: bool,
    /// Time zone for cron expressions
    pub default_timezone: String,
    /// Whether to cleanup old execution attempts
    pub cleanup_old_attempts: bool,
    /// Age of execution attempts to cleanup
    pub cleanup_attempts_older_than_days: i64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(60),
            max_concurrent_jobs: 10,
            default_timeout: Duration::from_secs(300),
            auto_start: true,
            default_timezone: "UTC".to_string(),
            cleanup_old_attempts: true,
            cleanup_attempts_older_than_days: 30,
        }
    }
}

/// Internal scheduler state
#[derive(Debug)]
struct SchedulerState {
    is_running: bool,
    start_time: Option<DateTime<Utc>>,
    last_cleanup_time: Option<DateTime<Utc>>,
    jobs_processed: u64,
    jobs_failed: u64,
    active_jobs: std::collections::HashSet<JobId>,
}

impl JobScheduler {
    /// Create a new job scheduler
    pub fn new(
        storage: Arc<dyn JobStorage>,
        executor: JobExecutor,
        config: SchedulerConfig,
    ) -> Self {
        Self {
            storage,
            executor,
            config,
            state: Arc::new(RwLock::new(SchedulerState {
                is_running: false,
                start_time: None,
                last_cleanup_time: None,
                jobs_processed: 0,
                jobs_failed: 0,
                active_jobs: std::collections::HashSet::new(),
            })),
            callback: None,
        }
    }

    /// Set execution callback
    pub fn with_callback(mut self, callback: Arc<dyn JobExecutionCallback>) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Start the scheduler
    pub async fn start(&self) -> JobResult<()> {
        let mut state = self.state.write().await;

        if state.is_running {
            return Err(JobError::execution("Scheduler is already running"));
        }

        state.is_running = true;
        state.start_time = Some(Utc::now());

        info!("Job scheduler started with configuration: {:?}", self.config);
        info!("Poll interval: {:?}", self.config.poll_interval);
        info!("Max concurrent jobs: {}", self.config.max_concurrent_jobs);

        drop(state);

        if self.config.auto_start {
            self.start_background_tasks().await?;
        }

        Ok(())
    }

    /// Stop the scheduler
    pub async fn stop(&self) -> JobResult<()> {
        let mut state = self.state.write().await;

        if !state.is_running {
            return Err(JobError::execution("Scheduler is not running"));
        }

        info!("Stopping job scheduler...");

        state.is_running = false;

        // Wait for active jobs to complete (with timeout)
        let wait_timeout = Duration::from_secs(30);
        let start_time = Utc::now();

        while !state.active_jobs.is_empty() {
            drop(state);
            sleep(Duration::from_secs(1)).await;
            state = self.state.write().await;

            if Utc::now() - start_time > chrono::Duration::from_std(wait_timeout).unwrap_or_else(|_| chrono::Duration::seconds(30)) {
                warn!("Timeout waiting for {} active jobs to complete", state.active_jobs.len());
                break;
            }
        }

        info!("Job scheduler stopped");
        Ok(())
    }

    /// Schedule a new job
    pub async fn schedule_job(&self, job: Job) -> JobResult<()> {
        // Validate the job
        job.validate()?;

        // Check if job already exists
        if let Some(existing) = self.storage.get_job(&job.id).await? {
            return Err(JobError::job_already_exists(existing.id.as_str()));
        }

        // Calculate next run time
        let next_run = self.calculate_next_run_time(&job).await?;

        let mut job_with_next = job.clone();
        job_with_next.next_run_time = Some(next_run);

        // Store the job
        self.storage.create_job(&job_with_next).await?;

        info!("Scheduled job '{}' with next run time: {}", job.name, next_run);
        Ok(())
    }

    /// Unschedule (delete) a job
    pub async fn unschedule_job(&self, job_id: &JobId) -> JobResult<()> {
        let job = self.storage.get_job(job_id).await?
            .ok_or_else(|| JobError::job_not_found(job_id.as_str()))?;

        // Check if job is currently running
        let state = self.state.read().await;
        if state.active_jobs.contains(job_id) {
            return Err(JobError::execution("Cannot delete job that is currently running"));
        }
        drop(state);

        self.storage.delete_job(job_id).await?;
        info!("Unscheduled job: {}", job.name);
        Ok(())
    }

    /// Update a job
    pub async fn update_job(&self, job: &Job) -> JobResult<()> {
        // Validate the job
        job.validate()?;

        // Check if job exists
        let existing = self.storage.get_job(&job.id).await?
            .ok_or_else(|| JobError::job_not_found(job.id.as_str()))?;

        // Don't allow updating running jobs
        let state = self.state.read().await;
        if state.active_jobs.contains(&job.id) {
            return Err(JobError::execution("Cannot update job that is currently running"));
        }
        drop(state);

        // Calculate new next run time if cron expression changed
        let next_run = if job.cron_expression != existing.cron_expression {
            Some(self.calculate_next_run_time(job).await?)
        } else {
            job.next_run_time
        };

        let mut updated_job = job.clone();
        updated_job.next_run_time = next_run;

        self.storage.update_job(&updated_job).await?;
        info!("Updated job: {}", job.name);
        Ok(())
    }

    /// Get a job by ID
    pub async fn get_job(&self, job_id: &JobId) -> JobResult<Option<Job>> {
        self.storage.get_job(job_id).await
    }

    /// List all jobs
    pub async fn list_jobs(&self) -> JobResult<Vec<Job>> {
        self.storage.list_jobs().await
    }

    /// List jobs by status
    pub async fn list_jobs_by_status(&self, status: JobStatus) -> JobResult<Vec<Job>> {
        self.storage.list_jobs_by_status(status).await
    }

    /// Pause a job
    pub async fn pause_job(&self, job_id: &JobId) -> JobResult<()> {
        let mut job = self.storage.get_job(job_id).await?
            .ok_or_else(|| JobError::job_not_found(job_id.as_str()))?;

        job.pause()?;
        self.storage.update_job(&job).await?;

        info!("Paused job: {}", job.name);
        Ok(())
    }

    /// Resume a paused job
    pub async fn resume_job(&self, job_id: &JobId) -> JobResult<()> {
        let mut job = self.storage.get_job(job_id).await?
            .ok_or_else(|| JobError::job_not_found(job_id.as_str()))?;

        job.resume()?;

        // Calculate next run time
        let next_run = self.calculate_next_run_time(&job).await?;
        job.next_run_time = Some(next_run);

        self.storage.update_job(&job).await?;

        info!("Resumed job: {}", job.name);
        Ok(())
    }

    /// Cancel a job
    pub async fn cancel_job(&self, job_id: &JobId) -> JobResult<()> {
        let mut job = self.storage.get_job(job_id).await?
            .ok_or_else(|| JobError::job_not_found(job_id.as_str()))?;

        job.cancel()?;
        self.storage.update_job(&job).await?;

        info!("Cancelled job: {}", job.name);
        Ok(())
    }

    /// Trigger immediate execution of a job
    pub async fn trigger_job(&self, job_id: &JobId) -> JobResult<()> {
        let job = self.storage.get_job(job_id).await?
            .ok_or_else(|| JobError::job_not_found(job_id.as_str()))?;

        if !job.is_scheduled() {
            return Err(JobError::execution("Job is not in a schedulable state"));
        }

        // Execute the job immediately
        self.execute_single_job(&job).await
    }

    /// Get scheduler statistics
    pub async fn get_statistics(&self) -> JobResult<JobStatistics> {
        let storage_stats = self.storage.get_statistics().await?;
        let state = self.state.read().await;

        let mut stats = storage_stats;
        stats.uptime_seconds = state.start_time
            .map(|start| (Utc::now() - start).num_seconds() as u64)
            .unwrap_or(0);
        stats.active_workers = state.active_jobs.len() as u32;

        Ok(stats)
    }

    /// Get execution history for a job
    pub async fn get_job_history(&self, job_id: &JobId, limit: Option<u32>) -> JobResult<Vec<crate::types::JobExecutionAttempt>> {
        self.storage.get_execution_history(job_id, limit).await
    }

    /// Calculate the next run time for a job
    async fn calculate_next_run_time(&self, job: &Job) -> JobResult<DateTime<Utc>> {
        let cron_scheduler = CronScheduler::new(&job.cron_expression, &job.timezone)?;
        let now = Utc::now();
        cron_scheduler.next_after(now)
    }

    /// Execute a single job
    async fn execute_single_job(&self, job: &Job) -> JobResult<()> {
        let mut job_clone = job.clone();

        // Mark as running
        job_clone.mark_as_running()?;
        self.storage.update_job(&job_clone).await?;

        // Add to active jobs
        {
            let mut state = self.state.write().await;
            state.active_jobs.insert(job.id.clone());
        }

        let execution_result = match &self.callback {
            Some(_callback) => {
                let callback_executor = crate::job_executor::CallbackJobExecutor::with_default_callback();
                // Execute with callback logic
                callback_executor.execute_job_callback(&job_clone).await
            }
            None => {
                self.executor.execute_job(&job_clone).await
            }
        };

        // Update job status based on execution result
        match execution_result {
            Ok(()) => {
                job_clone.mark_as_completed()?;
                self.storage.update_job(&job_clone).await?;

                // Calculate next run time
                if let Ok(next_run) = self.calculate_next_run_time(&job_clone).await {
                    job_clone.next_run_time = Some(next_run);
                    self.storage.update_job(&job_clone).await?;
                }

                // Update statistics
                {
                    let mut state = self.state.write().await;
                    state.jobs_processed += 1;
                }

                debug!("Job {} completed successfully", job.name);
            }
            Err(e) => {
                job_clone.mark_as_failed()?;
                self.storage.update_job(&job_clone).await?;

                // Check if job can be retried
                if job_clone.can_retry() {
                    let retry_delay = job_clone.retry_policy.delay_for_attempt(job_clone.run_count);
                    let next_run = Utc::now() + retry_delay;
                    job_clone.set_next_run_time(next_run);
                    job_clone.reset_for_retry()?;
                    self.storage.update_job(&job_clone).await?;

                    info!("Job {} failed, will retry in {:?}: {}", job.name, retry_delay, e);
                } else {
                    info!("Job {} failed permanently: {}", job.name, e);
                }

                // Update statistics
                {
                    let mut state = self.state.write().await;
                    state.jobs_failed += 1;
                }

                error!("Job {} failed: {}", job.name, e);
            }
        }

        // Remove from active jobs
        {
            let mut state = self.state.write().await;
            state.active_jobs.remove(&job.id);
        }

        Ok(())
    }

    /// Start background tasks
    async fn start_background_tasks(&self) -> JobResult<()> {
        let scheduler = self.clone();

        // Job processing task
        tokio::spawn(async move {
            let mut interval = interval(scheduler.config.poll_interval);

            loop {
                let state = scheduler.state.read().await;
                if !state.is_running {
                    break;
                }
                drop(state);

                interval.tick().await;

                if let Err(e) = scheduler.process_pending_jobs().await {
                    error!("Error processing pending jobs: {}", e);
                }

                if let Err(e) = scheduler.cleanup_old_attempts().await {
                    error!("Error cleaning up old attempts: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Process pending jobs
    async fn process_pending_jobs(&self) -> JobResult<()> {
        let now = Utc::now();
        let pending_jobs = self.storage.get_pending_jobs(now).await?;

        if pending_jobs.is_empty() {
            return Ok(());
        }

        debug!("Processing {} pending jobs", pending_jobs.len());

        // Limit concurrent execution
        let state = self.state.read().await;
        let available_slots = self.config.max_concurrent_jobs.saturating_sub(state.active_jobs.len());
        drop(state);

        if available_slots == 0 {
            debug!("No available slots for job execution");
            return Ok(());
        }

        let jobs_to_execute = pending_jobs.into_iter().take(available_slots);

        for job in jobs_to_execute {
            if let Err(e) = self.execute_single_job(&job).await {
                error!("Failed to execute job {}: {}", job.name, e);
            }
        }

        Ok(())
    }

    /// Clean up old execution attempts
    async fn cleanup_old_attempts(&self) -> JobResult<()> {
        if !self.config.cleanup_old_attempts {
            return Ok(());
        }

        let state = self.state.read().await;
        let should_cleanup = state.last_cleanup_time
            .map(|last| Utc::now() - last > chrono::Duration::seconds(24 * 3600)) // 24 hours
            .unwrap_or(true);
        drop(state);

        if !should_cleanup {
            return Ok(());
        }

        let cutoff_date = Utc::now() - chrono::Duration::days(self.config.cleanup_attempts_older_than_days);
        let cleaned_count = self.storage.cleanup_old_attempts(cutoff_date).await?;

        if cleaned_count > 0 {
            info!("Cleaned up {} old execution attempts", cleaned_count);
        }

        // Update last cleanup time
        {
            let mut state = self.state.write().await;
            state.last_cleanup_time = Some(Utc::now());
        }

        Ok(())
    }
}

impl Clone for JobScheduler {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            executor: self.executor.clone(),
            config: self.config.clone(),
            state: self.state.clone(),
            callback: self.callback.clone(),
        }
    }
}

/// Builder for JobScheduler
pub struct JobSchedulerBuilder {
    storage: Option<Arc<dyn JobStorage>>,
    config: SchedulerConfig,
    callback: Option<Arc<dyn JobExecutionCallback>>,
}

impl JobSchedulerBuilder {
    /// Create a new scheduler builder
    pub fn new() -> Self {
        Self {
            storage: None,
            config: SchedulerConfig::default(),
            callback: None,
        }
    }

    /// Set the job storage
    pub fn with_storage(mut self, storage: Arc<dyn JobStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Set the configuration
    pub fn with_config(mut self, config: SchedulerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the polling interval
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.config.poll_interval = interval;
        self
    }

    /// Set the maximum concurrent jobs
    pub fn max_concurrent_jobs(mut self, max: usize) -> Self {
        self.config.max_concurrent_jobs = max;
        self
    }

    /// Set the default timeout
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.config.default_timeout = timeout;
        self
    }

    /// Set the execution callback
    pub fn with_callback(mut self, callback: Arc<dyn JobExecutionCallback>) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Build the scheduler
    pub fn build(self) -> JobResult<JobScheduler> {
        let storage = self.storage.ok_or_else(|| {
            JobError::configuration("Job storage is required")
        })?;

        // This would need a queue service - for now we'll create a placeholder
        // In a real implementation, you'd pass this in the builder
        let queue_service = std::sync::Arc::new(crate::job_executor::MockQueueService::default());
        let executor = JobExecutor::new(queue_service, self.config.default_timeout);

        Ok(JobScheduler::new(storage, executor, self.config))
    }
}

impl Default for JobSchedulerBuilder {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_storage::InMemoryJobStorage;

    #[tokio::test]
    async fn test_scheduler_lifecycle() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let queue_service = Arc::new(crate::job_executor::MockQueueService);
        let executor = JobExecutor::new(queue_service, Duration::from_secs(300));
        let config = SchedulerConfig::default();

        let scheduler = JobScheduler::new(storage, executor, config);

        // Should be able to start
        assert!(scheduler.start().await.is_ok());

        // Should not be able to start again
        assert!(scheduler.start().await.is_err());

        // Should be able to stop
        assert!(scheduler.stop().await.is_ok());

        // Should not be able to stop again
        assert!(scheduler.stop().await.is_err());
    }
}
//! In-memory implementation of job storage (for testing)

use crate::error::{JobError, JobResult};
use crate::job::Job;
use crate::job_storage::JobStorage;
use crate::types::{JobExecutionAttempt, JobId, JobStatistics, JobStatus};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory job storage for testing
pub struct InMemoryJobStorage {
    jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    execution_history: Arc<RwLock<HashMap<JobId, Vec<JobExecutionAttempt>>>>,
}

impl InMemoryJobStorage {
    /// Create a new in-memory job storage
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            execution_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryJobStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl JobStorage for InMemoryJobStorage {
    async fn create_job(&self, job: &Job) -> JobResult<()> {
        let mut jobs = self.jobs.write().await;
        if jobs.contains_key(&job.id) {
            return Err(JobError::job_already_exists(job.id.as_str()));
        }
        jobs.insert(job.id.clone(), job.clone());
        Ok(())
    }

    async fn update_job(&self, job: &Job) -> JobResult<()> {
        let mut jobs = self.jobs.write().await;
        if !jobs.contains_key(&job.id) {
            return Err(JobError::job_not_found(job.id.as_str()));
        }
        jobs.insert(job.id.clone(), job.clone());
        Ok(())
    }

    async fn get_job(&self, job_id: &JobId) -> JobResult<Option<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.get(job_id).cloned())
    }

    async fn delete_job(&self, job_id: &JobId) -> JobResult<()> {
        let mut jobs = self.jobs.write().await;
        match jobs.remove(job_id) {
            Some(_) => {
                // Also remove execution history
                let mut history = self.execution_history.write().await;
                history.remove(job_id);
                Ok(())
            }
            None => Err(JobError::job_not_found(job_id.as_str())),
        }
    }

    async fn list_jobs(&self) -> JobResult<Vec<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.values().cloned().collect())
    }

    async fn list_jobs_by_status(&self, status: JobStatus) -> JobResult<Vec<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs
            .values()
            .filter(|job| job.status == status)
            .cloned()
            .collect())
    }

    async fn get_pending_jobs(&self, before: DateTime<Utc>) -> JobResult<Vec<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs
            .values()
            .filter(|job| {
                job.is_scheduled()
                    && job.next_run_time.is_some_and(|next| next <= before)
            })
            .cloned()
            .collect())
    }

    async fn update_job_status(&self, job_id: &JobId, status: JobStatus) -> JobResult<()> {
        let mut jobs = self.jobs.write().await;
        match jobs.get_mut(job_id) {
            Some(job) => {
                job.status = status;
                job.updated_at = Utc::now();
                Ok(())
            }
            None => Err(JobError::job_not_found(job_id.as_str())),
        }
    }

    async fn update_next_run_time(&self, job_id: &JobId, next_run: DateTime<Utc>) -> JobResult<()> {
        let mut jobs = self.jobs.write().await;
        match jobs.get_mut(job_id) {
            Some(job) => {
                job.next_run_time = Some(next_run);
                job.updated_at = Utc::now();
                Ok(())
            }
            None => Err(JobError::job_not_found(job_id.as_str())),
        }
    }

    async fn record_execution_attempt(&self, job_id: &JobId, attempt: &JobExecutionAttempt) -> JobResult<()> {
        let mut history = self.execution_history.write().await;
        let attempts = history.entry(job_id.clone()).or_insert_with(Vec::new);
        attempts.push(attempt.clone());
        Ok(())
    }

    async fn get_execution_history(&self, job_id: &JobId, limit: Option<u32>) -> JobResult<Vec<JobExecutionAttempt>> {
        let history = self.execution_history.read().await;
        let attempts = history.get(job_id).cloned().unwrap_or_default();
        match limit {
            Some(limit) => Ok(attempts.into_iter().rev().take(limit as usize).collect()),
            None => Ok(attempts.into_iter().rev().collect()),
        }
    }

    async fn get_statistics(&self) -> JobResult<JobStatistics> {
        let jobs = self.jobs.read().await;
        let history = self.execution_history.read().await;
        let mut stats = JobStatistics {
            total_jobs: jobs.len() as u64,
            ..Default::default()
        };

        let mut total_successes = 0;
        let mut total_runs = 0;

        for job in jobs.values() {
            // Count by status
            let status_key = job.status.to_string();
            *stats.jobs_by_status.entry(status_key).or_insert(0) += 1;

            // Count by priority
            let priority_key = job.priority.to_string();
            *stats.jobs_by_priority.entry(priority_key).or_insert(0) += 1;

            // Track execution counts from job status
            if job.status == JobStatus::Completed {
                total_successes += 1;
            }
            if job.status == JobStatus::Completed || job.status == JobStatus::Failed {
                total_runs += 1;
            }
        }

        // Calculate success rate
        if total_runs > 0 {
            stats.success_rate = (total_successes as f64 / total_runs as f64) * 100.0;
        }

        // Calculate average execution time from history
        let mut execution_times = Vec::new();
        for attempts in history.values() {
            for attempt in attempts {
                if attempt.result.is_success() {
                    execution_times.push(attempt.duration_seconds as f64);
                }
            }
        }

        if !execution_times.is_empty() {
            stats.avg_execution_time = execution_times.iter().sum::<f64>() / execution_times.len() as f64;
        }

        Ok(stats)
    }

    async fn cleanup_old_attempts(&self, older_than: DateTime<Utc>) -> JobResult<u64> {
        let mut history = self.execution_history.write().await;
        let mut removed_count = 0;

        for attempts in history.values_mut() {
            attempts.retain(|attempt| attempt.started_at >= older_than);
            removed_count += attempts.len();
        }

        Ok(removed_count as u64)
    }
}
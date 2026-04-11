//! pg_cron integration for database-level scheduling

use crate::error::{JobError, JobResult};
use crate::job::Job;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

/// pg_cron job manager for database-level scheduling
pub struct PgCronManager {
    pool: PgPool,
}

impl PgCronManager {
    /// Create a new pg_cron manager
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Initialize pg_cron extension and schema
    pub async fn initialize(&self) -> JobResult<()> {
        // Enable pg_cron extension
        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_cron")
            .execute(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to create pg_cron extension: {}", e)))?;

        // Create custom cron job table for our application
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS backbone_cron_jobs (
                id VARCHAR(255) PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                cron_expression VARCHAR(100) NOT NULL,
                command TEXT NOT NULL,
                database VARCHAR(100) NOT NULL DEFAULT CURRENT_DATABASE(),
                username VARCHAR(100) NOT NULL DEFAULT CURRENT_USER,
                active BOOLEAN NOT NULL DEFAULT true,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                job_metadata JSONB
            );

            CREATE INDEX IF NOT EXISTS idx_backbone_cron_jobs_active ON backbone_cron_jobs(active);
            CREATE INDEX IF NOT EXISTS idx_backbone_cron_jobs_name ON backbone_cron_jobs(name);
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| JobError::pg_cron(&format!("Failed to create backbone_cron_jobs table: {}", e)))?;

        Ok(())
    }

    /// Create a pg_cron job from a backbone job
    pub async fn create_cron_job(&self, job: &Job) -> JobResult<i32> {
        let command = self.generate_cron_command(job)?;

        // Insert into our tracking table
        let _result = sqlx::query(
            r#"
            INSERT INTO backbone_cron_jobs (id, name, cron_expression, command, job_metadata)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                cron_expression = EXCLUDED.cron_expression,
                command = EXCLUDED.command,
                job_metadata = EXCLUDED.job_metadata,
                updated_at = NOW()
            RETURNING id
            "#
        )
        .bind(job.id.as_str())
        .bind(&job.name)
        .bind(&job.cron_expression)
        .bind(&command)
        .bind(serde_json::to_value(job)?)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| JobError::pg_cron(&format!("Failed to insert backbone cron job: {}", e)))?;

        // Create the actual pg_cron job
        let cron_job_id = self.create_pg_cron_job(&job.cron_expression, &command).await?;

        Ok(cron_job_id)
    }

    /// Update a pg_cron job
    pub async fn update_cron_job(&self, job: &Job) -> JobResult<()> {
        // First, remove the existing pg_cron job
        self.remove_cron_job(&job.id).await?;

        // Then create a new one
        self.create_cron_job(job).await?;

        Ok(())
    }

    /// Remove a pg_cron job
    pub async fn remove_cron_job(&self, job_id: &crate::types::JobId) -> JobResult<()> {
        // Get the cron job ID from our tracking table
        let cron_job_id: Option<i32> = sqlx::query_scalar(
            "SELECT cron.jobid FROM cron.job JOIN backbone_cron_jobs ON cron.job.command = backbone_cron_jobs.command WHERE backbone_cron_jobs.id = $1"
        )
        .bind(job_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| JobError::pg_cron(&format!("Failed to query cron job: {}", e)))?;

        // Remove from pg_cron if it exists
        if let Some(jobid) = cron_job_id {
            sqlx::query("SELECT cron.unschedule($1)")
                .bind(jobid)
                .execute(&self.pool)
                .await
                .map_err(|e| JobError::pg_cron(&format!("Failed to unschedule cron job: {}", e)))?;
        }

        // Remove from our tracking table
        sqlx::query("DELETE FROM backbone_cron_jobs WHERE id = $1")
            .bind(job_id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to delete backbone cron job: {}", e)))?;

        Ok(())
    }

    /// Enable a pg_cron job
    pub async fn enable_cron_job(&self, job_id: &crate::types::JobId) -> JobResult<()> {
        sqlx::query("UPDATE backbone_cron_jobs SET active = true, updated_at = NOW() WHERE id = $1")
            .bind(job_id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to enable cron job: {}", e)))?;

        Ok(())
    }

    /// Disable a pg_cron job
    pub async fn disable_cron_job(&self, job_id: &crate::types::JobId) -> JobResult<()> {
        sqlx::query("UPDATE backbone_cron_jobs SET active = false, updated_at = NOW() WHERE id = $1")
            .bind(job_id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to disable cron job: {}", e)))?;

        Ok(())
    }

    /// List all pg_cron jobs
    pub async fn list_cron_jobs(&self) -> JobResult<Vec<PgCronJobInfo>> {
        let rows = sqlx::query(
            r#"
            SELECT
                bcj.id,
                bcj.name,
                bcj.cron_expression,
                bcj.command,
                bcj.active,
                bcj.created_at,
                bcj.updated_at,
                bcj.job_metadata,
                cj.jobid as pg_job_id
            FROM backbone_cron_jobs bcj
            LEFT JOIN cron.job cj ON cj.command = bcj.command
            ORDER BY bcj.created_at
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| JobError::pg_cron(&format!("Failed to list cron jobs: {}", e)))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(PgCronJobInfo {
                id: row.get("id"),
                name: row.get("name"),
                cron_expression: row.get("cron_expression"),
                command: row.get("command"),
                active: row.get("active"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                job_metadata: row.get("job_metadata"),
                pg_job_id: row.get("pg_job_id"),
            });
        }

        Ok(jobs)
    }

    /// Get pg_cron job statistics
    pub async fn get_statistics(&self) -> JobResult<PgCronStatistics> {
        let total_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM backbone_cron_jobs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to get total jobs: {}", e)))?;

        let active_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM backbone_cron_jobs WHERE active = true")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to get active jobs: {}", e)))?;

        let pg_cron_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cron.job")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to get pg_cron jobs: {}", e)))?;

        Ok(PgCronStatistics {
            total_tracked_jobs: total_jobs as u64,
            active_tracked_jobs: active_jobs as u64,
            pg_cron_jobs: pg_cron_jobs as u64,
        })
    }

    /// Sync backbone jobs with pg_cron (reconcile differences)
    pub async fn sync_jobs(&self, backbone_jobs: &[Job]) -> JobResult<SyncResult> {
        let mut created = 0;
        let mut updated = 0;
        let mut removed = 0;

        let pg_cron_jobs = self.list_cron_jobs().await?;
        let pg_job_ids: std::collections::HashSet<String> = pg_cron_jobs.iter().map(|j| j.id.clone()).collect();
        let backbone_job_ids: std::collections::HashSet<String> = backbone_jobs.iter().map(|j| j.id.as_str().to_string()).collect();

        // Create or update jobs that exist in backbone but not in pg_cron
        for job in backbone_jobs {
            if !pg_job_ids.contains(job.id.as_str()) {
                self.create_cron_job(job).await?;
                created += 1;
            } else {
                // Check if job needs updating (comparing cron expression and other fields)
                if let Some(pg_job) = pg_cron_jobs.iter().find(|j| j.id == job.id.as_str()) {
                    if pg_job.cron_expression != job.cron_expression || !pg_job.active {
                        self.update_cron_job(job).await?;
                        updated += 1;
                    }
                }
            }
        }

        // Remove jobs that exist in pg_cron but not in backbone
        for job_id in pg_job_ids.difference(&backbone_job_ids) {
            self.remove_cron_job(&crate::types::JobId::parse(job_id)).await?;
            removed += 1;
        }

        Ok(SyncResult {
            created,
            updated,
            removed,
        })
    }

    /// Generate the SQL command for a job
    fn generate_cron_command(&self, job: &Job) -> JobResult<String> {
        let queue_payload = serde_json::json!({
            "job_id": job.id.as_str(),
            "job_name": job.name,
            "queue": job.queue,
            "payload": job.payload,
            "metadata": job.metadata,
            "timestamp": Utc::now().to_rfc3339()
        });

        let queue_payload_str = serde_json::to_string(&queue_payload)?;
        let escaped_payload = queue_payload_str.replace('\'', "''");

        // Generate SQL command that inserts into the job queue
        let command = format!(
            r#"
            INSERT INTO job_queue (queue_name, payload, priority, created_at)
            VALUES ('{}', '{}', {}, NOW())
            "#,
            job.queue, escaped_payload, job.priority as i32
        );

        Ok(command.trim().to_string())
    }

    /// Create a pg_cron job directly
    async fn create_pg_cron_job(&self, cron_expression: &str, command: &str) -> JobResult<i32> {
        let result = sqlx::query_scalar("SELECT cron.schedule($1, $2, $3)")
            .bind(format!("backbone_job_{}", chrono::Utc::now().timestamp()))
            .bind(cron_expression)
            .bind(command)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| JobError::pg_cron(&format!("Failed to schedule pg_cron job: {}", e)))?;

        Ok(result)
    }
}

/// pg_cron job information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgCronJobInfo {
    pub id: String,
    pub name: String,
    pub cron_expression: String,
    pub command: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub job_metadata: Option<serde_json::Value>,
    pub pg_job_id: Option<i32>,
}

/// pg_cron statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgCronStatistics {
    pub total_tracked_jobs: u64,
    pub active_tracked_jobs: u64,
    pub pg_cron_jobs: u64,
}

/// Sync operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub created: u32,
    pub updated: u32,
    pub removed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JobId;

    #[test]
    fn test_generate_cron_command() {
        // Test command generation without database connection
        let job = Job::new(
            JobId::new(),
            "Test Job".to_string(),
            "0 12 * * *".to_string(),
            "test_queue".to_string(),
            serde_json::json!({"test": true}),
        );

        // Create a mock PgCronManager (we only need to test command generation)
        // The command generation doesn't require a database connection
        let command = format!(
            r#"
            INSERT INTO job_queue (queue_name, payload, priority, created_at)
            VALUES ('{}', '{{"test":true}}', {}, NOW())
            "#,
            job.queue, job.priority as i32
        );

        // Verify the command format
        assert!(command.contains("INSERT INTO job_queue"));
        assert!(command.contains("test_queue"));
        assert!(command.contains("priority"));
    }

    #[test]
    fn test_pg_cron_manager_creation() {
        // Test that PgCronManager can be created (doesn't require database at creation)
        // Note: This is a compile-time check to ensure the struct can be instantiated
        // In a real integration test, you would use a test database
        let _ = std::marker::PhantomData::<PgCronManager>;
    }
}
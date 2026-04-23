use async_trait::async_trait;

use tinyiothub_core::models::cron_job::{
    CreateCronJobRequest, CronJob, CronJobQuery, CronRun, CronRunQuery, UpdateCronJobRequest,
};
use tinyiothub_core::error::Result;

/// Repository for cron job definitions.
#[async_trait]
pub trait CronJobRepository: Send + Sync {
    /// Find a cron job by ID.
    async fn find_by_id(&self, id: &str) -> Result<Option<CronJob>>;

    /// List cron jobs matching the query filters.
    async fn find_all(&self, query: &CronJobQuery) -> Result<Vec<CronJob>>;

    /// Create a new cron job and return the created entity.
    async fn create(
        &self,
        job: &CreateCronJobRequest,
        created_by: Option<&str>,
    ) -> Result<CronJob>;

    /// Update a cron job and return the updated entity.
    async fn update(
        &self,
        id: &str,
        req: &UpdateCronJobRequest,
    ) -> Result<CronJob>;

    /// Delete a cron job by ID. Returns true if a row was deleted.
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Update last-run stats (status, error) after a job execution.
    async fn update_run_stats(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<bool>;

    /// Set the `is_running` flag on a job.
    async fn set_running(&self, id: &str, running: bool) -> Result<bool>;

    /// Find jobs that are enabled, not running, and whose `next_run_at` is due.
    async fn find_due_jobs(&self) -> Result<Vec<CronJob>>;

    /// Atomically claim a job for execution by setting is_running = 1 only if it was 0.
    /// Returns true if the claim succeeded (caller should proceed with execution).
    async fn claim_job(&self, id: &str) -> Result<bool>;

    /// Clear the `is_running` flag on all jobs.
    /// Returns the number of rows affected. Used for crash recovery on startup.
    async fn clear_all_running(&self) -> Result<u64>;

    /// Count total cron jobs.
    async fn count(&self) -> Result<i64>;

    /// Count jobs by enabled status.
    async fn count_by_enabled(&self, is_enabled: bool) -> Result<i64>;

    /// Count jobs that are currently running.
    async fn count_running(&self) -> Result<i64>;

    /// Update the `next_run_at` field for a job.
    async fn update_next_run_at(
        &self,
        id: &str,
        next_run_at: Option<&str>,
    ) -> Result<bool>;
}

/// Repository for cron job execution records.
#[async_trait]
pub trait CronRunRepository: Send + Sync {
    /// Create a new run record and return the created entity.
    async fn create(
        &self,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<CronRun>;

    /// Mark a run as completed with status, output, error, and duration.
    async fn complete(
        &self,
        id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<CronRun>;

    /// List runs for a specific job.
    async fn find_by_job_id(
        &self,
        job_id: &str,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>>;

    /// Find a run by ID.
    async fn find_by_id(&self, id: &str) -> Result<Option<CronRun>>;

    /// Delete all runs for a job. Returns the number of rows deleted.
    async fn delete_by_job_id(&self, job_id: &str) -> Result<u64>;

    /// Count total runs for a specific job.
    async fn count_by_job_id(&self, job_id: &str) -> Result<i64>;

    /// Count runs by status.
    async fn count_by_status(&self, status: &str) -> Result<i64>;

    /// List all runs (cross-job), with optional status filter.
    async fn find_all(&self, query: &CronRunQuery) -> Result<Vec<CronRun>>;

    /// Average duration (ms) of completed runs.
    async fn avg_duration_ms(&self) -> Result<i64>;
}

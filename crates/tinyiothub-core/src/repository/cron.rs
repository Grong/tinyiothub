//! Cron repository contracts.

use async_trait::async_trait;

use crate::models::cron_job::{
    CreateCronJobRequest, CronJob, CronJobQuery, CronRun, CronRunQuery, UpdateCronJobRequest,
};
use crate::error::Result;

/// Repository for cron job definitions.
#[async_trait]
pub trait CronJobRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<CronJob>>;
    async fn find_all(&self, query: &CronJobQuery) -> Result<Vec<CronJob>>;
    async fn create(&self, job: &CreateCronJobRequest, created_by: Option<&str>) -> Result<CronJob>;
    async fn update(&self, id: &str, req: &UpdateCronJobRequest) -> Result<CronJob>;
    async fn delete(&self, id: &str) -> Result<bool>;
    async fn update_run_stats(&self, id: &str, status: &str, error: Option<&str>) -> Result<bool>;
    async fn set_running(&self, id: &str, running: bool) -> Result<bool>;
    async fn find_due_jobs(&self) -> Result<Vec<CronJob>>;
    async fn claim_job(&self, id: &str) -> Result<bool>;
    async fn clear_all_running(&self) -> Result<u64>;
    async fn count(&self) -> Result<i64>;
    async fn count_by_enabled(&self, is_enabled: bool) -> Result<i64>;
    async fn count_running(&self) -> Result<i64>;
    async fn update_next_run_at(&self, id: &str, next_run_at: Option<&str>) -> Result<bool>;
}

/// Repository for cron job execution records.
#[async_trait]
pub trait CronRunRepository: Send + Sync {
    async fn create(&self, job_id: &str, workspace_id: &str, trigger_type: &str, triggered_by: Option<&str>) -> Result<CronRun>;
    async fn complete(&self, id: &str, workspace_id: &str, status: &str, output: Option<&str>, error: Option<&str>, duration_ms: i64) -> Result<CronRun>;
    async fn find_by_job_id(&self, job_id: &str, workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>>;
    async fn find_by_id(&self, id: &str, workspace_id: &str) -> Result<Option<CronRun>>;
    async fn delete_by_job_id(&self, job_id: &str, workspace_id: &str) -> Result<u64>;
    async fn count_by_job_id(&self, job_id: &str, workspace_id: &str) -> Result<i64>;
    async fn count_by_status(&self, workspace_id: &str, status: &str) -> Result<i64>;
    async fn find_all(&self, workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>>;
    async fn avg_duration_ms(&self, workspace_id: &str) -> Result<i64>;
}

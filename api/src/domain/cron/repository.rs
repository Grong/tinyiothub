use async_trait::async_trait;

use crate::dto::entity::cron_job::{
    CreateCronJobRequest, CronJob, CronJobQuery, CronRun, CronRunQuery, UpdateCronJobRequest,
};

#[async_trait]
pub trait CronJobRepository: Send + Sync {
    async fn find_by_id(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> Result<Option<CronJob>, sqlx::Error>;
    async fn find_all(&self, query: &CronJobQuery) -> Result<Vec<CronJob>, sqlx::Error>;
    async fn create(
        &self,
        job: &CreateCronJobRequest,
        workspace_id: &str,
        created_by: Option<&str>,
    ) -> Result<String, sqlx::Error>;
    async fn update(
        &self,
        id: &str,
        workspace_id: &str,
        req: &UpdateCronJobRequest,
    ) -> Result<bool, sqlx::Error>;
    async fn delete(&self, id: &str, workspace_id: &str) -> Result<bool, sqlx::Error>;
    async fn update_run_stats(
        &self,
        id: &str,
        workspace_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<bool, sqlx::Error>;
    async fn set_running(
        &self,
        id: &str,
        workspace_id: &str,
        running: bool,
    ) -> Result<bool, sqlx::Error>;
    async fn find_due_jobs(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<CronJob>, sqlx::Error>;
    async fn clear_all_running(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<u64, sqlx::Error>;
    async fn count(&self, workspace_id: &str) -> Result<i64, sqlx::Error>;
}

#[async_trait]
pub trait CronRunRepository: Send + Sync {
    async fn create(
        &self,
        job_id: &str,
        workspace_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<String, sqlx::Error>;
    async fn complete(
        &self,
        id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<bool, sqlx::Error>;
    async fn find_by_job_id(
        &self,
        job_id: &str,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>, sqlx::Error>;
    async fn find_by_id(
        &self,
        id: &str,
        workspace_id: &str,
    ) -> Result<Option<CronRun>, sqlx::Error>;
    async fn delete_by_job_id(
        &self,
        job_id: &str,
        workspace_id: &str,
    ) -> Result<u64, sqlx::Error>;
    async fn count_by_job_id(
        &self,
        job_id: &str,
        workspace_id: &str,
    ) -> Result<i64, sqlx::Error>;
    async fn count_by_status(
        &self,
        workspace_id: &str,
        status: &str,
    ) -> Result<i64, sqlx::Error>;
}

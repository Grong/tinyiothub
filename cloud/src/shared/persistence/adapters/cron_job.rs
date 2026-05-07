use std::collections::HashSet;

use async_trait::async_trait;
use tinyiothub_core::{
    error::Result,
    models::cron_job::{CreateCronJobRequest, CronJob, CronJobQuery, UpdateCronJobRequest},
};
use tinyiothub_storage::traits::cron::CronJobRepository;

use crate::shared::persistence::database::Database;

/// Tenant-aware cron job repository adapter
///
/// Wraps a CronJobRepository implementation and automatically adds
/// workspace filtering to enforce tenant isolation.
#[derive(Debug, Clone)]
pub struct TenantCronJobRepository<R: CronJobRepository> {
    inner: R,
    workspace_id: String,
    database: Database,
}

impl<R: CronJobRepository> TenantCronJobRepository<R> {
    /// Create a new tenant-aware cron job repository adapter
    pub fn new(inner: R, workspace_id: String, database: Database) -> Self {
        Self { inner, workspace_id, database }
    }

    /// Get the workspace ID this adapter is filtering for
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Check if a cron job belongs to this workspace
    async fn job_belongs_to_workspace(&self, job_id: &str) -> Result<bool> {
        let result: Option<(String,)> =
            sqlx::query_as("SELECT workspace_id FROM cron_jobs WHERE id = ?")
                .bind(job_id)
                .fetch_optional(self.database.pool())
                .await?;

        match result {
            Some((workspace_id,)) => Ok(workspace_id == self.workspace_id),
            None => Ok(false), // Job doesn't exist
        }
    }
}

#[async_trait]
impl<R: CronJobRepository + Send + Sync> CronJobRepository for TenantCronJobRepository<R> {
    async fn find_by_id(&self, id: &str) -> Result<Option<CronJob>> {
        if !self.job_belongs_to_workspace(id).await? {
            return Ok(None);
        }
        self.inner.find_by_id(id).await
    }

    async fn find_all(&self, query: &CronJobQuery) -> Result<Vec<CronJob>> {
        let mut filtered_query = query.clone();
        filtered_query.workspace_id = Some(self.workspace_id.clone());
        self.inner.find_all(&filtered_query).await
    }

    async fn create(
        &self,
        job: &CreateCronJobRequest,
        created_by: Option<&str>,
    ) -> Result<CronJob> {
        self.inner.create(job, created_by).await
    }

    async fn update(&self, id: &str, req: &UpdateCronJobRequest) -> Result<CronJob> {
        if !self.job_belongs_to_workspace(id).await? {
            return Err(tinyiothub_core::error::Error::NotFound);
        }
        self.inner.update(id, req).await
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        if !self.job_belongs_to_workspace(id).await? {
            return Err(tinyiothub_core::error::Error::NotFound);
        }
        self.inner.delete(id).await
    }

    async fn update_run_stats(&self, id: &str, status: &str, error: Option<&str>) -> Result<bool> {
        if !self.job_belongs_to_workspace(id).await? {
            return Ok(false);
        }
        self.inner.update_run_stats(id, status, error).await
    }

    async fn set_running(&self, id: &str, running: bool) -> Result<bool> {
        if !self.job_belongs_to_workspace(id).await? {
            return Ok(false);
        }
        self.inner.set_running(id, running).await
    }

    async fn find_due_jobs(&self) -> Result<Vec<CronJob>> {
        let ws_job_ids: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM cron_jobs WHERE workspace_id = ?")
                .bind(&self.workspace_id)
                .fetch_all(self.database.pool())
                .await?;

        let ws_ids: HashSet<String> = ws_job_ids.into_iter().map(|(id,)| id).collect();

        let due_jobs = self.inner.find_due_jobs().await?;
        Ok(due_jobs.into_iter().filter(|j| ws_ids.contains(&j.id)).collect())
    }

    async fn claim_job(&self, id: &str) -> Result<bool> {
        if !self.job_belongs_to_workspace(id).await? {
            return Ok(false);
        }
        self.inner.claim_job(id).await
    }

    async fn clear_all_running(&self) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE cron_jobs SET is_running = 0, updated_at = datetime('now') WHERE is_running = 1 AND workspace_id = ?"
        )
        .bind(&self.workspace_id)
        .execute(self.database.pool())
        .await?;
        Ok(result.rows_affected())
    }

    async fn count(&self) -> Result<i64> {
        let result: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM cron_jobs WHERE workspace_id = ?")
                .bind(&self.workspace_id)
                .fetch_one(self.database.pool())
                .await?;
        Ok(result.0)
    }

    async fn count_by_enabled(&self, is_enabled: bool) -> Result<i64> {
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM cron_jobs WHERE workspace_id = ? AND is_enabled = ?",
        )
        .bind(&self.workspace_id)
        .bind(if is_enabled { 1 } else { 0 })
        .fetch_one(self.database.pool())
        .await?;
        Ok(result.0)
    }

    async fn count_running(&self) -> Result<i64> {
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM cron_jobs WHERE workspace_id = ? AND is_running = 1",
        )
        .bind(&self.workspace_id)
        .fetch_one(self.database.pool())
        .await?;
        Ok(result.0)
    }

    async fn update_next_run_at(&self, id: &str, next_run_at: Option<&str>) -> Result<bool> {
        if !self.job_belongs_to_workspace(id).await? {
            return Ok(false);
        }
        self.inner.update_next_run_at(id, next_run_at).await
    }
}

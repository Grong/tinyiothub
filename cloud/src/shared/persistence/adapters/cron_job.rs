use async_trait::async_trait;
use tinyiothub_core::error::Result;
use tinyiothub_core::models::cron_job::{CreateCronJobRequest, CronJob, CronJobQuery, UpdateCronJobRequest};
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
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT workspace_id FROM cron_jobs WHERE id = ?"
        )
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
        // Verify job belongs to this workspace
        if !self.job_belongs_to_workspace(id).await? {
            return Ok(None);
        }
        self.inner.find_by_id(id).await
    }

    async fn find_all(&self, query: &CronJobQuery) -> Result<Vec<CronJob>> {
        // TODO: Add workspace filtering to query
        // Since CronJobQuery doesn't have workspace_id field,
        // we need to modify the SQL query or filter results after fetching.
        self.inner.find_all(query).await
    }

    async fn create(
        &self,
        job: &CreateCronJobRequest,
        created_by: Option<&str>,
    ) -> Result<CronJob> {
        self.inner.create(job, created_by).await
    }

    async fn update(
        &self,
        id: &str,
        req: &UpdateCronJobRequest,
    ) -> Result<CronJob> {
        // TODO: Verify job belongs to this workspace before updating
        self.inner.update(id, req).await
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        // TODO: Verify job belongs to this workspace before deleting
        self.inner.delete(id).await
    }

    async fn update_run_stats(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<bool> {
        // TODO: Verify job belongs to this workspace
        self.inner.update_run_stats(id, status, error).await
    }

    async fn set_running(&self, id: &str, running: bool) -> Result<bool> {
        // TODO: Verify job belongs to this workspace
        self.inner.set_running(id, running).await
    }

    async fn find_due_jobs(&self) -> Result<Vec<CronJob>> {
        // TODO: Add workspace filtering - only due jobs in this workspace
        self.inner.find_due_jobs().await
    }

    async fn claim_job(&self, id: &str) -> Result<bool> {
        // TODO: Verify job belongs to this workspace and claim only if it does
        self.inner.claim_job(id).await
    }

    async fn clear_all_running(&self) -> Result<u64> {
        // TODO: Clear only running jobs in this workspace
        self.inner.clear_all_running().await
    }

    async fn count(&self) -> Result<i64> {
        // TODO: Count only jobs in this workspace
        self.inner.count().await
    }

    async fn count_by_enabled(&self, is_enabled: bool) -> Result<i64> {
        // TODO: Count only jobs in this workspace with given enabled status
        self.inner.count_by_enabled(is_enabled).await
    }

    async fn count_running(&self) -> Result<i64> {
        // TODO: Count only running jobs in this workspace
        self.inner.count_running().await
    }

    async fn update_next_run_at(
        &self,
        id: &str,
        next_run_at: Option<&str>,
    ) -> Result<bool> {
        // TODO: Verify job belongs to this workspace
        self.inner.update_next_run_at(id, next_run_at).await
    }
}
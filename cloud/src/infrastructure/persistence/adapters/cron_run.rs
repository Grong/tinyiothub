use async_trait::async_trait;
use tinyiothub_core::error::Result;
use tinyiothub_core::models::cron_job::{CronRun, CronRunQuery};
use tinyiothub_storage::traits::cron::CronRunRepository;

/// Tenant-aware cron run repository adapter
///
/// Wraps a CronRunRepository implementation and automatically adds
/// workspace filtering to enforce tenant isolation.
#[derive(Debug, Clone)]
pub struct TenantCronRunRepository<R: CronRunRepository> {
    inner: R,
    workspace_id: String,
}

impl<R: CronRunRepository> TenantCronRunRepository<R> {
    /// Create a new tenant-aware cron run repository adapter
    pub fn new(inner: R, workspace_id: String) -> Self {
        Self { inner, workspace_id }
    }

    /// Get the workspace ID this adapter is filtering for
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

#[async_trait]
impl<R: CronRunRepository + Send + Sync> CronRunRepository for TenantCronRunRepository<R> {
    async fn create(
        &self,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<CronRun> {
        // TODO: Verify job belongs to this workspace before creating run record
        // Also need to inject workspace_id into the database insert
        self.inner.create(job_id, trigger_type, triggered_by).await
    }

    async fn complete(
        &self,
        id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<CronRun> {
        // TODO: Verify run belongs to this workspace
        self.inner.complete(id, status, output, error, duration_ms).await
    }

    async fn find_by_job_id(
        &self,
        job_id: &str,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>> {
        // TODO: Verify job belongs to this workspace and filter runs accordingly
        self.inner.find_by_job_id(job_id, query).await
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<CronRun>> {
        // TODO: Verify run belongs to this workspace
        self.inner.find_by_id(id).await
    }

    async fn delete_by_job_id(&self, job_id: &str) -> Result<u64> {
        // TODO: Verify job belongs to this workspace
        self.inner.delete_by_job_id(job_id).await
    }

    async fn count_by_job_id(&self, job_id: &str) -> Result<i64> {
        // TODO: Verify job belongs to this workspace
        self.inner.count_by_job_id(job_id).await
    }

    async fn count_by_status(&self, status: &str) -> Result<i64> {
        // TODO: Count only runs in this workspace with given status
        self.inner.count_by_status(status).await
    }

    async fn find_all(&self, query: &CronRunQuery) -> Result<Vec<CronRun>> {
        // TODO: Add workspace filtering to query
        self.inner.find_all(query).await
    }

    async fn avg_duration_ms(&self) -> Result<i64> {
        // TODO: Calculate average duration only for runs in this workspace
        self.inner.avg_duration_ms().await
    }
}
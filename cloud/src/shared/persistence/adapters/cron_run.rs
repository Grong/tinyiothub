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
        _workspace_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<CronRun> {
        self.inner.create(job_id, &self.workspace_id, trigger_type, triggered_by).await
    }

    async fn complete(
        &self,
        id: &str,
        _workspace_id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) -> Result<CronRun> {
        self.inner.complete(id, &self.workspace_id, status, output, error, duration_ms).await
    }

    async fn find_by_job_id(
        &self,
        job_id: &str,
        _workspace_id: &str,
        query: &CronRunQuery,
    ) -> Result<Vec<CronRun>> {
        self.inner.find_by_job_id(job_id, &self.workspace_id, query).await
    }

    async fn find_by_id(&self, id: &str, _workspace_id: &str) -> Result<Option<CronRun>> {
        self.inner.find_by_id(id, &self.workspace_id).await
    }

    async fn delete_by_job_id(&self, job_id: &str, _workspace_id: &str) -> Result<u64> {
        self.inner.delete_by_job_id(job_id, &self.workspace_id).await
    }

    async fn count_by_job_id(&self, job_id: &str, _workspace_id: &str) -> Result<i64> {
        self.inner.count_by_job_id(job_id, &self.workspace_id).await
    }

    async fn count_by_status(&self, _workspace_id: &str, status: &str) -> Result<i64> {
        self.inner.count_by_status(&self.workspace_id, status).await
    }

    async fn find_all(&self, _workspace_id: &str, query: &CronRunQuery) -> Result<Vec<CronRun>> {
        self.inner.find_all(&self.workspace_id, query).await
    }

    async fn avg_duration_ms(&self, _workspace_id: &str) -> Result<i64> {
        self.inner.avg_duration_ms(&self.workspace_id).await
    }
}

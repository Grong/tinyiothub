use async_trait::async_trait;

use crate::dto::entity::job::{
    CreateJobRequest, Job, JobExecution, JobExecutionQueryParams, JobQueryParams, JobStatistics,
    UpdateJobRequest,
};
use crate::shared::error::Result;

#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Job>>;
    async fn find_all(&self, params: &JobQueryParams) -> Result<Vec<Job>>;
    async fn create(&self, req: &CreateJobRequest) -> Result<Job>;
    async fn update(&self, id: &str, req: &UpdateJobRequest) -> Result<Job>;
    async fn delete(&self, id: &str) -> Result<u64>;
    async fn set_enabled(&self, id: &str, is_enabled: bool) -> Result<Job>;
    async fn set_running(&self, id: &str, is_running: bool) -> Result<()>;
    async fn update_run_stats(&self, id: &str, status: &str, error: Option<&str>) -> Result<()>;
    async fn get_statistics(&self) -> Result<JobStatistics>;
}

#[async_trait]
pub trait JobExecutionRepository: Send + Sync {
    async fn find_all(&self, params: &JobExecutionQueryParams) -> Result<Vec<JobExecution>>;
    async fn count(&self, params: &JobExecutionQueryParams) -> Result<i64>;
    async fn create(
        &self,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<JobExecution>;
    async fn find_by_id(&self, id: &str) -> Result<Option<JobExecution>>;
    async fn find_by_job(&self, job_id: &str, limit: i32) -> Result<Vec<JobExecution>>;
    async fn update_status(
        &self,
        id: &str,
        status: &str,
        ended_at: Option<&str>,
        result: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()>;
    async fn delete_by_job_id(&self, job_id: &str) -> Result<u64>;
}

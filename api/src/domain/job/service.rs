use std::sync::Arc;

use crate::dto::entity::job::{
    CreateJobRequest, Job, JobExecution, JobExecutionQueryParams, JobQueryParams, JobStatistics,
    UpdateJobRequest,
};
use crate::shared::error::Result;

use super::repository::{JobExecutionRepository, JobRepository};

pub struct JobService {
    job_repository: Arc<dyn JobRepository>,
}

impl JobService {
    pub fn new(job_repository: Arc<dyn JobRepository>) -> Self {
        Self { job_repository }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Job>> {
        self.job_repository.find_by_id(id).await
    }

    pub async fn find_all(&self, params: &JobQueryParams) -> Result<Vec<Job>> {
        self.job_repository.find_all(params).await
    }

    pub async fn create(&self, req: &CreateJobRequest) -> Result<Job> {
        self.job_repository.create(req).await
    }

    pub async fn update(&self, id: &str, req: &UpdateJobRequest) -> Result<Job> {
        self.job_repository.update(id, req).await
    }

    pub async fn delete(&self, id: &str) -> Result<u64> {
        self.job_repository.delete(id).await
    }

    pub async fn set_enabled(&self, id: &str, is_enabled: bool) -> Result<Job> {
        self.job_repository.set_enabled(id, is_enabled).await
    }

    pub async fn set_running(&self, id: &str, is_running: bool) -> Result<()> {
        self.job_repository.set_running(id, is_running).await
    }

    pub async fn update_run_stats(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.job_repository.update_run_stats(id, status, error).await
    }

    pub async fn get_statistics(&self) -> Result<JobStatistics> {
        self.job_repository.get_statistics().await
    }
}

pub struct JobExecutionService {
    job_execution_repository: Arc<dyn JobExecutionRepository>,
}

impl JobExecutionService {
    pub fn new(job_execution_repository: Arc<dyn JobExecutionRepository>) -> Self {
        Self { job_execution_repository }
    }

    pub async fn find_all(&self, params: &JobExecutionQueryParams) -> Result<Vec<JobExecution>> {
        self.job_execution_repository.find_all(params).await
    }

    pub async fn count(&self, params: &JobExecutionQueryParams) -> Result<i64> {
        self.job_execution_repository.count(params).await
    }

    pub async fn create(
        &self,
        job_id: &str,
        trigger_type: &str,
        triggered_by: Option<&str>,
    ) -> Result<JobExecution> {
        self.job_execution_repository.create(job_id, trigger_type, triggered_by).await
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<JobExecution>> {
        self.job_execution_repository.find_by_id(id).await
    }

    pub async fn find_by_job(&self, job_id: &str, limit: i32) -> Result<Vec<JobExecution>> {
        self.job_execution_repository.find_by_job(job_id, limit).await
    }

    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
        ended_at: Option<&str>,
        result: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        self.job_execution_repository
            .update_status(id, status, ended_at, result, error_message)
            .await
    }

    pub async fn delete_by_job_id(&self, job_id: &str) -> Result<u64> {
        self.job_execution_repository.delete_by_job_id(job_id).await
    }
}

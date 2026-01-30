use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::infrastructure::persistence::database::Database;

/// Job entity - 定时任务实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Job {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub cron_expression: String,
    pub command: String,
    pub args: Option<String>, // JSON string
    pub is_enabled: i32,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub run_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Query parameters for job search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct JobQueryParams {
    pub name: Option<String>,
    pub is_enabled: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateJobRequest {
    pub name: String,
    pub description: Option<String>,
    pub cron_expression: String,
    pub command: String,
    pub args: Option<String>,
    pub is_enabled: Option<i32>,
}

/// Request for updating a job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cron_expression: Option<String>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub is_enabled: Option<i32>,
}

impl Job {
    /// Find a job by ID
    pub async fn find_by_id(_db: &Database, _id: &str) -> Result<Option<Job>, sqlx::Error> {
        // Jobs table doesn't exist in current schema, return None
        Ok(None)
    }

    /// Create a new job
    pub async fn create(_db: &Database, request: &CreateJobRequest) -> Result<Job, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Jobs table doesn't exist, return a mock job
        Ok(Job {
            id,
            name: request.name.clone(),
            description: request.description.clone(),
            cron_expression: request.cron_expression.clone(),
            command: request.command.clone(),
            args: request.args.clone(),
            is_enabled: request.is_enabled.unwrap_or(1),
            last_run: None,
            next_run: None,
            run_count: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Update a job
    pub async fn update(
        _db: &Database,
        _id: &str,
        _request: &UpdateJobRequest,
    ) -> Result<Job, sqlx::Error> {
        // Jobs table doesn't exist, return error
        Err(sqlx::Error::RowNotFound)
    }

    /// Delete a job
    pub async fn delete(_db: &Database, _id: &str) -> Result<u64, sqlx::Error> {
        // Jobs table doesn't exist, return 0
        Ok(0)
    }

    /// Find all jobs with optional filtering
    pub async fn find_all(
        _db: &Database,
        _params: &JobQueryParams,
    ) -> Result<Vec<Job>, sqlx::Error> {
        // Return empty list if Jobs table doesn't exist
        Ok(vec![])
    }

    /// Count jobs with optional filtering
    pub async fn count(_db: &Database, _params: &JobQueryParams) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    /// Find enabled jobs
    pub async fn find_enabled(_db: &Database) -> Result<Vec<Job>, sqlx::Error> {
        Ok(vec![])
    }

    /// Update job run statistics
    pub async fn update_run_stats(
        _db: &Database,
        _id: &str,
        _last_run: &str,
        _next_run: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // Jobs table doesn't exist, do nothing
        Ok(())
    }

    /// Enable/disable job
    pub async fn set_enabled(
        _db: &Database,
        _id: &str,
        _is_enabled: bool,
    ) -> Result<Job, sqlx::Error> {
        // Jobs table doesn't exist, return error
        Err(sqlx::Error::RowNotFound)
    }

    /// Get job statistics
    pub async fn get_statistics(_db: &Database) -> Result<JobStatistics, sqlx::Error> {
        Ok(JobStatistics {
            total_jobs: 0,
            enabled_jobs: 0,
            disabled_jobs: 0,
            total_runs: 0,
        })
    }

    /// Read jobs (向后兼容方法)
    pub fn read(
        _params: Option<JobQueryParams>,
    ) -> Result<Vec<Job>, Box<dyn std::error::Error + Send + Sync>> {
        // Jobs table doesn't exist in current schema, return empty list
        Ok(vec![])
    }
}

/// Job statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JobStatistics {
    pub total_jobs: i64,
    pub enabled_jobs: i64,
    pub disabled_jobs: i64,
    pub total_runs: i64,
}

/// Job DTO for backward compatibility
pub type JobDto = Job;

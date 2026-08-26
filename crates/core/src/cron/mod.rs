//! Cron job executor contracts — traits and types for job execution.

use async_trait::async_trait;

use crate::models::cron_job::CronJob;

/// Result of a single job execution.
#[derive(Debug)]
pub struct ExecutionResult {
    pub status: String,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: i64,
}

/// Errors that can occur during job execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("execution timed out after {0}s")]
    Timeout(u64),
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("agent error: {0}")]
    AgentError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait for job executors. Each executor handles a specific job type.
#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// Execute the given cron job.
    async fn execute(&self, job: &CronJob, run_id: &str) -> std::result::Result<ExecutionResult, ExecutorError>;

    /// Return true if this executor can handle the given job type.
    fn can_handle(&self, job_type: &str) -> bool;
}

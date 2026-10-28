use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tinyiothub_core::agent_task::{AgentTask, TaskWake};
use tinyiothub_core::device_job::DeviceJobError;
use tinyiothub_core::task_approval::{ApprovalDecision, DecisionOutcome, TaskApproval};

use crate::runtime::thing_agent::task::coordinator::TaskError;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApprovalError {
    #[error("approval not found")]
    NotFound,
    #[error("approval decision conflicts with the recorded decision")]
    DecisionConflict,
    #[error("approval storage unavailable: {0}")]
    Storage(String),
    #[error(transparent)]
    Job(#[from] DeviceJobError),
    #[error(transparent)]
    Task(#[from] TaskError),
}

#[async_trait]
pub trait TaskApprovalStore: Send + Sync {
    async fn create_pending(&self, approval: &TaskApproval, expected_version: i64) -> Result<AgentTask, ApprovalError>;
    async fn get_scoped(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
    ) -> Result<Option<TaskApproval>, ApprovalError>;
    async fn get_by_decision_event(&self, event_id: &str) -> Result<Option<TaskApproval>, ApprovalError>;
    async fn list_for_task(&self, workspace_id: &str, task_id: &str) -> Result<Vec<TaskApproval>, ApprovalError>;
    async fn decide(
        &self,
        workspace_id: &str,
        task_id: &str,
        approval_id: &str,
        decision: ApprovalDecision,
        decider_id: &str,
        now: DateTime<Utc>,
        wake: &TaskWake,
    ) -> Result<DecisionOutcome, ApprovalError>;
    async fn list_recoverable(&self, workspace_id: Option<&str>) -> Result<Vec<TaskApproval>, ApprovalError>;
    async fn expire_due(&self, now: DateTime<Utc>) -> Result<u64, ApprovalError>;
    async fn cancel(
        &self,
        workspace_id: &str,
        task_id: &str,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<AgentTask, ApprovalError>;
}

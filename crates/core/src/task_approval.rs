use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Expired,
    Unavailable,
}

impl ApprovalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "expired" => Self::Expired,
            "unavailable" => Self::Unavailable,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    Reject,
}

impl ApprovalDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskApproval {
    pub id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
    pub reason: String,
    pub state: ApprovalState,
    pub decision: Option<ApprovalDecision>,
    pub decider_id: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub decision_event_id: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionOutcome {
    pub approval: TaskApproval,
    pub deduplicated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovedToolCall {
    pub workspace_id: String,
    pub task_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
}

impl From<&TaskApproval> for ApprovedToolCall {
    fn from(approval: &TaskApproval) -> Self {
        Self {
            workspace_id: approval.workspace_id.clone(),
            task_id: approval.task_id.clone(),
            run_id: approval.run_id.clone(),
            tool_call_id: approval.tool_call_id.clone(),
            device_id: approval.device_id.clone(),
            tool: approval.tool.clone(),
            args_json: approval.args_json.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaskApprovalStoreError {
    #[error("task approval not found")]
    NotFound,
    #[error("task approval decision conflicts with the recorded decision")]
    DecisionConflict,
    #[error("task approval storage unavailable: {0}")]
    Storage(String),
}

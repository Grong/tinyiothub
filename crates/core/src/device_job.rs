use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceJobState {
    Accepted,
    Running,
    Done,
    Failed,
    Rejected,
    Cancelled,
    CancelledUnknown,
    Interrupted,
}

impl DeviceJobState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Accepted | Self::Running)
    }

    pub fn is_terminal(self) -> bool {
        !self.is_active()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::CancelledUnknown => "cancelled_unknown",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "accepted" => Self::Accepted,
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "rejected" => Self::Rejected,
            "cancelled" => Self::Cancelled,
            "cancelled_unknown" => Self::CancelledUnknown,
            "interrupted" => Self::Interrupted,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceJob {
    pub id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub tool: String,
    pub args_json: Value,
    pub state: DeviceJobState,
    pub stop_requested_at: Option<DateTime<Utc>>,
    pub progress_json: Option<Value>,
    pub result_json: Option<Value>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub driver_call_id: Option<String>,
    pub timeout_ms: u64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeviceJobError {
    #[error("device job not found")]
    NotFound,
    #[error("invalid device job: {0}")]
    Validation(String),
    #[error("device job conflict: {0}")]
    Conflict(String),
    #[error("device execution conflicts with another claim")]
    ExecutionConflict,
    #[error("device job storage unavailable: {0}")]
    Storage(String),
}

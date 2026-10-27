use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active,
    WaitingJob,
    WaitingWatch,
    NeedsApproval,
    Paused,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::WaitingJob => "waiting_job",
            Self::WaitingWatch => "waiting_watch",
            Self::NeedsApproval => "needs_approval",
            Self::Paused => "paused",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "active" => Self::Active,
            "waiting_job" => Self::WaitingJob,
            "waiting_watch" => Self::WaitingWatch,
            "needs_approval" => Self::NeedsApproval,
            "paused" => Self::Paused,
            "cancelling" => Self::Cancelling,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub workspace_id: String,
    pub idempotency_key: String,
    pub objective: String,
    pub session_key: Option<String>,
    pub created_by: String,
    pub status: TaskStatus,
    pub current_run_id: Option<String>,
    pub state_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeDispatchState {
    Pending,
    Dispatched,
    Dead,
}

impl WakeDispatchState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
            Self::Dead => "dead",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "dispatched" => Some(Self::Dispatched),
            "dead" => Some(Self::Dead),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTaskOutcome {
    pub task: AgentTask,
    pub deduplicated: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaskStoreError {
    #[error("invalid task: {0}")]
    Validation(String),
    #[error("idempotency key already belongs to a different objective")]
    ObjectiveConflict,
    #[error("task changed concurrently")]
    ConcurrentUpdate,
    #[error("task store unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWake {
    pub task_id: String,
    pub event_id: String,
    pub signal_json: String,
    pub dispatch_state: WakeDispatchState,
    pub created_at: DateTime<Utc>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}

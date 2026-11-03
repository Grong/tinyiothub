use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Production,
    Simulation,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Simulation => "simulation",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "production" => Some(Self::Production),
            "simulation" => Some(Self::Simulation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEvent {
    pub seq: i64,
    pub event_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub execution_mode: ExecutionMode,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub cause_id: Option<i64>,
    pub schema_v: i64,
    pub payload_digest: String,
    pub payload_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEventInput {
    pub event_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub execution_mode: ExecutionMode,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub cause_id: Option<i64>,
    pub schema_v: i64,
    pub payload_digest: String,
    pub payload_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFactsSnapshot {
    pub observed_at: DateTime<Utc>,
    pub declaration_hash: String,
    pub driver_version: String,
    pub facts: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Prepared,
    Dispatching,
    Cancelling,
    Completed,
    Failed,
    NotApplied,
    Unknown,
    Cancelled,
    CancelledUnknown,
}

impl EffectState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Dispatching => "dispatching",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::NotApplied => "not_applied",
            Self::Unknown => "unknown",
            Self::Cancelled => "cancelled",
            Self::CancelledUnknown => "cancelled_unknown",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "prepared" => Self::Prepared,
            "dispatching" => Self::Dispatching,
            "cancelling" => Self::Cancelling,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "not_applied" => Self::NotApplied,
            "unknown" => Self::Unknown,
            "cancelled" => Self::Cancelled,
            "cancelled_unknown" => Self::CancelledUnknown,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::NotApplied | Self::Cancelled | Self::CancelledUnknown
        )
    }

    pub fn event_type(self) -> &'static str {
        match self {
            Self::Prepared => "effect.intent",
            Self::Dispatching => "effect.dispatching",
            Self::Cancelling => "effect.cancelling",
            Self::Completed => "effect.settled",
            Self::Failed => "effect.failed",
            Self::NotApplied => "effect.not_applied",
            Self::Unknown => "effect.unknown",
            Self::Cancelled => "effect.cancelled",
            Self::CancelledUnknown => "effect.cancel_unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessState {
    pub workspace_id: String,
    pub effect_id: String,
    pub session_id: String,
    pub task_id: String,
    pub run_id: String,
    pub execution_mode: ExecutionMode,
    pub status: EffectState,
    pub state_version: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProjection {
    pub job_id: String,
    pub task_id: String,
    pub run_id: String,
    pub state: String,
    pub result_json: Option<Value>,
    pub external_ref: Option<String>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeProjection {
    pub task_id: String,
    pub wake_id: String,
    pub signal_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessFact {
    pub transition_id: String,
    pub effect_id: String,
    pub next: EffectState,
    pub occurred_at: DateTime<Utc>,
    pub external_ref: Option<String>,
    pub result: Option<Value>,
    pub job: Option<JobProjection>,
    pub wake: Option<WakeProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionPrecondition {
    pub effect_id: String,
    pub expected_state: EffectState,
    pub expected_version: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectMutation {
    pub effect_id: String,
    pub state: EffectState,
    pub state_version: i64,
    pub external_ref: Option<String>,
    pub result: Option<Value>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub transition_id: String,
    pub precondition: TransitionPrecondition,
    pub effect: Option<EffectMutation>,
    pub event: Option<HarnessEventInput>,
    pub job: Option<JobProjection>,
    pub wake: Option<WakeProjection>,
}

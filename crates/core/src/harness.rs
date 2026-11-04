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
pub struct WorkflowDefinition {
    pub objective: String,
    pub trigger: WorkflowTrigger,
    pub device_selector: Value,
    pub source_selector: Value,
    pub tool_ids: Vec<String>,
    pub skill_refs: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub approval_policy: Value,
    pub evidence_policy: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowTrigger {
    Cron { trigger_id: String, expression: String },
    Watch { trigger_id: String },
    Event { trigger_id: String },
}

impl WorkflowTrigger {
    pub fn trigger_id(&self) -> &str {
        match self {
            Self::Cron { trigger_id, .. } | Self::Watch { trigger_id } | Self::Event { trigger_id } => trigger_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRevision {
    pub workflow_id: String,
    pub version: i64,
    pub enabled: bool,
    pub definition: WorkflowDefinition,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOccurrence {
    pub source_key: String,
    pub workflow_id: String,
    pub trigger_id: String,
    pub occurrence_id: String,
    pub workflow_version: i64,
    pub task_id: String,
    pub run_id: String,
    pub session_id: String,
    pub source: Value,
    pub created_at: DateTime<Utc>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    ToolApproval,
    EffectReconciliation,
    Other,
}

impl DecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToolApproval => "tool_approval",
            Self::EffectReconciliation => "effect_reconciliation",
            Self::Other => "other",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "tool_approval" => Some(Self::ToolApproval),
            "effect_reconciliation" => Some(Self::EffectReconciliation),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionState {
    Requested,
    Answered,
    Consumed,
    Expired,
    Withdrawn,
}

impl DecisionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Answered => "answered",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "answered" => Some(Self::Answered),
            "consumed" => Some(Self::Consumed),
            "expired" => Some(Self::Expired),
            "withdrawn" => Some(Self::Withdrawn),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    Steer,
    Interrupt,
    Pause,
    Cancel,
    Queue,
}

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::Interrupt => "interrupt",
            Self::Pause => "pause",
            Self::Cancel => "cancel",
            Self::Queue => "queue",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "steer" => Some(Self::Steer),
            "interrupt" => Some(Self::Interrupt),
            "pause" => Some(Self::Pause),
            "cancel" => Some(Self::Cancel),
            "queue" => Some(Self::Queue),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandState {
    Requested,
    Consumed,
    Rejected,
    Superseded,
    Expired,
}

impl CommandState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Consumed => "consumed",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "consumed" => Some(Self::Consumed),
            "rejected" => Some(Self::Rejected),
            "superseded" => Some(Self::Superseded),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    pub fn event_type(self) -> &'static str {
        match self {
            Self::Requested => "command.requested",
            Self::Consumed => "command.consumed",
            Self::Rejected => "command.rejected",
            Self::Superseded => "command.superseded",
            Self::Expired => "command.expired",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessCommand {
    pub command_id: String,
    pub kind: CommandKind,
    pub target_task_id: String,
    pub target_run_id: String,
    pub actor: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub state: CommandState,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectIntent {
    pub effect_id: String,
    pub session_id: String,
    pub task_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    #[serde(skip_serializing)]
    pub request_json: Value,
    pub request_digest: String,
    pub idempotency_key: String,
    pub external_ref: Option<String>,
    pub state: EffectState,
    pub recovery_from: Option<EffectState>,
    pub state_version: i64,
    pub result_json: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for EffectIntent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EffectIntent")
            .field("effect_id", &self.effect_id)
            .field("session_id", &self.session_id)
            .field("task_id", &self.task_id)
            .field("run_id", &self.run_id)
            .field("tool_call_id", &self.tool_call_id)
            .field("tool_name", &self.tool_name)
            .field("request_digest", &self.request_digest)
            .field("idempotency_key", &self.idempotency_key)
            .field("external_ref", &self.external_ref)
            .field("state", &self.state)
            .field("recovery_from", &self.recovery_from)
            .field("state_version", &self.state_version)
            .field("result_json", &self.result_json)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub decision_id: String,
    pub kind: DecisionKind,
    pub state: DecisionState,
    pub session_id: String,
    pub run_id: String,
    pub task_id: String,
    pub effect_id: Option<String>,
    pub subject: Value,
    pub options: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub answer: Option<String>,
    pub answer_actor: Option<String>,
    pub answer_event_id: Option<String>,
    pub answer_evidence: Option<Value>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub consumed_by: Option<String>,
}

impl std::fmt::Debug for Decision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Decision")
            .field("decision_id", &self.decision_id)
            .field("kind", &self.kind)
            .field("state", &self.state)
            .field("session_id", &self.session_id)
            .field("run_id", &self.run_id)
            .field("task_id", &self.task_id)
            .field("effect_id", &self.effect_id)
            .field("expires_at", &self.expires_at)
            .field("answer", &self.answer)
            .field("answer_actor", &self.answer_actor)
            .field("answer_event_id", &self.answer_event_id)
            .field("consumed_at", &self.consumed_at)
            .field("consumed_by", &self.consumed_by)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionMutation {
    pub decision: Decision,
    pub expected_state: Option<DecisionState>,
    pub insert_if_missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTaskProjection {
    pub run_id: String,
    pub run_state: String,
    pub task_id: String,
    pub task_state: String,
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
    pub recovery_from: Option<EffectState>,
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
    pub additional_events: Vec<HarnessEventInput>,
    pub decision: Option<DecisionMutation>,
    pub run_task: Option<RunTaskProjection>,
    pub job: Option<JobProjection>,
    pub wake: Option<WakeProjection>,
}

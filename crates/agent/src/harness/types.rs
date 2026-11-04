use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_core::harness::{DeviceFactsSnapshot, EffectIntent, EffectState, HarnessFact, WorkflowRevision};

use crate::port::provider::ChatMessage;

pub type EffectResult = Value;

#[derive(Debug, Clone)]
pub struct MainInput {
    pub session_id: String,
    pub fact: HarnessFact,
    pub projection: Value,
    pub transcript: Vec<ChatMessage>,
}

#[derive(Debug, Clone)]
pub struct SubagentInput {
    pub session_id: String,
    pub task_id: String,
    pub run_id: String,
    pub workflow: WorkflowRevision,
    pub facts: DeviceFactsSnapshot,
    pub source: Value,
    pub transcript: Vec<ChatMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskOutcome {
    pub status: String,
    pub summary: String,
    pub observations: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub recommended_actions: Vec<String>,
}

#[async_trait::async_trait]
pub trait MainAgent: Send + Sync {
    async fn coordinate(&self, input: MainInput) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait SubagentExecutor: Send + Sync {
    async fn execute(&self, input: SubagentInput) -> anyhow::Result<TaskOutcome>;
}

#[derive(Clone, PartialEq)]
pub enum ReconcileOutcome {
    Settled(EffectResult),
    Failed(EffectResult),
    Cancelled(EffectResult),
    DefinitelyNotApplied,
    Unknown { reason: String },
}

impl std::fmt::Debug for ReconcileOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Settled(_) => "Settled(<redacted>)",
            Self::Failed(_) => "Failed(<redacted>)",
            Self::Cancelled(_) => "Cancelled(<redacted>)",
            Self::DefinitelyNotApplied => "DefinitelyNotApplied",
            Self::Unknown { .. } => "Unknown { reason: <redacted> }",
        })
    }
}

#[async_trait::async_trait]
pub trait EffectReconciler: Send + Sync {
    async fn settle(&self, intent: &EffectIntent) -> ReconcileOutcome;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HarnessError {
    #[error("effect identity does not match state")]
    IdentityMismatch,
    #[error("effect transition {from} -> {to} is forbidden")]
    InvalidTransition { from: &'static str, to: &'static str },
    #[error("completed effect requires a result")]
    MissingResult,
    #[error("decision state does not permit this recovery action")]
    DecisionConflict,
    #[error("reconciliation outcome {outcome} is invalid for effect state {state}")]
    InvalidRecovery { state: &'static str, outcome: &'static str },
}

pub(crate) fn can_transition(from: EffectState, to: EffectState) -> bool {
    match from {
        EffectState::Prepared => matches!(to, EffectState::Dispatching | EffectState::NotApplied),
        EffectState::Dispatching => matches!(
            to,
            EffectState::Completed
                | EffectState::Failed
                | EffectState::NotApplied
                | EffectState::Unknown
                | EffectState::Cancelling
        ),
        EffectState::Cancelling => matches!(
            to,
            EffectState::Cancelled | EffectState::Failed | EffectState::CancelledUnknown
        ),
        EffectState::Unknown => matches!(
            to,
            EffectState::Completed | EffectState::Failed | EffectState::NotApplied
        ),
        EffectState::Completed
        | EffectState::Failed
        | EffectState::NotApplied
        | EffectState::Cancelled
        | EffectState::CancelledUnknown => false,
    }
}

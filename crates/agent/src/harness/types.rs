use serde_json::Value;
use tinyiothub_core::harness::{EffectIntent, EffectState};

pub type EffectResult = Value;

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

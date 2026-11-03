use tinyiothub_core::harness::EffectState;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HarnessError {
    #[error("effect identity does not match state")]
    IdentityMismatch,
    #[error("effect transition {from} -> {to} is forbidden")]
    InvalidTransition { from: &'static str, to: &'static str },
    #[error("completed effect requires a result")]
    MissingResult,
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

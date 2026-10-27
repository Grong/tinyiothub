use tinyiothub_core::agent_task::TaskStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("task state transition is forbidden")]
pub enum TransitionError {
    Forbidden,
}

pub fn validate_transition(from: TaskStatus, to: TaskStatus) -> Result<(), TransitionError> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(TransitionError::Forbidden)
    }
}

pub mod approval;
pub mod coordinator;
pub mod prompt;
pub mod state;
pub mod wake;

#[cfg(test)]
mod tests {
    use tinyiothub_core::agent_task::TaskStatus;

    use super::state::{TransitionError, validate_transition};

    #[test]
    fn pause_never_changes_waiting_relationship_into_terminal() {
        assert_eq!(validate_transition(TaskStatus::Active, TaskStatus::Paused), Ok(()));
        assert_eq!(validate_transition(TaskStatus::WaitingJob, TaskStatus::Paused), Ok(()));
        assert_eq!(
            validate_transition(TaskStatus::Paused, TaskStatus::Cancelled),
            Err(TransitionError::Forbidden)
        );
    }
}

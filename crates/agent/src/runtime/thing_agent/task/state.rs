use tinyiothub_core::agent_task::TaskStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("task state transition is forbidden")]
pub enum TransitionError {
    Forbidden,
}

pub fn validate_transition(from: TaskStatus, to: TaskStatus) -> Result<(), TransitionError> {
    let allowed: Vec<TaskStatus> = match from {
        TaskStatus::Active => vec![
            TaskStatus::Active,
            TaskStatus::WaitingJob,
            TaskStatus::WaitingWatch,
            TaskStatus::Paused,
            TaskStatus::Cancelling,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ],
        TaskStatus::WaitingJob | TaskStatus::WaitingWatch => vec![
            TaskStatus::Active,
            TaskStatus::Paused,
            TaskStatus::Cancelling,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ],
        TaskStatus::Paused => vec![
            TaskStatus::Active,
            TaskStatus::WaitingJob,
            TaskStatus::WaitingWatch,
            TaskStatus::Cancelling,
        ],
        TaskStatus::Cancelling => vec![TaskStatus::Cancelling, TaskStatus::Cancelled, TaskStatus::Failed],
        TaskStatus::NeedsApproval => vec![
            TaskStatus::Active,
            TaskStatus::WaitingJob,
            TaskStatus::WaitingWatch,
            TaskStatus::Paused,
            TaskStatus::Cancelling,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ],
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => vec![],
    };
    if allowed.contains(&to) {
        Ok(())
    } else {
        Err(TransitionError::Forbidden)
    }
}

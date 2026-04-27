// Self-healing domain errors
// Migrated from domain/self_healing/errors.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SelfHealingError {
    #[error("Policy not found: {0}")]
    PolicyNotFound(String),

    #[error("Execution not found: {0}")]
    ExecutionNotFound(String),

    #[error("Action not allowed: {0}")]
    ActionNotAllowed(String),

    #[error("Cooldown active for target: {0}")]
    CooldownActive(String),

    #[error("Probe failed: {0}")]
    ProbeFailed(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

pub type Result<T> = std::result::Result<T, SelfHealingError>;

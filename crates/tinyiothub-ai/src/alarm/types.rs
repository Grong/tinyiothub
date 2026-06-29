//! Alarm types for the AI subsystem.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Alarm entity — subset used cross-domain by AiEvent::AlarmCreated.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Alarm {
    pub id: String,
    pub workspace_id: String,
    pub device_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub message: String,
    pub rule_id: Option<String>,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

/// Alarm rule definition (skeleton — full type in cloud/ migration phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRule {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub condition_json: String,
    pub enabled: bool,
}

/// Domain errors for alarm module.
#[derive(Debug, thiserror::Error)]
pub enum AlarmError {
    #[error("Alarm not found: {id}")]
    NotFound { id: String },
    #[error("Rule evaluation failed: {0}")]
    RuleEvaluation(String),
    #[error("Repository error: {0}")]
    Repository(String),
}

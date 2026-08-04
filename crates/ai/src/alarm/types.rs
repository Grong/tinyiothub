//! Cross-domain alarm event types — NOT the full alarm domain model.
//!
//! The full alarm model (AlarmService, RuleEngine, AlarmSpecifications) lives
//! in cloud::modules::alarm. This module only contains the lightweight event
//! payload used by `AiEvent::AlarmCreated(AlarmEvent)`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lightweight alarm event payload for cross-domain event dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEvent {
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

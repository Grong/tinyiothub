//! Cross-domain alarm event payload shared by the alarm and agent domains.
//!
//! Re-homed from `crates/alarm/src/types_ai.rs` in P4-Task22: the agent crate
//! must not depend on the alarm crate (dependency whitelist), while the alarm
//! crate already depends on this one — so the shared payload lives here.
//! `tinyiothub_alarm::AlarmEvent` re-exports it; the agent loop carries it in
//! `AiEvent::AlarmCreated(..)`.

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

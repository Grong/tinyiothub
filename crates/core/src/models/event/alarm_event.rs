//! Cross-domain alarm event payload (F5 归位 core — 值类型，全域共享)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lightweight alarm event payload for cross-domain event dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEvent {
    pub id: String,
    pub workspace_id: String,
    pub thing_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub message: String,
    pub rule_id: Option<String>,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

//! Patrol types — trust configuration, wake priority, wake signals, patrol reports.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Priority level for a WakeSignal
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WakePriority {
    Normal = 0,
    High = 1,
    Critical = 2,
}

impl WakePriority {
    pub fn label(&self) -> &str {
        match self {
            WakePriority::Normal => "NORMAL",
            WakePriority::High => "HIGH",
            WakePriority::Critical => "CRITICAL",
        }
    }
}

/// Signal sent to wake a specific workspace's patrol loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeSignal {
    pub workspace_id: String,
    pub reason: String,
    pub context: String,
    pub priority: WakePriority,
    /// Dedup key: signals with same (device_id, alarm_type) are merged.
    pub device_id: Option<String>,
    pub alarm_type: Option<String>,
    pub rule_id: Option<String>,
}

impl WakeSignal {
    /// Dedup key — signals with the same key and workspace replace each other.
    pub fn dedup_key(&self) -> Option<(String, String)> {
        match (&self.device_id, &self.alarm_type) {
            (Some(did), Some(at)) => Some((did.clone(), at.clone())),
            _ => None,
        }
    }
}

/// Trust level for automatic tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// All tools require human approval.
    ApprovalRequired,
    /// Read-only tools auto-execute; write tools require approval.
    ReadOnlyAuto,
    /// All tools auto-execute.
    FullAuto,
}

/// Per-workspace trust configuration for patrol auto-execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    pub trust_level: TrustLevel,
    pub max_auto_actions_per_tick: u32,
    pub allowed_tool_categories: Vec<String>,
    pub blocked_tools: Vec<String>,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            trust_level: TrustLevel::ApprovalRequired,
            max_auto_actions_per_tick: 5,
            allowed_tool_categories: vec![],
            blocked_tools: vec![],
        }
    }
}

impl TrustConfig {
    /// Load from DB JSON column, falling back to safe default.
    pub fn from_db_json(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }

    /// Serialize to JSON for DB storage.
    pub fn to_db_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Resolve final trust for a tool + workspace combination.
pub fn resolve_trust(config: &TrustConfig, tool_category: &str) -> TrustLevel {
    if config.blocked_tools.iter().any(|t| t == tool_category) {
        return TrustLevel::ApprovalRequired;
    }
    match config.trust_level {
        TrustLevel::ApprovalRequired => TrustLevel::ApprovalRequired,
        TrustLevel::ReadOnlyAuto => {
            if tool_category == "read" || tool_category == "query" {
                TrustLevel::FullAuto
            } else {
                TrustLevel::ApprovalRequired
            }
        }
        TrustLevel::FullAuto => TrustLevel::FullAuto,
    }
}

/// Status of a patrol tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatrolStatus {
    Complete,
    Partial,
    Error,
}

/// A single auto-executed action from a patrol tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoExecutedAction {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub success: bool,
    pub details: String,
}

/// A pending proposal requiring human approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingProposal {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub proposed_action: String,
    pub rationale: String,
}

/// Result of a patrol loop tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolReport {
    pub workspace_id: String,
    pub status: PatrolStatus,
    pub summary: String,
    pub executed_actions: Vec<AutoExecutedAction>,
    pub pending_proposals: Vec<PendingProposal>,
    pub error: Option<String>,
}

/// Heartbeat task persisted in DB (replaces HEARTBEAT.md file).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HeartbeatTask {
    pub id: i64,
    pub workspace_id: String,
    pub priority: String,
    pub text: String,
    pub paused: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Configuration for a patrol loop.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 15,
        }
    }
}

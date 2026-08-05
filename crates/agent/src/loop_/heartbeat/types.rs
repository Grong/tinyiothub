//! Heartbeat types — periodic check tasks and execution results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Priority level for a heartbeat signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SignalPriority {
    Normal = 0,
    High = 1,
    Critical = 2,
}

impl SignalPriority {
    pub fn label(&self) -> &str {
        match self {
            SignalPriority::Normal => "NORMAL",
            SignalPriority::High => "HIGH",
            SignalPriority::Critical => "CRITICAL",
        }
    }
}

/// Signal sent to immediately trigger a workspace's heartbeat loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatSignal {
    pub workspace_id: String,
    pub reason: String,
    pub context: String,
    pub priority: SignalPriority,
    /// Dedup key: signals with same (device_id, alarm_type) are merged.
    pub device_id: Option<String>,
    pub alarm_type: Option<String>,
    pub rule_id: Option<String>,
}

impl HeartbeatSignal {
    pub fn dedup_key(&self) -> Option<(String, String)> {
        match (&self.device_id, &self.alarm_type) {
            (Some(did), Some(at)) => Some((did.clone(), at.clone())),
            _ => None,
        }
    }
}

/// Status of a heartbeat tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeartbeatStatus {
    Complete,
    Partial,
    Error,
}

/// A single action executed during a heartbeat tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedAction {
    pub tool_name: String,
    pub device_id: Option<String>,
    pub success: bool,
    pub details: String,
}

/// Result of a heartbeat tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResult {
    pub workspace_id: String,
    pub status: HeartbeatStatus,
    pub summary: String,
    /// Number of tasks executed this tick (set by the loop, not the LLM).
    #[serde(default)]
    pub task_count: u32,
    pub executed_actions: Vec<ExecutedAction>,
    pub proposals: Vec<tinyiothub_policy::proposal::Proposal>,
    pub error: Option<String>,
}

/// A periodic heartbeat check task.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Input for creating/replacing heartbeat tasks (no server-assigned fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewHeartbeatTask {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Configuration for the heartbeat runner.
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

/// Lowest interval a workspace may configure — a tick can take minutes
/// (LLM call + tool execution), so tighter loops just pile up.
pub const MIN_HEARTBEAT_INTERVAL_MINUTES: u32 = 5;

/// Per-workspace heartbeat settings, persisted as JSON on the workspace row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceHeartbeatConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
}

impl WorkspaceHeartbeatConfig {
    pub fn validated(enabled: bool, interval_minutes: u32) -> Result<Self, String> {
        if interval_minutes < MIN_HEARTBEAT_INTERVAL_MINUTES {
            return Err(format!(
                "interval_minutes must be >= {}",
                MIN_HEARTBEAT_INTERVAL_MINUTES
            ));
        }
        Ok(Self {
            enabled,
            interval_minutes,
        })
    }

    pub fn from_db_json(json: Option<&str>) -> Option<Self> {
        let json = json?.trim();
        if json.is_empty() {
            return None;
        }
        serde_json::from_str(json).ok()
    }

    pub fn to_db_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Internal signal sent to a heartbeat loop.
#[derive(Debug, Clone)]
pub enum LoopSignal {
    /// External trigger (alarm, workspace event).
    External(HeartbeatSignal),
    /// Reload task list from repository.
    ReloadTasks,
    /// Re-read TrustConfig from shared state.
    ReloadConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_config_roundtrips_json() {
        let cfg = WorkspaceHeartbeatConfig {
            enabled: true,
            interval_minutes: 30,
        };
        let json = cfg.to_db_json();
        let loaded = WorkspaceHeartbeatConfig::from_db_json(Some(&json)).expect("parse");
        assert_eq!(loaded.interval_minutes, 30);
        assert!(loaded.enabled);
    }

    #[test]
    fn workspace_config_empty_is_none() {
        assert!(WorkspaceHeartbeatConfig::from_db_json(Some("")).is_none());
        assert!(WorkspaceHeartbeatConfig::from_db_json(Some("  ")).is_none());
        assert!(WorkspaceHeartbeatConfig::from_db_json(None).is_none());
    }

    #[test]
    fn workspace_config_rejects_sub_minimum_interval() {
        assert!(WorkspaceHeartbeatConfig::validated(true, MIN_HEARTBEAT_INTERVAL_MINUTES - 1).is_err());
        assert!(WorkspaceHeartbeatConfig::validated(true, MIN_HEARTBEAT_INTERVAL_MINUTES).is_ok());
    }
}

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
    pub executed_actions: Vec<ExecutedAction>,
    pub proposals: Vec<super::super::proposal::Proposal>,
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

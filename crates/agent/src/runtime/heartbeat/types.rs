//! Heartbeat types — periodic check tasks and execution results.

// 值类型住 core（Task 1）；WorkspaceHeartbeatConfig 为 DB 行序列化格式，留
// db crate，由 cloud 侧直接引用（Task 13 起不再经本模块转口）。
pub use serde::{Deserialize, Serialize};
pub use tinyiothub_core::heartbeat::{
    ExecutedAction, HeartbeatResult, HeartbeatStatus, HeartbeatTask, MIN_HEARTBEAT_INTERVAL_MINUTES, NewHeartbeatTask,
    TrustConfig,
};

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
    /// Dedup key: signals with same (thing_id, alarm_type) are merged.
    pub thing_id: Option<String>,
    pub alarm_type: Option<String>,
    pub rule_id: Option<String>,
}

impl HeartbeatSignal {
    pub fn dedup_key(&self) -> Option<(String, String)> {
        match (&self.thing_id, &self.alarm_type) {
            (Some(did), Some(at)) => Some((did.clone(), at.clone())),
            _ => None,
        }
    }
}
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

//! Heartbeat types — periodic check tasks and execution results.

// 持久化行类型已迁 db（E6b）；re-export 兼容。
pub use serde::{Deserialize, Serialize};
pub use tinyiothub_storage::heartbeat::{
    ExecutedAction, HeartbeatResult, HeartbeatStatus, HeartbeatTask, MIN_HEARTBEAT_INTERVAL_MINUTES, NewHeartbeatTask,
    TrustConfig, WorkspaceHeartbeatConfig,
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

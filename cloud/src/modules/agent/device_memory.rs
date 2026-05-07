// Device Memory — migrated from domain/agent/device_memory.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMemory {
    pub id: Option<i64>,
    pub workspace_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub snapshot_data: String,
    pub snapshot_time: i64,
    pub created_at: Option<String>,
}

impl DeviceMemory {
    pub fn new(
        workspace_id: String,
        agent_id: String,
        device_id: String,
        snapshot_data: serde_json::Value,
    ) -> Self {
        Self {
            id: None,
            workspace_id,
            agent_id,
            device_id,
            snapshot_data: serde_json::to_string(&snapshot_data).unwrap_or_default(),
            snapshot_time: chrono::Utc::now().timestamp_millis(),
            created_at: None,
        }
    }

    pub fn parse_snapshot(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.snapshot_data).ok()
    }
}

// Device Memory - 设备状态快照模块
// 用于 AI Agent 记忆设备状态，支持上下文理解

use serde::{Deserialize, Serialize};

/// 设备状态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMemory {
    pub id: Option<i64>,
    pub workspace_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub snapshot_data: String,  // JSON string
    pub snapshot_time: i64,     // Unix timestamp milliseconds
    pub created_at: Option<String>,
}

impl DeviceMemory {
    /// 创建新的设备快照
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

    /// 解析快照数据为 JSON Value
    pub fn parse_snapshot(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.snapshot_data).ok()
    }
}

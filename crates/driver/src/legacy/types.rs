//! Device-dashboard DTOs.
//!
//! Moved here from `cloud::modules::monitoring::types` (P4-Task20): the
//! device plane (query service + dashboard handler) is their only consumer;
//! the monitoring module itself never referenced them. The rest of
//! `cloud::modules::monitoring` (system dashboard/metrics) stays in cloud
//! and is reclaimed by the admin/system task (Task 24).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusDistribution {
    /// 在线设备数
    pub online: i64,
    /// 离线设备数
    pub offline: i64,
    /// 故障设备数
    pub error: i64,
    /// 维护中设备数
    pub maintenance: i64,
}

/// 关键设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickDevice {
    /// 设备ID
    pub id: String,
    /// 设备名称
    pub name: String,
    /// 设备状态
    pub status: String,
    /// 最后在线时间
    pub last_seen: DateTime<Utc>,
    /// 设备类型
    pub device_type: String,
}

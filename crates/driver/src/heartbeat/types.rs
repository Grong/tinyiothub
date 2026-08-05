// Heartbeat DTO
// DTOs for heartbeat tools and API endpoints

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Heartbeat status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatStatus {
    /// Gateway ID
    pub gateway_id: String,
    /// Overall health status
    pub status: String,
    /// Timestamp of the heartbeat
    pub timestamp: DateTime<Utc>,
    /// CPU usage percent (0-100)
    pub cpu_usage_percent: f64,
    /// Memory usage percent (0-100)
    pub memory_usage_percent: f64,
    /// Disk usage percent (0-100)
    pub disk_usage_percent: f64,
    /// Network connectivity status
    pub network_status: String,
    /// Number of connected devices
    pub connected_devices: u32,
    /// Number of active alarms
    pub active_alarms: u32,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Last successful cloud sync
    pub last_cloud_sync: Option<DateTime<Utc>>,
    /// Error message if any
    pub error_message: Option<String>,
}

impl Default for HeartbeatStatus {
    fn default() -> Self {
        Self {
            gateway_id: "gateway-001".to_string(),
            status: "healthy".to_string(),
            timestamp: Utc::now(),
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            disk_usage_percent: 0.0,
            network_status: "connected".to_string(),
            connected_devices: 0,
            active_alarms: 0,
            uptime_seconds: 0,
            last_cloud_sync: None,
            error_message: None,
        }
    }
}

/// Heartbeat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfig {
    /// Probe interval in seconds
    pub probe_interval_secs: u64,
    /// CPU threshold percent (triggers warning above this)
    pub cpu_threshold_percent: f64,
    /// Memory threshold percent (triggers warning above this)
    pub memory_threshold_percent: f64,
    /// Disk threshold percent (triggers warning above this)
    pub disk_threshold_percent: f64,
    /// Whether cloud sync is enabled
    pub cloud_sync_enabled: bool,
    /// Cloud sync interval in seconds
    pub cloud_sync_interval_secs: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            probe_interval_secs: 60,
            cpu_threshold_percent: 80.0,
            memory_threshold_percent: 80.0,
            disk_threshold_percent: 90.0,
            cloud_sync_enabled: true,
            cloud_sync_interval_secs: 300,
        }
    }
}

/// Request to report heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportHeartbeatRequest {
    /// Gateway ID
    pub gateway_id: Option<String>,
    /// CPU usage percent
    pub cpu_usage_percent: Option<f64>,
    /// Memory usage percent
    pub memory_usage_percent: Option<f64>,
    /// Disk usage percent
    pub disk_usage_percent: Option<f64>,
    /// Network status
    pub network_status: Option<String>,
    /// Connected devices count
    pub connected_devices: Option<u32>,
    /// Active alarms count
    pub active_alarms: Option<u32>,
    /// Custom metadata
    pub metadata: Option<serde_json::Value>,
}

/// Response after reporting heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportHeartbeatResponse {
    /// Whether the heartbeat was accepted
    pub accepted: bool,
    /// Next heartbeat expected at
    pub next_heartbeat_at: DateTime<Utc>,
    /// Current status
    pub status: String,
}

/// Request to configure heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureHeartbeatRequest {
    /// Probe interval in seconds
    pub probe_interval_secs: Option<u64>,
    /// CPU threshold percent
    pub cpu_threshold_percent: Option<f64>,
    /// Memory threshold percent
    pub memory_threshold_percent: Option<f64>,
    /// Disk threshold percent
    pub disk_threshold_percent: Option<f64>,
    /// Cloud sync enabled
    pub cloud_sync_enabled: Option<bool>,
    /// Cloud sync interval in seconds
    pub cloud_sync_interval_secs: Option<u64>,
}

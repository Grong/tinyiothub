// Heartbeat Tools Module
// MCP tools for gateway heartbeat management

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::modules::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};
use crate::modules::heartbeat::types::{
    HeartbeatConfig, HeartbeatStatus,
    ReportHeartbeatResponse,
};

/// Global heartbeat state
static HEARTBEAT_STATUS: OnceLock<Arc<RwLock<HeartbeatStatus>>> = OnceLock::new();

static HEARTBEAT_CONFIG: OnceLock<Arc<RwLock<HeartbeatConfig>>> = OnceLock::new();

/// Initialize global heartbeat state
pub fn init_heartbeat_state() -> (Arc<RwLock<HeartbeatStatus>>, Arc<RwLock<HeartbeatConfig>>) {
    let status = HEARTBEAT_STATUS
        .get_or_init(|| Arc::new(RwLock::new(HeartbeatStatus::default())))
        .clone();
    let config = HEARTBEAT_CONFIG
        .get_or_init(|| Arc::new(RwLock::new(HeartbeatConfig::default())))
        .clone();
    (status, config)
}

/// Get heartbeat status
pub fn get_heartbeat_status() -> Option<Arc<RwLock<HeartbeatStatus>>> {
    HEARTBEAT_STATUS.get().cloned()
}

/// Get heartbeat config
pub fn get_heartbeat_config() -> Option<Arc<RwLock<HeartbeatConfig>>> {
    HEARTBEAT_CONFIG.get().cloned()
}

/// Tool input: Report heartbeat
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct ReportHeartbeatInput {
    gateway_id: Option<String>,
    cpu_usage_percent: Option<f64>,
    memory_usage_percent: Option<f64>,
    disk_usage_percent: Option<f64>,
    network_status: Option<String>,
    connected_devices: Option<u32>,
    active_alarms: Option<u32>,
    metadata: Option<Value>,
}

/// Tool input: Get heartbeat status
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct GetHeartbeatStatusInput {
    // No required parameters
}

/// Tool input: Configure heartbeat
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureHeartbeatInput {
    probe_interval_secs: Option<u64>,
    cpu_threshold_percent: Option<f64>,
    memory_threshold_percent: Option<f64>,
    disk_threshold_percent: Option<f64>,
    cloud_sync_enabled: Option<bool>,
    cloud_sync_interval_secs: Option<u64>,
}

/// Report heartbeat tool handler
pub struct ReportHeartbeatHandler;

#[async_trait]
impl ToolHandler for ReportHeartbeatHandler {
    fn name(&self) -> &str {
        "report_heartbeat"
    }

    fn description(&self) -> &str {
        "Report gateway heartbeat status to the cloud. This tool pushes the current gateway health status including CPU, memory, disk usage, and connected device count."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "gatewayId".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Gateway identifier".to_string()),
            },
        );
        props.insert(
            "cpuUsagePercent".to_string(),
            PropertySchema {
                prop_type: "number".to_string(),
                description: Some("CPU usage percentage (0-100)".to_string()),
            },
        );
        props.insert(
            "memoryUsagePercent".to_string(),
            PropertySchema {
                prop_type: "number".to_string(),
                description: Some("Memory usage percentage (0-100)".to_string()),
            },
        );
        props.insert(
            "diskUsagePercent".to_string(),
            PropertySchema {
                prop_type: "number".to_string(),
                description: Some("Disk usage percentage (0-100)".to_string()),
            },
        );
        props.insert(
            "networkStatus".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("Network connectivity status".to_string()),
            },
        );
        props.insert(
            "connectedDevices".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Number of connected devices".to_string()),
            },
        );
        props.insert(
            "activeAlarms".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Number of active alarms".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ReportHeartbeatInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let status_lock = get_heartbeat_status().ok_or_else(|| {
            ToolError::Internal("Heartbeat status not initialized".to_string())
        })?;
        let config_lock = get_heartbeat_config().ok_or_else(|| {
            ToolError::Internal("Heartbeat config not initialized".to_string())
        })?;

        let mut status = status_lock.write().await;
        let config = config_lock.read().await;

        // Update status
        status.gateway_id = input.gateway_id.unwrap_or_else(|| status.gateway_id.clone());
        status.timestamp = Utc::now();
        status.cpu_usage_percent = input.cpu_usage_percent.unwrap_or(status.cpu_usage_percent);
        status.memory_usage_percent =
            input.memory_usage_percent.unwrap_or(status.memory_usage_percent);
        status.disk_usage_percent = input.disk_usage_percent.unwrap_or(status.disk_usage_percent);
        status.network_status = input
            .network_status
            .unwrap_or_else(|| status.network_status.clone());
        status.connected_devices = input.connected_devices.unwrap_or(status.connected_devices);
        status.active_alarms = input.active_alarms.unwrap_or(status.active_alarms);
        status.last_cloud_sync = Some(Utc::now());
        status.error_message = None;

        // Calculate overall status based on thresholds
        status.status = calculate_health_status(&status, &config);

        let next_heartbeat =
            status.timestamp + chrono::Duration::seconds(config.probe_interval_secs as i64);

        let response = ReportHeartbeatResponse {
            accepted: true,
            next_heartbeat_at: next_heartbeat,
            status: status.status.clone(),
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}

/// Get heartbeat status tool handler
pub struct GetHeartbeatStatusHandler;

#[async_trait]
impl ToolHandler for GetHeartbeatStatusHandler {
    fn name(&self) -> &str {
        "get_heartbeat_status"
    }

    fn description(&self) -> &str {
        "Get the current gateway heartbeat status including health metrics, uptime, and cloud sync status."
    }

    fn input_schema(&self) -> InputSchema {
        InputSchema::object(vec![], HashMap::new())
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        // Args are ignored for get status
        let _ = args;

        let status_lock = get_heartbeat_status().ok_or_else(|| {
            ToolError::Internal("Heartbeat status not initialized".to_string())
        })?;
        let config_lock = get_heartbeat_config().ok_or_else(|| {
            ToolError::Internal("Heartbeat config not initialized".to_string())
        })?;

        let status = status_lock.read().await;
        let config = config_lock.read().await;

        // Recalculate health status
        let current_status = calculate_health_status(&status, &config);

        let response = HeartbeatStatus {
            gateway_id: status.gateway_id.clone(),
            status: current_status,
            timestamp: status.timestamp,
            cpu_usage_percent: status.cpu_usage_percent,
            memory_usage_percent: status.memory_usage_percent,
            disk_usage_percent: status.disk_usage_percent,
            network_status: status.network_status.clone(),
            connected_devices: status.connected_devices,
            active_alarms: status.active_alarms,
            uptime_seconds: status.uptime_seconds,
            last_cloud_sync: status.last_cloud_sync,
            error_message: status.error_message.clone(),
        };

        Ok(serde_json::to_value(response).unwrap())
    }
}

/// Configure heartbeat tool handler
pub struct ConfigureHeartbeatHandler;

#[async_trait]
impl ToolHandler for ConfigureHeartbeatHandler {
    fn name(&self) -> &str {
        "configure_heartbeat"
    }

    fn description(&self) -> &str {
        "Configure the heartbeat probe interval and thresholds for CPU, memory, and disk usage warnings."
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "probeIntervalSecs".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Probe interval in seconds".to_string()),
            },
        );
        props.insert(
            "cpuThresholdPercent".to_string(),
            PropertySchema {
                prop_type: "number".to_string(),
                description: Some("CPU threshold percentage (0-100)".to_string()),
            },
        );
        props.insert(
            "memoryThresholdPercent".to_string(),
            PropertySchema {
                prop_type: "number".to_string(),
                description: Some("Memory threshold percentage (0-100)".to_string()),
            },
        );
        props.insert(
            "diskThresholdPercent".to_string(),
            PropertySchema {
                prop_type: "number".to_string(),
                description: Some("Disk threshold percentage (0-100)".to_string()),
            },
        );
        props.insert(
            "cloudSyncEnabled".to_string(),
            PropertySchema {
                prop_type: "boolean".to_string(),
                description: Some("Whether cloud sync is enabled".to_string()),
            },
        );
        props.insert(
            "cloudSyncIntervalSecs".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("Cloud sync interval in seconds".to_string()),
            },
        );
        InputSchema::object(vec![], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ConfigureHeartbeatInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let config_lock = get_heartbeat_config().ok_or_else(|| {
            ToolError::Internal("Heartbeat config not initialized".to_string())
        })?;

        let mut config = config_lock.write().await;

        if let Some(interval) = input.probe_interval_secs {
            config.probe_interval_secs = interval;
        }
        if let Some(threshold) = input.cpu_threshold_percent {
            config.cpu_threshold_percent = threshold;
        }
        if let Some(threshold) = input.memory_threshold_percent {
            config.memory_threshold_percent = threshold;
        }
        if let Some(threshold) = input.disk_threshold_percent {
            config.disk_threshold_percent = threshold;
        }
        if let Some(enabled) = input.cloud_sync_enabled {
            config.cloud_sync_enabled = enabled;
        }
        if let Some(interval) = input.cloud_sync_interval_secs {
            config.cloud_sync_interval_secs = interval;
        }

        Ok(serde_json::to_value(config.clone()).unwrap())
    }
}

/// Calculate health status based on metrics and thresholds
fn calculate_health_status(status: &HeartbeatStatus, config: &HeartbeatConfig) -> String {
    // Check critical thresholds
    if status.cpu_usage_percent > config.cpu_threshold_percent {
        return "warning".to_string();
    }
    if status.memory_usage_percent > config.memory_threshold_percent {
        return "warning".to_string();
    }
    if status.disk_usage_percent > config.disk_threshold_percent {
        return "warning".to_string();
    }
    if status.network_status != "connected" {
        return "degraded".to_string();
    }
    if status.active_alarms > 0 {
        return "warning".to_string();
    }

    "healthy".to_string()
}

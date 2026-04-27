// Heartbeat API — moved from api/heartbeat/

use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use crate::modules::heartbeat::types::{
    ConfigureHeartbeatRequest, HeartbeatConfig, HeartbeatStatus, ReportHeartbeatResponse,
};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;

use crate::{
    modules::heartbeat::{get_heartbeat_config, get_heartbeat_status},
    shared::api_response::ApiResponse,
    shared::app_state::AppState,
};

/// Create the heartbeat router
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", post(report_heartbeat).get(get_heartbeat))
        .route("/config", get(get_config).put(configure_heartbeat))
}

/// Request to report heartbeat via API
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ReportHeartbeatApiRequest {
    pub gateway_id: Option<String>,
    pub cpu_usage_percent: Option<f64>,
    pub memory_usage_percent: Option<f64>,
    pub disk_usage_percent: Option<f64>,
    pub network_status: Option<String>,
    pub connected_devices: Option<u32>,
    pub active_alarms: Option<u32>,
    pub uptime_seconds: Option<u64>,
    pub metadata: Option<serde_json::Value>,
}

/// Report heartbeat endpoint
async fn report_heartbeat(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(request): Json<ReportHeartbeatApiRequest>,
) -> Json<ApiResponse<ReportHeartbeatResponse>> {
    let status_lock = match get_heartbeat_status() {
        Some(s) => s,
        None => {
            return ApiResponseBuilder::error("Heartbeat system not initialized");
        }
    };

    let config_lock = match get_heartbeat_config() {
        Some(c) => c,
        None => {
            return ApiResponseBuilder::error("Heartbeat config not initialized");
        }
    };

    let mut status = status_lock.write().await;
    let config = config_lock.read().await;

    // Update status
    status.gateway_id = request.gateway_id.unwrap_or_else(|| status.gateway_id.clone());
    status.timestamp = Utc::now();
    status.cpu_usage_percent = request.cpu_usage_percent.unwrap_or(status.cpu_usage_percent);
    status.memory_usage_percent = request.memory_usage_percent.unwrap_or(status.memory_usage_percent);
    status.disk_usage_percent = request.disk_usage_percent.unwrap_or(status.disk_usage_percent);
    status.network_status = request
        .network_status
        .unwrap_or_else(|| status.network_status.clone());
    status.connected_devices = request.connected_devices.unwrap_or(status.connected_devices);
    status.active_alarms = request.active_alarms.unwrap_or(status.active_alarms);
    status.uptime_seconds = request.uptime_seconds.unwrap_or(status.uptime_seconds);
    status.last_cloud_sync = Some(Utc::now());
    status.error_message = None;

    // Calculate overall status
    status.status = calculate_health_status(&status, &config);

    let next_heartbeat =
        status.timestamp + chrono::Duration::seconds(config.probe_interval_secs as i64);

    let response = ReportHeartbeatResponse {
        accepted: true,
        next_heartbeat_at: next_heartbeat,
        status: status.status.clone(),
    };

    ApiResponseBuilder::success(response)
}

/// Get heartbeat status endpoint
async fn get_heartbeat(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<HeartbeatStatus>> {
    let status_lock = match get_heartbeat_status() {
        Some(s) => s,
        None => {
            return ApiResponseBuilder::error("Heartbeat system not initialized");
        }
    };

    let config_lock = match get_heartbeat_config() {
        Some(c) => c,
        None => {
            return ApiResponseBuilder::error("Heartbeat config not initialized");
        }
    };

    let status = status_lock.read().await;
    let config = config_lock.read().await;

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

    ApiResponseBuilder::success(response)
}

/// Get heartbeat configuration endpoint
async fn get_config(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<HeartbeatConfig>> {
    let config_lock = match get_heartbeat_config() {
        Some(c) => c,
        None => {
            return ApiResponseBuilder::error("Heartbeat config not initialized");
        }
    };

    let config = config_lock.read().await;
    ApiResponseBuilder::success(config.clone())
}

/// Configure heartbeat endpoint
async fn configure_heartbeat(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(request): Json<ConfigureHeartbeatRequest>,
) -> Json<ApiResponse<HeartbeatConfig>> {
    let config_lock = match get_heartbeat_config() {
        Some(c) => c,
        None => {
            return ApiResponseBuilder::error("Heartbeat config not initialized");
        }
    };

    let mut config = config_lock.write().await;

    if let Some(interval) = request.probe_interval_secs {
        config.probe_interval_secs = interval;
    }
    if let Some(threshold) = request.cpu_threshold_percent {
        config.cpu_threshold_percent = threshold;
    }
    if let Some(threshold) = request.memory_threshold_percent {
        config.memory_threshold_percent = threshold;
    }
    if let Some(threshold) = request.disk_threshold_percent {
        config.disk_threshold_percent = threshold;
    }
    if let Some(enabled) = request.cloud_sync_enabled {
        config.cloud_sync_enabled = enabled;
    }
    if let Some(interval) = request.cloud_sync_interval_secs {
        config.cloud_sync_interval_secs = interval;
    }

    ApiResponseBuilder::success(config.clone())
}

/// Calculate health status based on metrics and thresholds
fn calculate_health_status(status: &HeartbeatStatus, config: &HeartbeatConfig) -> String {
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

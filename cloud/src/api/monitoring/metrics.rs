use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{api::AppState, dto::response::{ApiResponse, ApiResponseBuilder}, shared::security::jwt::Claims};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub disk_usage_mb: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DeviceMetrics {
    pub device_id: String,
    pub device_name: String,
    pub status: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub message_count: u64,
    pub error_count: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct GatewayMetrics {
    pub total_devices: u32,
    pub online_devices: u32,
    pub offline_devices: u32,
    pub total_messages: u64,
    pub messages_per_minute: f64,
    pub error_rate_percent: f64,
    pub uptime_seconds: u64,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/system", get(get_system_metrics))
        .route("/devices", get(get_device_metrics))
        .route("/gateway", get(get_gateway_metrics))
}

/// 获取系统指标
async fn get_system_metrics(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<SystemMetrics>> {
    // TODO: 实现实际的系统指标收集
    let metrics = SystemMetrics {
        cpu_usage_percent: 0.0,
        memory_usage_mb: 0,
        disk_usage_mb: 0,
        network_rx_bytes: 0,
        network_tx_bytes: 0,
        timestamp: chrono::Utc::now(),
    };

    ApiResponseBuilder::success(metrics)
}

/// 获取设备指标
async fn get_device_metrics(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<DeviceMetrics>>> {
    // TODO: 实现设备指标收集
    let metrics = vec![];

    ApiResponseBuilder::success(metrics)
}

/// 获取网关指标
async fn get_gateway_metrics(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<GatewayMetrics>> {
    // TODO: 实现网关指标收集
    let metrics = GatewayMetrics {
        total_devices: 0,
        online_devices: 0,
        offline_devices: 0,
        total_messages: 0,
        messages_per_minute: 0.0,
        error_rate_percent: 0.0,
        uptime_seconds: 0,
    };

    ApiResponseBuilder::success(metrics)
}

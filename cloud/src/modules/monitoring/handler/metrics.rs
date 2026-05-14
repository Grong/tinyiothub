use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::shared::{
    api_response::ApiResponse, app_state::AppState, error_handling::AuthHelper,
    security::jwt::Claims,
};

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

/// 获取系统指标（仅管理员可访问）
async fn get_system_metrics(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<SystemMetrics>> {
    match AuthHelper::require_admin_role(&state, &claims.user_id, "get_system_metrics").await {
        Ok(()) => {}
        Err(_) => {
            return ApiResponseBuilder::error_with_code(403, "Access denied: admin role required");
        }
    }
    let mut sys = state.sysinfo_system.lock().unwrap();
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpus = sys.cpus();
    let cpu_usage_percent = if !cpus.is_empty() {
        cpus.iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / cpus.len() as f32
    } else {
        0.0
    };

    let memory_usage_mb = sys.used_memory() / 1024;

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let disk_usage_mb = disks
        .iter()
        .map(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            (total - available) / 1024 / 1024
        })
        .sum();

    let networks = sysinfo::Networks::new_with_refreshed_list();
    let (network_rx_bytes, network_tx_bytes) = networks
        .values()
        .map(|network| (network.total_received(), network.total_transmitted()))
        .fold((0, 0), |(acc_rx, acc_tx), (rx, tx)| (acc_rx + rx, acc_tx + tx));

    let metrics = SystemMetrics {
        cpu_usage_percent: cpu_usage_percent as f64,
        memory_usage_mb,
        disk_usage_mb,
        network_rx_bytes,
        network_tx_bytes,
        timestamp: chrono::Utc::now(),
    };

    ApiResponseBuilder::success(metrics)
}

/// 获取设备指标
async fn get_device_metrics(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<DeviceMetrics>>> {
    let workspace_id = match state.resolve_workspace(&claims.tenant_id, None).await {
        Ok(ws) => Some(ws),
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };

    let device_service = state.tenant_device_service(&workspace_id);

    let mut metrics = Vec::new();

    match device_service
        .get_devices(&tinyiothub_core::models::device::DeviceQueryParams::default())
        .await
    {
        Ok(devices) => {
            for device in devices {
                let status = device.status.to_string();
                let last_seen = device
                    .last_heartbeat
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                metrics.push(DeviceMetrics {
                    device_id: device.id.clone(),
                    device_name: device.name.clone(),
                    status,
                    last_seen,
                    message_count: 0,
                    error_count: 0,
                });
            }
        }
        Err(e) => {
            tracing::warn!("Failed to get device metrics: {}", e);
        }
    }

    ApiResponseBuilder::success(metrics)
}

/// 获取网关指标
async fn get_gateway_metrics(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<GatewayMetrics>> {
    let workspace_id = match state.resolve_workspace(&claims.tenant_id, None).await {
        Ok(ws) => Some(ws),
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };

    let device_service = state.tenant_device_service(&workspace_id);

    let mut total_devices = 0u32;
    let mut online_devices = 0u32;
    let mut offline_devices = 0u32;

    match device_service
        .get_devices(&tinyiothub_core::models::device::DeviceQueryParams::default())
        .await
    {
        Ok(devices) => {
            total_devices = devices.len() as u32;
            online_devices = devices.iter().filter(|d| d.status.is_online()).count() as u32;
            offline_devices = total_devices - online_devices;
        }
        Err(e) => {
            tracing::warn!("Failed to get device counts for gateway metrics: {}", e);
        }
    }

    // Real uptime from global start time
    let uptime_seconds = crate::modules::monitoring::handler::health::START_TIME
        .get()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let metrics = GatewayMetrics {
        total_devices,
        online_devices,
        offline_devices,
        // TODO: implement real message counting when message pipeline metrics are available
        total_messages: 0,
        messages_per_minute: 0.0,
        error_rate_percent: 0.0,
        uptime_seconds,
    };

    ApiResponseBuilder::success(metrics)
}

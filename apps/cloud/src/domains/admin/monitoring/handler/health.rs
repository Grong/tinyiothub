use crate::domains::admin::AdminState;

use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use tinyiothub_web::api_response::ApiResponse;

/// Global start time for uptime calculation (shared with metrics)
fn get_uptime_seconds(state: &AdminState) -> u64 {
    state.started_at.elapsed().unwrap_or_default().as_secs()
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DetailedHealthStatus {
    pub overall_status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: u64,
    pub database_status: String,
    pub mqtt_status: String,
    pub device_count: u32,
    pub active_device_count: u32,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(get_health))
        .route("/detailed", get(get_detailed_health))
}

/// 基础健康检查
async fn get_health(State(state): State<AdminState>) -> Json<ApiResponse<HealthStatus>> {
    let db_status = state.db().ping().await;

    let status = match db_status {
        Ok(_) => "healthy",
        Err(_) => "degraded",
    };

    let health = HealthStatus {
        status: status.to_string(),
        timestamp: chrono::Utc::now(),
        uptime_seconds: get_uptime_seconds(&state),
    };

    ApiResponseBuilder::success(health)
}

/// 详细健康状态
async fn get_detailed_health(
    State(state): State<AdminState>,
    claims: Claims,
) -> Json<ApiResponse<DetailedHealthStatus>> {
    let db_status = state.db().ping().await;

    let (overall_status, database_status) = match db_status {
        Ok(_) => ("healthy", "connected"),
        Err(_) => ("degraded", "disconnected"),
    };

    let workspace_id = match state.resolve_workspace(&claims.tenant_id, None).await {
        Ok(ws) => Some(ws),
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };

    let device_service = state.tenant_device_service(&workspace_id);

    let mut device_count = 0u32;
    let mut active_device_count = 0u32;

    match device_service
        .count_devices(&tinyiothub_core::models::device::DeviceQueryParams::default())
        .await
    {
        Ok(count) => device_count = count as u32,
        Err(e) => tracing::warn!("Failed to get device count for health check: {}", e),
    }

    match device_service
        .get_devices(&tinyiothub_core::models::device::DeviceQueryParams::default())
        .await
    {
        Ok(things) => {
            active_device_count = things.iter().filter(|d| d.status.is_online()).count() as u32;
        }
        Err(e) => {
            tracing::warn!("Failed to get device list for health check: {}", e);
        }
    }

    // Query real system metrics via cached sysinfo
    let mut sys = state.sysinfo_system.lock().unwrap();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let cpu_usage_percent = if !sys.cpus().is_empty() {
        sys.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
    } else {
        0.0
    };
    let memory_usage_mb = sys.used_memory() / 1024;
    drop(sys);

    // MQTT status: no direct health check available, report honestly
    let mqtt_status = if state.data_server().is_some() {
        "available"
    } else {
        "unavailable"
    };

    let detailed_health = DetailedHealthStatus {
        overall_status: overall_status.to_string(),
        timestamp: chrono::Utc::now(),
        uptime_seconds: get_uptime_seconds(&state),
        database_status: database_status.to_string(),
        mqtt_status: mqtt_status.to_string(),
        device_count,
        active_device_count,
        memory_usage_mb,
        cpu_usage_percent: cpu_usage_percent as f64,
    };

    ApiResponseBuilder::success(detailed_health)
}

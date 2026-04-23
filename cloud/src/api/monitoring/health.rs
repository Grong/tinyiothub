use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{api::AppState, dto::response::ApiResponse};

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

pub fn create_router() -> Router<AppState> {
    Router::new().route("/", get(get_health)).route("/detailed", get(get_detailed_health))
}

/// 基础健康检查
async fn get_health(State(_state): State<AppState>) -> Json<ApiResponse<HealthStatus>> {
    let health = HealthStatus {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
        uptime_seconds: 0, // TODO: 实现实际的运行时间计算
    };

    ApiResponseBuilder::success(health)
}

/// 详细健康状态
async fn get_detailed_health(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<DetailedHealthStatus>> {
    // TODO: 实现详细健康状态检查逻辑
    let detailed_health = DetailedHealthStatus {
        overall_status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
        uptime_seconds: 0,
        database_status: "connected".to_string(),
        mqtt_status: "connected".to_string(),
        device_count: 0,
        active_device_count: 0,
        memory_usage_mb: 0,
        cpu_usage_percent: 0.0,
    };

    ApiResponseBuilder::success(detailed_health)
}

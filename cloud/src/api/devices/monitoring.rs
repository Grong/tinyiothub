use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::device::{
        monitoring_service::{DeviceMetrics, SystemOverview},
        performance_service::{
            DevicePerformanceMetrics, PerformanceAlert, SystemPerformanceOverview,
        },
    },
    dto::response::{ApiResponse},
    shared::{app_state::AppState},
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PerformanceHistoryQuery {
    pub hours: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceOnlineStatus {
    pub device_id: String,
    pub is_online: bool,
    pub connection_quality: Option<u8>,
    pub last_check: String,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        // 设备状态相关
        .route("/{device_id}/status", get(get_device_online_status))
        .route("/{device_id}/metrics", get(get_device_metrics))
        // 性能监控相关
        .route("/{device_id}/performance", get(get_device_performance_metrics))
        .route("/{device_id}/performance/history", get(get_device_performance_history))
        .route("/{device_id}/performance/alerts", get(get_device_performance_alerts))
        // 系统级监控
        .route("/overview", get(get_system_overview))
        .route("/performance/overview", get(get_system_performance_overview))
        .route("/performance/alerts", get(get_all_performance_alerts))
}

/// 获取设备在线状态
async fn get_device_online_status(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<DeviceOnlineStatus>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id. The adapter ensures
    // that all device queries are scoped to the current workspace, eliminating
    // the need for explicit tenant verification in API handlers.
    let is_online = state.monitoring_service.is_device_online(&device_id);
    let connection_quality = state.monitoring_service.get_device_connection_quality(&device_id);

    let status = DeviceOnlineStatus {
        device_id: device_id.clone(),
        is_online,
        connection_quality,
        last_check: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    ApiResponseBuilder::success(status)
}

/// 获取设备指标信息
async fn get_device_metrics(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DeviceMetrics>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id. The adapter ensures
    // that all device queries are scoped to the current workspace, eliminating
    // the need for explicit tenant verification in API handlers.
    match state.monitoring_service.get_device_metrics(&device_id).await {
        Some(stats) => ApiResponseBuilder::success(Some(stats)),
        None => ApiResponseBuilder::success(None),
    }
}

/// 获取系统概览
async fn get_system_overview(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<SystemOverview>> {
    let overview = state.monitoring_service.get_system_overview().await;
    ApiResponseBuilder::success(overview)
}

/// 获取设备性能指标
async fn get_device_performance_metrics(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DevicePerformanceMetrics>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id. The adapter ensures
    // that all device queries are scoped to the current workspace, eliminating
    // the need for explicit tenant verification in API handlers.
    match state.performance_service.get_device_performance_metrics(&device_id).await {
        Some(metrics) => ApiResponseBuilder::success(Some(metrics)),
        None => ApiResponseBuilder::success(None),
    }
}

/// 获取设备性能历史数据
async fn get_device_performance_history(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(params): Query<PerformanceHistoryQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<DevicePerformanceMetrics>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id. The adapter ensures
    // that all device queries are scoped to the current workspace, eliminating
    // the need for explicit tenant verification in API handlers.
    let hours = params.hours.unwrap_or(24); // 默认24小时
    match state.performance_service.get_device_performance_history(&device_id, hours).await {
        Ok(history) => ApiResponseBuilder::success(history),
        Err(e) => {
            tracing::error!("Failed to get performance history for {}: {}", device_id, e);
            match e {
                crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
                _ => ApiResponseBuilder::error("获取性能历史数据失败"),
            }
        }
    }
}

/// 获取系统性能概览
async fn get_system_performance_overview(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<SystemPerformanceOverview>> {
    let overview = state.performance_service.get_system_performance_overview().await;
    ApiResponseBuilder::success(overview)
}

/// 获取设备性能告警
async fn get_device_performance_alerts(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<PerformanceAlert>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id. The adapter ensures
    // that all device queries are scoped to the current workspace, eliminating
    // the need for explicit tenant verification in API handlers.
    let alerts = state.performance_service.check_device_performance_alerts(&device_id).await;
    ApiResponseBuilder::success(alerts)
}

/// 获取所有设备性能告警
async fn get_all_performance_alerts(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<PerformanceAlert>>> {
    // 获取所有设备的告警
    let all_devices = state.device_cache.all();
    let mut all_alerts = Vec::new();

    for device in all_devices {
        let alerts = state.performance_service.check_device_performance_alerts(&device.id).await;
        all_alerts.extend(alerts);
    }

    // 按严重程度排序（critical 在前）
    all_alerts.sort_by(|a, b| {
        match (a.severity.as_str(), b.severity.as_str()) {
            ("critical", "warning") => std::cmp::Ordering::Less,
            ("warning", "critical") => std::cmp::Ordering::Greater,
            _ => a.timestamp.cmp(&b.timestamp).reverse(), // 最新的在前
        }
    });

    ApiResponseBuilder::success(all_alerts)
}

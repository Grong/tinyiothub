use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::device::trace_service::{DeviceTrace, DeviceTraceStatistics, SystemTraceOverview},
    shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims},
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecordTraceRequest {
    pub trace_type: String,
    pub level: String,
    pub category: String,
    pub title: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct TraceQuery {
    pub trace_types: Option<Vec<String>>,
    pub levels: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Deserialize)]
pub struct TraceStatisticsQuery {
    pub days: Option<u32>,
}

#[derive(Deserialize)]
pub struct ClearTracesRequest {
    pub before_date: Option<String>,
    pub trace_types: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct CleanupTracesRequest {
    pub days_to_keep: u32,
}

#[derive(Deserialize)]
pub struct SystemTraceQuery {
    pub days: Option<u32>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/{device_id}/traces", post(record_device_trace))
        .route("/{device_id}/traces", get(get_device_traces))
        .route("/{device_id}/traces/statistics", get(get_device_trace_summary))
        .route("/{device_id}/traces/clear", post(clear_device_traces))
        .route("/system/traces/overview", get(get_system_trace_overview))
        .route("/system/traces/cleanup", post(cleanup_expired_traces))
}

/// 记录设备追踪信息
async fn record_device_trace(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
    Json(req): Json<RecordTraceRequest>,
) -> Json<ApiResponse<String>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id
    match state
        .trace_service
        .record_device_trace(
            &device_id,
            &req.trace_type,
            &req.level,
            &req.category,
            &req.title,
            &req.message,
            req.details,
            req.source.as_deref(),
            req.user_id.as_deref(),
            req.session_id.as_deref(),
        )
        .await
    {
        Ok(trace_id) => ApiResponseBuilder::success(trace_id),
        Err(e) => {
            tracing::error!("Failed to record device trace: {}", e);
            match e {
                crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
                _ => ApiResponseBuilder::error("记录追踪信息失败"),
            }
        }
    }
}

/// 获取设备追踪记录
async fn get_device_traces(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(params): Query<TraceQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<DeviceTrace>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id
    let trace_types = params.trace_types.as_deref();
    let levels = params.levels.as_deref();

    match state
        .trace_service
        .get_device_traces(&device_id, trace_types, levels, params.limit, params.offset)
        .await
    {
        Ok(traces) => ApiResponseBuilder::success(traces),
        Err(e) => {
            tracing::error!("Failed to get device traces: {}", e);
            match e {
                crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
                _ => ApiResponseBuilder::error("获取追踪记录失败"),
            }
        }
    }
}

/// 获取设备追踪记录摘要
async fn get_device_trace_summary(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(params): Query<TraceStatisticsQuery>,
    _claims: Claims,
) -> Json<ApiResponse<DeviceTraceStatistics>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id
    match state.trace_service.get_device_trace_statistics(&device_id, params.days).await {
        Ok(stats) => ApiResponseBuilder::success(stats),
        Err(e) => {
            tracing::error!("Failed to get device trace statistics: {}", e);
            match e {
                crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
                _ => ApiResponseBuilder::error("获取追踪统计失败"),
            }
        }
    }
}

/// 清理设备追踪记录
async fn clear_device_traces(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
    Json(req): Json<ClearTracesRequest>,
) -> Json<ApiResponse<u32>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id
    let trace_types = req.trace_types.as_deref();

    match state
        .trace_service
        .clear_device_traces(&device_id, req.before_date.as_deref(), trace_types)
        .await
    {
        Ok(cleared_count) => ApiResponseBuilder::success(cleared_count),
        Err(e) => {
            tracing::error!("Failed to clear device traces: {}", e);
            ApiResponseBuilder::error("清理追踪记录失败")
        }
    }
}

/// 获取系统追踪记录概览
async fn get_system_trace_overview(
    State(state): State<AppState>,
    Query(params): Query<SystemTraceQuery>,
    _claims: Claims,
) -> Json<ApiResponse<SystemTraceOverview>> {
    let overview = state.trace_service.get_system_trace_overview(params.days).await;
    ApiResponseBuilder::success(overview)
}

/// 清理过期的追踪记录
async fn cleanup_expired_traces(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<CleanupTracesRequest>,
) -> Json<ApiResponse<u32>> {
    match state.trace_service.cleanup_expired_traces(req.days_to_keep).await {
        Ok(cleaned_count) => ApiResponseBuilder::success(cleaned_count),
        Err(e) => {
            tracing::error!("Failed to cleanup expired traces: {}", e);
            ApiResponseBuilder::error("清理过期记录失败")
        }
    }
}

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::shared::{
    api_response::ApiResponse, app_state::AppState, pagination::PaginationQuery,
    security::jwt::Claims,
};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub message: String,
    pub source: String,
    pub device_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct LogLevel {
    pub name: String,
    pub description: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct LogQuery {
    pub level: Option<String>,
    pub source: Option<String>,
    pub device_id: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub fn create_router() -> Router<AppState> {
    Router::new().route("/", get(get_logs)).route("/levels", get(get_log_levels))
}

/// 获取日志列表（已启用 workspace 隔离）
async fn get_logs(
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
    claims: Claims,
) -> Json<ApiResponse<Vec<LogEntry>>> {
    // Resolve workspace and fetch allowed device IDs
    let workspace_id = match state.resolve_workspace(&claims.tenant_id, None).await {
        Ok(ws) => Some(ws),
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code, &msg),
    };

    let device_service = state.tenant_device_service(&workspace_id);

    let allowed_device_ids: Vec<String> = match device_service
        .get_devices(&tinyiothub_core::models::device::DeviceQueryParams::default())
        .await
    {
        Ok(devices) => devices.into_iter().map(|d| d.id).collect(),
        Err(e) => {
            tracing::warn!("Failed to get devices for log workspace isolation: {}", e);
            return ApiResponseBuilder::error("获取日志失败".to_string());
        }
    };

    // If user specified a device_id, verify it belongs to their workspace
    if let Some(ref requested_device_id) = query.device_id
        && !allowed_device_ids.contains(requested_device_id)
    {
        return ApiResponseBuilder::success(vec![]);
    }

    let device_ids_filter = if query.device_id.is_none() && !allowed_device_ids.is_empty() {
        Some(allowed_device_ids)
    } else {
        None
    };

    let levels = query.level.as_ref().map(|l| vec![l.to_lowercase()]);
    let sources = query.source.as_ref().map(|s| vec![s.clone()]);
    let limit = query.pagination.page_size.unwrap_or(50);
    let offset = (query.pagination.page.unwrap_or(1).saturating_sub(1)) * limit;

    match state
        .trace_service
        .find_all_traces(
            levels.as_deref(),
            sources.as_deref(),
            query.device_id.as_deref(),
            device_ids_filter.as_deref(),
            query.start_time.as_deref(),
            query.end_time.as_deref(),
            Some(limit),
            Some(offset),
        )
        .await
    {
        Ok(traces) => {
            let logs: Vec<LogEntry> = traces
                .into_iter()
                .map(|t| LogEntry {
                    id: t.id,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&t.created_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    level: t.level.to_uppercase(),
                    message: format!("{}: {}", t.title, t.message),
                    source: t.source.unwrap_or(t.category),
                    device_id: Some(t.device_id),
                })
                .collect();
            ApiResponseBuilder::success(logs)
        }
        Err(e) => {
            tracing::warn!("Failed to get logs: {}", e);
            ApiResponseBuilder::error("获取日志失败".to_string())
        }
    }
}

/// 获取日志级别列表
async fn get_log_levels(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<LogLevel>>> {
    let levels = vec![
        LogLevel { name: "ERROR".to_string(), description: "错误级别".to_string() },
        LogLevel { name: "WARN".to_string(), description: "警告级别".to_string() },
        LogLevel { name: "INFO".to_string(), description: "信息级别".to_string() },
        LogLevel { name: "DEBUG".to_string(), description: "调试级别".to_string() },
    ];

    ApiResponseBuilder::success(levels)
}

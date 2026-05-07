use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    shared::app_state::AppState,
    shared::pagination::PaginationQuery, shared::api_response::ApiResponse,
};
use crate::shared::security::jwt::Claims;

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

/// 获取日志列表
async fn get_logs(
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<LogEntry>>> {
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
                    source: t.source.unwrap_or_else(|| t.category),
                    device_id: Some(t.device_id),
                })
                .collect();
            ApiResponseBuilder::success(logs)
        }
        Err(e) => {
            tracing::warn!("Failed to get logs: {}", e);
            ApiResponseBuilder::success(vec![])
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

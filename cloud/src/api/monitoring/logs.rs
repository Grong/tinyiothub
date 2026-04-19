use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::AppState,
    dto::{request::pagination::PaginationQuery, response::ApiResponse},
    shared::security::jwt::Claims,
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

/// 获取日志列表
async fn get_logs(
    State(_state): State<AppState>,
    Query(_query): Query<LogQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<LogEntry>>> {
    // TODO: 实现日志查询逻辑
    tracing::info!("Getting logs with filters");

    let logs = vec![];
    ApiResponse::success(logs)
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

    ApiResponse::success(levels)
}

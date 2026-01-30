use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use chrono::{DateTime, Utc};
use tracing::{info, error};

use crate::{
    api::AppState,
    dto::response::{ApiResponse, RecentAlarm},
    infrastructure::persistence::Database,
    shared::security::jwt::Claims,
};

#[derive(Debug, Deserialize)]
pub struct RecentAlarmsQuery {
    limit: Option<i32>,
}

/// 获取最新告警列表
/// GET /api/alarms/recent
pub async fn get_recent_alarms(
    State(state): State<AppState>,
    Query(query): Query<RecentAlarmsQuery>,
    claims: Claims,
) -> Json<ApiResponse<Vec<RecentAlarm>>> {
    info!("获取最新告警列表, 用户: {}, 限制: {:?}", claims.username, query.limit);

    let db = Database::new(state.db_pool());

    let limit = query.limit.unwrap_or(10);
    match get_recent_alarms_list(&db, limit).await {
        Ok(alarms) => ApiResponse::success(alarms),
        Err(e) => {
            error!("获取最新告警列表失败: {}", e);
            ApiResponse::error("获取最新告警列表失败".to_string())
        }
    }
}

/// 获取最新告警列表
async fn get_recent_alarms_list(db: &Database, limit: i32) -> Result<Vec<RecentAlarm>, sqlx::Error> {
    let alarms = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, chrono::NaiveDateTime, Option<String>)>(
        r#"
        SELECT 
            da.id,
            da.device_id,
            d.name as device_name,
            da.level,
            da.message,
            da.created_at,
            da.status
        FROM device_alarms da
        LEFT JOIN devices d ON da.device_id = d.id
        ORDER BY da.created_at DESC
        LIMIT ?
        "#
    )
    .bind(limit)
    .fetch_all(db.pool())
    .await?;

    let recent_alarms = alarms
        .into_iter()
        .map(|(id, device_id, device_name, level, message, created_at, status)| RecentAlarm {
            id,
            device_id,
            device_name: device_name.unwrap_or_else(|| "未知设备".to_string()),
            level: level.unwrap_or_else(|| "info".to_string()),
            message,
            created_at: created_at.and_utc(),
            status: status.unwrap_or_else(|| "active".to_string()),
        })
        .collect();

    Ok(recent_alarms)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/recent", get(get_recent_alarms))
}
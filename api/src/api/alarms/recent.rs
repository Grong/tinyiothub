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
    api::{middleware::WorkspaceScope, AppState},
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
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<RecentAlarm>>> {
    info!("获取最新告警列表, 用户: {}, 限制: {:?}", claims.username, query.limit);

    let db = Database::new(state.db_pool());

    let limit = query.limit.unwrap_or(10);
    match get_recent_alarms_list(&db, limit, workspace_id.as_deref()).await {
        Ok(alarms) => ApiResponse::success(alarms),
        Err(e) => {
            error!("获取最新告警列表失败: {}", e);
            ApiResponse::error("获取最新告警列表失败".to_string())
        }
    }
}

/// 获取最新告警列表
async fn get_recent_alarms_list(db: &Database, limit: i32, workspace_id: Option<&str>) -> Result<Vec<RecentAlarm>, sqlx::Error> {
    let workspace_filter = workspace_id.map(|_| " WHERE d.workspace_id = ?").unwrap_or("");
    let query_str = format!(
        r#"
        SELECT
            da.id,
            da.device_id,
            d.name as device_name,
            da.alarm_level,
            da.alarm_message,
            da.alarm_time,
            da.is_acknowledged,
            da.is_resolved
        FROM device_alarms da
        LEFT JOIN devices d ON da.device_id = d.id{}
        ORDER BY da.alarm_time DESC
        LIMIT ?
        "#,
        workspace_filter
    );

    let alarms: Vec<(String, String, Option<String>, String, String, chrono::NaiveDateTime, bool, bool)> = if let Some(wid) = workspace_id {
        sqlx::query_as(
            r#"
        SELECT
            da.id,
            da.device_id,
            d.name,
            da.alarm_message,
            da.alarm_level,
            da.alarm_time,
            da.is_acknowledged,
            da.is_resolved
        FROM device_alarms da
        LEFT JOIN devices d ON da.device_id = d.id
        WHERE da.workspace_id = ?
        ORDER BY da.alarm_time DESC
        LIMIT ?"#,
        )
        .bind(wid)
        .bind(limit)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as(
            r#"
        SELECT
            da.id,
            da.device_id,
            d.name,
            da.alarm_message,
            da.alarm_level,
            da.alarm_time,
            da.is_acknowledged,
            da.is_resolved
        FROM device_alarms da
        LEFT JOIN devices d ON da.device_id = d.id
        ORDER BY da.alarm_time DESC
        LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(db.pool())
        .await?
    };

    let recent_alarms = alarms
        .into_iter()
        .map(|(id, device_id, device_name, level, message, alarm_time, is_acknowledged, is_resolved)| {
            let status = if is_resolved {
                "resolved".to_string()
            } else if is_acknowledged {
                "acknowledged".to_string()
            } else {
                "active".to_string()
            };
            RecentAlarm {
                id,
                device_id,
                device_name: device_name.unwrap_or_else(|| "未知设备".to_string()),
                level,
                message,
                created_at: alarm_time.and_utc(),
                status,
            }
        })
        .collect();

    Ok(recent_alarms)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/recent", get(get_recent_alarms))
}
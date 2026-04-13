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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

    async fn create_minimal_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to create in-memory SQLite pool");

        // Create devices table (minimal, for JOIN)
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create devices table");

        // Create minimal table structure without foreign keys
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS device_alarms (
                id TEXT PRIMARY KEY,
                device_id TEXT,
                workspace_id TEXT,
                alarm_level TEXT NOT NULL,
                alarm_message TEXT NOT NULL,
                alarm_time TEXT NOT NULL,
                is_acknowledged INTEGER NOT NULL DEFAULT 0,
                is_resolved INTEGER NOT NULL DEFAULT 0
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create device_alarms table");

        pool
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_empty() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        let result = get_recent_alarms_list(&db, 10, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_returns_alarms() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        // Insert alarm directly
        sqlx::query(
            r#"INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved)
               VALUES ('alarm-001', 'dev-001', 'ws-001', 'warning', 'High temperature', datetime('now'), 0, 0)"#
        )
        .execute(&pool)
        .await
        .expect("insert alarm failed");

        let result = get_recent_alarms_list(&db, 10, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].id, "alarm-001");
        assert_eq!(alarms[0].message, "High temperature");
        assert_eq!(alarms[0].level, "warning");
        assert_eq!(alarms[0].status, "active");
        assert_eq!(alarms[0].device_name, "未知设备");
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_with_workspace_filter() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        // Insert alarms for different workspaces
        sqlx::query(
            r#"INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved)
               VALUES ('alarm-ws1', 'd1', 'ws-001', 'warning', 'WS1 alarm', datetime('now'), 0, 0)"#
        )
        .execute(&pool)
        .await
        .expect("insert ws1 alarm failed");

        sqlx::query(
            r#"INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved)
               VALUES ('alarm-ws2', 'd2', 'ws-002', 'error', 'WS2 alarm', datetime('now'), 0, 0)"#
        )
        .execute(&pool)
        .await
        .expect("insert ws2 alarm failed");

        // Filter by ws-001
        let result = get_recent_alarms_list(&db, 10, Some("ws-001")).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].id, "alarm-ws1");
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_status_resolved() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        sqlx::query(
            r#"INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved)
               VALUES ('alarm-resolved', 'd1', 'ws-001', 'info', 'Resolved', datetime('now'), 1, 1)"#
        )
        .execute(&pool)
        .await
        .expect("insert alarm failed");

        let result = get_recent_alarms_list(&db, 10, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].status, "resolved");
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_status_acknowledged() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        sqlx::query(
            r#"INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved)
               VALUES ('alarm-ack', 'd1', 'ws-001', 'warning', 'Acknowledged', datetime('now'), 1, 0)"#
        )
        .execute(&pool)
        .await
        .expect("insert alarm failed");

        let result = get_recent_alarms_list(&db, 10, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].status, "acknowledged");
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_limit() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        // Insert 5 alarms with different times
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-0', 'd1', 'ws-001', 'info', 'Alarm 0', datetime('now'), 0, 0)")
            .execute(&pool).await.expect("insert alarm 0 failed");
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-1', 'd1', 'ws-001', 'info', 'Alarm 1', datetime('now', '-1 hours'), 0, 0)")
            .execute(&pool).await.expect("insert alarm 1 failed");
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-2', 'd1', 'ws-001', 'info', 'Alarm 2', datetime('now', '-2 hours'), 0, 0)")
            .execute(&pool).await.expect("insert alarm 2 failed");
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-3', 'd1', 'ws-001', 'info', 'Alarm 3', datetime('now', '-3 hours'), 0, 0)")
            .execute(&pool).await.expect("insert alarm 3 failed");
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-4', 'd1', 'ws-001', 'info', 'Alarm 4', datetime('now', '-4 hours'), 0, 0)")
            .execute(&pool).await.expect("insert alarm 4 failed");

        // Request only 3
        let result = get_recent_alarms_list(&db, 3, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 3);
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_ordering() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        // Insert alarms with specific times
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-old', 'd1', 'ws-001', 'info', 'Old alarm', datetime('now', '-2 hours'), 0, 0)")
            .execute(&pool).await.expect("insert old alarm failed");
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-new', 'd1', 'ws-001', 'info', 'New alarm', datetime('now'), 0, 0)")
            .execute(&pool).await.expect("insert new alarm failed");

        let result = get_recent_alarms_list(&db, 10, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 2);
        // Most recent first
        assert_eq!(alarms[0].id, "alarm-new");
        assert_eq!(alarms[1].id, "alarm-old");
    }
}

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
    let alarms: Vec<(String, String, Option<String>, String, String, chrono::NaiveDateTime, bool, bool)> = if let Some(wid) = workspace_id {
        sqlx::query_as(
            r#"
        SELECT
            da.id,
            da.device_id,
            d.name,
            da.alarm_level,
            da.alarm_message,
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
            da.alarm_level,
            da.alarm_message,
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

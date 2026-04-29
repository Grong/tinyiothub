// Alarm HTTP handlers — query + recent + alarm rules CRUD

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::modules::alarm::types::{AlarmDto, AlarmRuleDto, AlarmStatisticsDto,
    AlarmQueryParams, CreateAlarmRuleRequest, StatisticsQueryParams, ToggleRuleRequest,
    UpdateAlarmRuleRequest};
use crate::shared::api_response::{ApiResponse, PaginatedResponse, PaginationInfo};
use crate::modules::monitoring::types::RecentAlarm;
use crate::shared::app_state::AppState;
use crate::shared::error_handling::ErrorCode;
use crate::shared::security::jwt::Claims;

use super::types::*;
use super::repo::{AlarmQueryCriteria, TimeRange, SortOrder};

pub fn create_alarm_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_alarms))
        .route("/statistics", get(get_alarm_statistics))
        .route("/recent", get(get_recent_alarms))
        .route("/{id}", get(get_alarm))
}

pub fn create_alarm_rule_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_alarm_rules))
        .route("/", post(create_alarm_rule))
        .route("/{id}", get(get_alarm_rule))
        .route("/{id}", put(update_alarm_rule))
        .route("/{id}", delete(delete_alarm_rule))
        .route("/{id}/toggle", post(toggle_alarm_rule))
}

// ============================================================================
// Alarm Query Handlers
// ============================================================================

async fn list_alarms(
    Query(params): Query<AlarmQueryParams>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<AlarmDto>>> {
    let time_range = if params.start_time.is_some() || params.end_time.is_some() {
        let start = params
            .start_time
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - chrono::Duration::days(30));

        let end = params
            .end_time
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Some(TimeRange { start, end })
    } else {
        None
    };

    let alarm_levels = params.levels.as_ref().and_then(|levels| {
        let parsed: Vec<AlarmLevel> =
            levels.iter().filter_map(|l| AlarmLevel::parse_str(l)).collect();
        if parsed.is_empty() { None } else { Some(parsed) }
    });

    let statuses = params.statuses.as_ref().and_then(|statuses| {
        let parsed: Vec<AlarmStatus> =
            statuses.iter().filter_map(|s| AlarmStatus::parse_str(s)).collect();
        if parsed.is_empty() { None } else { Some(parsed) }
    });

    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    let criteria = AlarmQueryCriteria {
        workspace_id: Some(claims.workspace_id.clone()),
        device_ids: params.device_ids,
        property_ids: None,
        alarm_levels,
        alarm_types: None,
        statuses,
        time_range,
        sort_by: Some("alarm_time".to_string()),
        sort_order: Some(SortOrder::Desc),
        limit: Some(page_size),
        offset: Some(offset),
    };

    match state.alarm_service.get_alarm_history(criteria.clone()).await {
        Ok(alarms) => {
            let total = state.alarm_service.count_alarms(criteria).await.unwrap_or(0);
            let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;

            let data: Vec<AlarmDto> = alarms.into_iter().map(AlarmDto::from).collect();

            ApiResponseBuilder::success(PaginatedResponse {
                data,
                pagination: PaginationInfo { page, page_size, total_pages, total_count: total },
            })
        }
        Err(e) => ApiResponseBuilder::error(format!("查询报警失败: {}", e)),
    }
}

async fn get_alarm(
    Path(id): Path<String>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmDto>> {
    match state.alarm_service.get_alarm_by_id(&id).await {
        Ok(Some(alarm)) => ApiResponseBuilder::success(AlarmDto::from(alarm)),
        Ok(None) => ApiResponseBuilder::error_with_code(ErrorCode::NotFound.as_i32(), "报警不存在"),
        Err(e) => ApiResponseBuilder::error(format!("获取报警失败: {}", e)),
    }
}

async fn get_alarm_statistics(
    Query(params): Query<StatisticsQueryParams>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<AlarmStatisticsDto>> {
    let start = params
        .start_time
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(7));

    let end = params
        .end_time
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let time_range = TimeRange { start, end };

    match state.alarm_service.get_alarm_statistics(time_range, &claims.workspace_id).await {
        Ok(stats) => ApiResponseBuilder::success(AlarmStatisticsDto::from(stats)),
        Err(e) => ApiResponseBuilder::error(format!("获取统计失败: {}", e)),
    }
}

// ============================================================================
// Recent Alarms Handler
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RecentAlarmsQuery {
    limit: Option<i32>,
    workspace_id: Option<String>,
}

async fn get_recent_alarms(
    State(state): State<AppState>,
    Query(query): Query<RecentAlarmsQuery>,
    claims: Claims,
) -> Json<ApiResponse<Vec<RecentAlarm>>> {
    let db = tinyiothub_storage::sqlite::Database::new(state.db_pool());
    let limit = query.limit.unwrap_or(10);

    match get_recent_alarms_list(&db, limit, Some(&claims.workspace_id)).await {
        Ok(alarms) => ApiResponseBuilder::success(alarms),
        Err(e) => {
            tracing::error!("获取最新告警列表失败: {}", e);
            ApiResponseBuilder::error("获取最新告警列表失败".to_string())
        }
    }
}

async fn get_recent_alarms_list(
    db: &tinyiothub_storage::sqlite::Database,
    limit: i32,
    workspace_id: Option<&str>,
) -> Result<Vec<RecentAlarm>, sqlx::Error> {
    let alarms: Vec<(String, String, Option<String>, String, String, chrono::NaiveDateTime, bool, bool)> =
        if let Some(wid) = workspace_id {
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

// ============================================================================
// Alarm Rule CRUD Handlers
// ============================================================================

#[derive(Deserialize)]
pub struct RuleQueryParams {
    pub device_id: Option<String>,
}

async fn list_alarm_rules(
    Query(params): Query<RuleQueryParams>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<AlarmRuleDto>>> {
    let rules = if let Some(device_id) = params.device_id {
        state.alarm_service.get_rules_by_device(&device_id, &claims.workspace_id).await
    } else {
        state.alarm_service.get_all_rules(&claims.workspace_id).await
    };

    match rules {
        Ok(rules) => {
            let dtos: Vec<AlarmRuleDto> = rules.into_iter().map(AlarmRuleDto::from).collect();
            ApiResponseBuilder::success(dtos)
        }
        Err(e) => ApiResponseBuilder::error(format!("查询规则失败: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
    use tinyiothub_storage::sqlite::Database;

    async fn create_minimal_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to create in-memory SQLite pool");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT
            )"
        )
        .execute(&pool)
        .await
        .expect("failed to create devices table");

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

        let result = get_recent_alarms_list(&db, 3, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 3);
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_ordering() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-old', 'd1', 'ws-001', 'info', 'Old alarm', datetime('now', '-2 hours'), 0, 0)")
            .execute(&pool).await.expect("insert old alarm failed");
        sqlx::query("INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved) VALUES ('alarm-new', 'd1', 'ws-001', 'info', 'New alarm', datetime('now'), 0, 0)")
            .execute(&pool).await.expect("insert new alarm failed");

        let result = get_recent_alarms_list(&db, 10, None).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 2);
        assert_eq!(alarms[0].id, "alarm-new");
        assert_eq!(alarms[1].id, "alarm-old");
    }

    #[sqlx::test]
    async fn test_get_recent_alarms_with_workspace_filter() {
        let pool = create_minimal_pool().await;
        let db = Database::new(pool.clone());

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

        let result = get_recent_alarms_list(&db, 10, Some("ws-001")).await;
        assert!(result.is_ok());
        let alarms = result.unwrap();
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].id, "alarm-ws1");
    }
}

async fn get_alarm_rule(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<AlarmRuleDto>> {
    match state.alarm_service.get_rule_by_id(&id).await {
        Ok(Some(rule)) => {
            if let Some(ref rule_ws) = rule.workspace_id {
                if rule_ws != &claims.workspace_id {
                    return ApiResponseBuilder::error_with_code(ErrorCode::NotFound.as_i32(), "规则不存在");
                }
            }
            ApiResponseBuilder::success(AlarmRuleDto::from(rule))
        }
        Ok(None) => ApiResponseBuilder::error_with_code(ErrorCode::NotFound.as_i32(), "规则不存在"),
        Err(e) => ApiResponseBuilder::error(format!("获取规则失败: {}", e)),
    }
}

async fn create_alarm_rule(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<CreateAlarmRuleRequest>,
) -> Json<ApiResponse<AlarmRuleDto>> {
    let alarm_level = match AlarmLevel::parse_str(&req.alarm_level) {
        Some(level) => level,
        None => return ApiResponseBuilder::error("无效的报警级别"),
    };

    let condition: AlarmCondition = match serde_json::from_value(req.condition) {
        Ok(c) => c,
        Err(e) => return ApiResponseBuilder::error(format!("无效的条件配置: {}", e)),
    };

    let notification_config: NotificationConfig =
        match serde_json::from_value(req.notification_config) {
            Ok(nc) => nc,
            Err(e) => return ApiResponseBuilder::error(format!("无效的通知配置: {}", e)),
        };

    let rule = match AlarmRule::new(
        req.name,
        req.description,
        req.device_id,
        req.property_id,
        req.rule_type,
        condition,
        alarm_level,
        notification_config,
        claims.workspace_id.clone(),
    ) {
        Ok(r) => r,
        Err(e) => return ApiResponseBuilder::error(format!("创建规则失败: {}", e)),
    };

    match state.alarm_service.create_rule(rule.clone()).await {
        Ok(_) => ApiResponseBuilder::success(AlarmRuleDto::from(rule)),
        Err(e) => ApiResponseBuilder::error(format!("保存规则失败: {}", e)),
    }
}

async fn update_alarm_rule(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<String>,
    Json(req): Json<UpdateAlarmRuleRequest>,
) -> Json<ApiResponse<AlarmRuleDto>> {
    let mut rule = match state.alarm_service.get_rule_by_id(&id).await {
        Ok(Some(r)) => r,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "规则不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("获取规则失败: {}", e)),
    };

    if let Some(ref rule_ws) = rule.workspace_id {
        if rule_ws != &claims.workspace_id {
            return ApiResponseBuilder::error_with_code(404, "规则不存在");
        }
    }

    let condition = req.condition.and_then(|c| serde_json::from_value(c).ok());
    let alarm_level = req.alarm_level.and_then(|l| AlarmLevel::parse_str(&l));
    let notification_config =
        req.notification_config.and_then(|nc| serde_json::from_value(nc).ok());

    if let Err(e) =
        rule.update(req.name, req.description, condition, alarm_level, notification_config)
    {
        return ApiResponseBuilder::error(format!("更新规则失败: {}", e));
    }

    match state.alarm_service.update_rule(rule.clone()).await {
        Ok(()) => ApiResponseBuilder::success(AlarmRuleDto::from(rule)),
        Err(e) => ApiResponseBuilder::error(format!("保存规则失败: {}", e)),
    }
}

async fn delete_alarm_rule(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<()>> {
    // Verify workspace ownership before delete
    if let Ok(Some(rule)) = state.alarm_service.get_rule_by_id(&id).await {
        if let Some(ref rule_ws) = rule.workspace_id {
            if rule_ws != &claims.workspace_id {
                return ApiResponseBuilder::error_with_code(404, "规则不存在");
            }
        }
    }

    match state.alarm_service.delete_rule(&id).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("删除规则失败: {}", e)),
    }
}

async fn toggle_alarm_rule(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<String>,
    Json(req): Json<ToggleRuleRequest>,
) -> Json<ApiResponse<()>> {
    // Verify workspace ownership before toggle
    if let Ok(Some(rule)) = state.alarm_service.get_rule_by_id(&id).await {
        if let Some(ref rule_ws) = rule.workspace_id {
            if rule_ws != &claims.workspace_id {
                return ApiResponseBuilder::error_with_code(404, "规则不存在");
            }
        }
    }

    match state.alarm_service.set_rule_enabled(&id, req.enabled).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("切换规则状态失败: {}", e)),
    }
}

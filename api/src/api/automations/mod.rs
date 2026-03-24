// Automations API Module
// 自动化规则管理 API

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json,
    Router,
};
use serde::Deserialize;
use sqlx::Row;

use crate::domain::automation::AutomationService;
use crate::dto::response::{builder::ApiResponseBuilder, ApiResponse};
use crate::shared::app_state::AppState;

/// 创建路由器
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/automations", get(list_automations))
        .route("/automations", post(create_automation))
        .route("/automations/{id}", get(get_automation))
        .route("/automations/{id}", put(update_automation))
        .route("/automations/{id}", delete(delete_automation))
        // 复杂业务动作，保持 RPC 风格
        .route("/automations/{id}/run", post(run_automation))
        .route("/automations/{id}/test", post(test_automation))
        .route("/automations/statistics", get(get_statistics))
}

/// 查询参数
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct QueryParams {
    pub trigger_type: Option<String>,
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 映射自动化记录
fn map_automation_row(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Value, sqlx::Error> {
    Ok(serde_json::json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "trigger_type": row.get::<String, _>("trigger_type"),
        "event_source_type": row.get::<Option<String>, _>("event_source_type"),
        "event_device_id": row.get::<Option<String>, _>("event_device_id"),
        "event_property": row.get::<Option<String>, _>("event_property"),
        "event_condition": row.get::<Option<String>, _>("event_condition"),
        "cron_expression": row.get::<Option<String>, _>("cron_expression"),
        "conditions": row.get::<Option<String>, _>("conditions"),
        "actions": row.get::<String, _>("actions"),
        "timeout_seconds": row.get::<i32, _>("timeout_seconds"),
        "retry_count": row.get::<i32, _>("retry_count"),
        "retry_delay_seconds": row.get::<i32, _>("retry_delay_seconds"),
        "cooldown_seconds": row.get::<i32, _>("cooldown_seconds"),
        "priority": row.get::<i32, _>("priority"),
        "enabled": row.get::<i32, _>("enabled") == 1,
        "run_count": row.get::<i64, _>("run_count"),
        "success_count": row.get::<i64, _>("success_count"),
        "fail_count": row.get::<i64, _>("fail_count"),
        "last_run_at": row.get::<Option<String>, _>("last_run_at"),
        "last_run_status": row.get::<Option<String>, _>("last_run_status"),
        "last_run_error": row.get::<Option<String>, _>("last_run_error"),
        "tags": row.get::<Option<String>, _>("tags"),
        "created_at": row.get::<String, _>("created_at"),
        "updated_at": row.get::<String, _>("updated_at"),
    }))
}

/// 列表
async fn list_automations(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Json<ApiResponse<Vec<serde_json::Value>>> {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    let db = state.database.clone();

    // 使用 QueryBuilder 防止 SQL 注入
    let mut query_builder = sqlx::query_builder::QueryBuilder::new("SELECT * FROM automations WHERE 1=1");

    if let Some(ref trigger_type) = params.trigger_type {
        query_builder.push(" AND trigger_type = ");
        query_builder.push_bind(trigger_type);
    }
    if let Some(enabled) = params.enabled {
        query_builder.push(" AND enabled = ");
        query_builder.push_bind(if enabled { 1 } else { 0 });
    }
    if let Some(ref name) = params.name {
        query_builder.push(" AND name LIKE ");
        query_builder.push_bind(format!("%{}%", name));
    }
    query_builder.push(" ORDER BY priority LIMIT ");
    query_builder.push_bind(page_size as i64);
    query_builder.push(" OFFSET ");
    query_builder.push_bind(offset as i64);

    // 使用 QueryBuilder 直接执行查询
    let query = query_builder.build();
    match query.fetch_all(db.pool()).await {
        Ok(rows) => {
            let results: Result<Vec<_>, _> = rows.iter().map(map_automation_row).collect();
            match results {
                Ok(data) => ApiResponseBuilder::success(data),
                Err(e) => {
                    tracing::error!("Failed to map automation rows: {}", e);
                    ApiResponseBuilder::error("获取自动化列表失败")
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to list automations: {}", e);
            ApiResponseBuilder::error("获取自动化列表失败")
        }
    }
}

/// 详情
async fn get_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db = state.database.clone();

    // 使用参数化查询防止 SQL 注入
    match sqlx::query("SELECT * FROM automations WHERE id = ?")
        .bind(&id)
        .fetch_optional(db.pool())
        .await
    {
        Ok(Some(row)) => match map_automation_row(&row) {
            Ok(result) => ApiResponseBuilder::success(result),
            Err(e) => {
                tracing::error!("Failed to map automation row: {}", e);
                ApiResponseBuilder::error("获取自动化失败")
            }
        },
        Ok(None) => ApiResponseBuilder::error("自动化不存在"),
        Err(e) => {
            tracing::error!("Failed to get automation: {}", e);
            ApiResponseBuilder::error("获取自动化失败")
        }
    }
}

/// 创建请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateAutomationRequest {
    pub name: String,
    pub description: Option<String>,
    pub trigger_type: Option<String>,
    pub event_source_type: Option<String>,
    pub event_device_id: Option<String>,
    pub event_property: Option<String>,
    pub event_condition: Option<String>,
    pub cron_expression: Option<String>,
    pub conditions: Option<String>,
    pub actions: String,
    pub timeout_seconds: Option<i32>,
    pub retry_count: Option<i32>,
    pub retry_delay_seconds: Option<i32>,
    pub cooldown_seconds: Option<i32>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
    pub tags: Option<String>,
}

/// 创建
async fn create_automation(
    State(state): State<AppState>,
    Json(payload): Json<CreateAutomationRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let trigger_type = payload.trigger_type.unwrap_or_else(|| "event".to_string());

    if let Some(ref cron) = payload.cron_expression {
        if cron::Schedule::from_str(cron).is_err() {
            return ApiResponseBuilder::error("无效的 Cron 表达式");
        }
    }

    // 使用参数化查询防止 SQL 注入
    let result = sqlx::query(
        r#"INSERT INTO automations (
            id, name, description, trigger_type, event_source_type, event_device_id,
            event_property, event_condition, cron_expression, conditions, actions,
            timeout_seconds, retry_count, retry_delay_seconds, cooldown_seconds,
            priority, enabled, tags, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&payload.name)
    .bind(payload.description.as_deref())
    .bind(&trigger_type)
    .bind(payload.event_source_type.as_deref())
    .bind(payload.event_device_id.as_deref())
    .bind(payload.event_property.as_deref())
    .bind(payload.event_condition.as_deref())
    .bind(payload.cron_expression.as_deref())
    .bind(payload.conditions.as_deref())
    .bind(&payload.actions)
    .bind(payload.timeout_seconds.unwrap_or(30))
    .bind(payload.retry_count.unwrap_or(0))
    .bind(payload.retry_delay_seconds.unwrap_or(5))
    .bind(payload.cooldown_seconds.unwrap_or(0))
    .bind(payload.priority.unwrap_or(100))
    .bind(payload.enabled.unwrap_or(true) as i32)
    .bind(payload.tags.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(state.database.pool())
    .await;

    match result {
        Ok(_) => get_automation(State(state), Path(id)).await,
        Err(e) => {
            tracing::error!("Failed to create automation: {}", e);
            ApiResponseBuilder::error("创建自动化失败")
        }
    }
}

/// 更新请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateAutomationRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub trigger_type: Option<String>,
    pub cron_expression: Option<String>,
    pub conditions: Option<String>,
    pub actions: Option<String>,
    pub timeout_seconds: Option<i32>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
    pub tags: Option<String>,
}

/// 更新
async fn update_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateAutomationRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let now = chrono::Utc::now().to_rfc3339();

    // 检查是否有任何字段需要更新
    let has_updates = payload.name.is_some()
        || payload.description.is_some()
        || payload.trigger_type.is_some()
        || payload.cron_expression.is_some()
        || payload.conditions.is_some()
        || payload.actions.is_some()
        || payload.timeout_seconds.is_some()
        || payload.priority.is_some()
        || payload.enabled.is_some()
        || payload.tags.is_some();

    if !has_updates {
        return get_automation(State(state), Path(id)).await;
    }

    // 使用 QueryBuilder 防止 SQL 注入
    let mut query_builder = sqlx::query_builder::QueryBuilder::new("UPDATE automations SET updated_at = ");
    query_builder.push_bind(&now);

    if let Some(ref name) = payload.name {
        query_builder.push(", name = ");
        query_builder.push_bind(name);
    }
    if let Some(ref description) = payload.description {
        query_builder.push(", description = ");
        query_builder.push_bind(description);
    }
    if let Some(ref trigger_type) = payload.trigger_type {
        query_builder.push(", trigger_type = ");
        query_builder.push_bind(trigger_type);
    }
    if let Some(ref cron_expression) = payload.cron_expression {
        query_builder.push(", cron_expression = ");
        query_builder.push_bind(cron_expression);
    }
    if let Some(ref conditions) = payload.conditions {
        query_builder.push(", conditions = ");
        query_builder.push_bind(conditions);
    }
    if let Some(ref actions) = payload.actions {
        query_builder.push(", actions = ");
        query_builder.push_bind(actions);
    }
    if let Some(timeout_seconds) = payload.timeout_seconds {
        query_builder.push(", timeout_seconds = ");
        query_builder.push_bind(timeout_seconds);
    }
    if let Some(priority) = payload.priority {
        query_builder.push(", priority = ");
        query_builder.push_bind(priority);
    }
    if let Some(enabled) = payload.enabled {
        query_builder.push(", enabled = ");
        query_builder.push_bind(enabled as i32);
    }
    if let Some(ref tags) = payload.tags {
        query_builder.push(", tags = ");
        query_builder.push_bind(tags);
    }

    query_builder.push(" WHERE id = ");
    query_builder.push_bind(&id);

    match query_builder.build().execute(state.database.pool()).await {
        Ok(_) => get_automation(State(state), Path(id)).await,
        Err(e) => {
            tracing::error!("Failed to update automation: {}", e);
            ApiResponseBuilder::error("更新自动化失败")
        }
    }
}

/// 删除
async fn delete_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<bool>> {
    let db = state.database.clone();

    // 使用参数化查询防止 SQL 注入
    match sqlx::query("DELETE FROM automations WHERE id = ?")
        .bind(&id)
        .execute(db.pool())
        .await
    {
        Ok(result) if result.rows_affected() > 0 => ApiResponseBuilder::success(true),
        Ok(_) => ApiResponseBuilder::error("自动化不存在"),
        Err(e) => {
            tracing::error!("Failed to delete automation: {}", e);
            ApiResponseBuilder::error("删除自动化失败")
        }
    }
}

/// 执行
async fn run_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let now = chrono::Utc::now().to_rfc3339();

    // 使用参数化查询防止 SQL 注入
    match sqlx::query(
        "UPDATE automations SET run_count = run_count + 1, last_run_at = ?, last_run_status = 'success' WHERE id = ?"
    )
    .bind(&now)
    .bind(&id)
    .execute(state.database.pool())
    .await
    {
        Ok(_) => ApiResponseBuilder::success(serde_json::json!({
            "message": "自动化已执行",
            "executed_at": now
        })),
        Err(e) => {
            tracing::error!("Failed to run automation: {}", e);
            ApiResponseBuilder::error("执行自动化失败")
        }
    }
}

/// 测试
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TestAutomationRequest {
    pub conditions: String,
    pub mock_data: serde_json::Value,
}

/// 测试条件
async fn test_automation(
    State(_state): State<AppState>,
    Json(payload): Json<TestAutomationRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let service = AutomationService::new();
    let (result, details) = service.test_condition(&payload.conditions, payload.mock_data);
    
    ApiResponseBuilder::success(serde_json::json!({
        "matched": result,
        "details": details
    }))
}

/// 统计
async fn get_statistics(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db = state.database.clone();

    // 获取总数
    let total: i64 = db
        .query_first("SELECT COUNT(*) FROM automations", |row| row.try_get::<i64, _>(0))
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    // 获取启用数
    let enabled: i64 = db
        .query_first("SELECT COUNT(*) FROM automations WHERE enabled = 1", |row| row.try_get::<i64, _>(0))
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    ApiResponseBuilder::success(serde_json::json!({
        "total": total,
        "enabled": enabled,
        "disabled": total - enabled
    }))
}

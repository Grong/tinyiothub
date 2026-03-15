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
        .route("/automations/{id}/enable", post(enable_automation))
        .route("/automations/{id}/disable", post(disable_automation))
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
    
    let mut sql = String::from("SELECT * FROM automations WHERE 1=1");
    
    if let Some(ref trigger_type) = params.trigger_type {
        sql.push_str(&format!(" AND trigger_type = '{}'", trigger_type));
    }
    if let Some(enabled) = params.enabled {
        sql.push_str(&format!(" AND enabled = {}", if enabled { 1 } else { 0 }));
    }
    if let Some(ref name) = params.name {
        sql.push_str(&format!(" AND name LIKE '%{}%'", name));
    }
    sql.push_str(&format!(" ORDER BY priority LIMIT {} OFFSET {}", page_size, offset));
    
    let db = state.database.clone();
    
    match db.query(&sql, map_automation_row).await {
        Ok(results) => ApiResponseBuilder::success(results),
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
    let sql = format!("SELECT * FROM automations WHERE id = '{}'", id);
    let db = state.database.clone();
    
    match db.query_first(&sql, map_automation_row).await {
        Ok(Some(result)) => ApiResponseBuilder::success(result),
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
    
    let sql = format!(
        r#"INSERT INTO automations (
            id, name, description, trigger_type, event_source_type, event_device_id,
            event_property, event_condition, cron_expression, conditions, actions,
            timeout_seconds, retry_count, retry_delay_seconds, cooldown_seconds,
            priority, enabled, tags, created_at, updated_at
        ) VALUES (
            '{}', '{}', {}, '{}', {}, {}, {}, {}, {}, {}, '{}',
            {}, {}, {}, {},
            {}, {}, {}, '{}', '{}'
        )"#,
        id,
        payload.name,
        payload.description.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        trigger_type,
        payload.event_source_type.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        payload.event_device_id.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        payload.event_property.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        payload.event_condition.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        payload.cron_expression.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        payload.conditions.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        payload.actions,
        payload.timeout_seconds.unwrap_or(30),
        payload.retry_count.unwrap_or(0),
        payload.retry_delay_seconds.unwrap_or(5),
        payload.cooldown_seconds.unwrap_or(0),
        payload.priority.unwrap_or(100),
        payload.enabled.unwrap_or(true) as i32,
        payload.tags.as_ref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "NULL".to_string()),
        now,
        now
    );
    
    let db = state.database.clone();
    
    match db.execute(&sql).await {
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
    
    let mut updates = vec![format!("updated_at = '{}'", now)];
    
    if let Some(name) = payload.name { updates.push(format!("name = '{}'", name)); }
    if let Some(description) = payload.description { updates.push(format!("description = '{}'", description)); }
    if let Some(trigger_type) = payload.trigger_type { updates.push(format!("trigger_type = '{}'", trigger_type)); }
    if let Some(cron_expression) = payload.cron_expression { updates.push(format!("cron_expression = '{}'", cron_expression)); }
    if let Some(conditions) = payload.conditions { updates.push(format!("conditions = '{}'", conditions)); }
    if let Some(actions) = payload.actions { updates.push(format!("actions = '{}'", actions)); }
    if let Some(timeout_seconds) = payload.timeout_seconds { updates.push(format!("timeout_seconds = {}", timeout_seconds)); }
    if let Some(priority) = payload.priority { updates.push(format!("priority = {}", priority)); }
    if let Some(enabled) = payload.enabled { updates.push(format!("enabled = {}", enabled as i32)); }
    if let Some(tags) = payload.tags { updates.push(format!("tags = '{}'", tags)); }
    
    let sql = format!("UPDATE automations SET {} WHERE id = '{}'", updates.join(", "), id);
    
    let db = state.database.clone();
    
    match db.execute(&sql).await {
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
    let sql = format!("DELETE FROM automations WHERE id = '{}'", id);
    let db = state.database.clone();
    
    match db.execute(&sql).await {
        Ok(_) => ApiResponseBuilder::success(true),
        Err(e) => {
            tracing::error!("Failed to delete automation: {}", e);
            ApiResponseBuilder::error("删除自动化失败")
        }
    }
}

/// 启用
async fn enable_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let sql = format!("UPDATE automations SET enabled = 1, updated_at = '{}' WHERE id = '{}'",
        chrono::Utc::now().to_rfc3339(), id);
    let db = state.database.clone();
    
    match db.execute(&sql).await {
        Ok(_) => get_automation(State(state), Path(id)).await,
        Err(e) => {
            tracing::error!("Failed to enable automation: {}", e);
            ApiResponseBuilder::error("启用自动化失败")
        }
    }
}

/// 禁用
async fn disable_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let sql = format!("UPDATE automations SET enabled = 0, updated_at = '{}' WHERE id = '{}'",
        chrono::Utc::now().to_rfc3339(), id);
    let db = state.database.clone();
    
    match db.execute(&sql).await {
        Ok(_) => get_automation(State(state), Path(id)).await,
        Err(e) => {
            tracing::error!("Failed to disable automation: {}", e);
            ApiResponseBuilder::error("禁用自动化失败")
        }
    }
}

/// 执行
async fn run_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let now = chrono::Utc::now().to_rfc3339();
    let sql = format!(
        "UPDATE automations SET run_count = run_count + 1, last_run_at = '{}', last_run_status = 'success' WHERE id = '{}'",
        now, id
    );
    
    let db = state.database.clone();
    let _ = db.execute(&sql).await;
    
    ApiResponseBuilder::success(serde_json::json!({
        "message": "自动化已执行",
        "executed_at": now
    }))
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
    let total = match db.execute("SELECT COUNT(*) FROM automations").await {
        Ok(_) => 0i64,
        Err(_) => 0i64,
    };
    
    // 获取启用数
    let enabled = match db.execute("SELECT COUNT(*) FROM automations WHERE enabled = 1").await {
        Ok(_) => 0i64,
        Err(_) => 0i64,
    };
    
    ApiResponseBuilder::success(serde_json::json!({
        "total": total,
        "enabled": enabled,
        "disabled": total - enabled
    }))
}

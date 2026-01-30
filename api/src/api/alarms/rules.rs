use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::AppState,
    dto::{entity::device_alarm_rule::DeviceAlarmRule, request::pagination::PaginationQuery, response::ApiResponse},
    shared::security::jwt::Claims,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AlarmRuleQuery {
    pub device_id: Option<String>,
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateAlarmRuleRequest {
    pub device_id: String,
    pub rule_name: String,
    pub condition: String,
    pub alarm_level: String,
    pub message_template: String,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateAlarmRuleRequest {
    pub rule_name: Option<String>,
    pub condition: Option<String>,
    pub alarm_level: Option<String>,
    pub message_template: Option<String>,
    pub enabled: Option<bool>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_alarm_rules).post(create_alarm_rule))
        .route("/:id", get(get_alarm_rule).put(update_alarm_rule).delete(delete_alarm_rule))
        .route("/:id/enable", post(enable_alarm_rule))
        .route("/:id/disable", post(disable_alarm_rule))
}

/// 获取告警规则列表
async fn list_alarm_rules(
    State(_state): State<AppState>,
    Query(query): Query<AlarmRuleQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<DeviceAlarmRule>>> {
    // TODO: 实现告警规则查询逻辑
    tracing::info!("Listing alarm rules with filters");
    
    let rules = vec![];
    ApiResponse::success(rules)
}

/// 创建告警规则
async fn create_alarm_rule(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateAlarmRuleRequest>,
) -> Json<ApiResponse<DeviceAlarmRule>> {
    // TODO: 实现告警规则创建逻辑
    tracing::info!("Creating alarm rule: {}", req.rule_name);
    
    // 临时返回一个示例规则 - 需要根据实际的DeviceAlarmRule结构调整
    // 由于字段不匹配，先返回错误
    ApiResponse::error("告警规则创建功能尚未实现".to_string())
}

/// 获取告警规则详情
async fn get_alarm_rule(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DeviceAlarmRule>>> {
    // TODO: 实现告警规则详情查询逻辑
    tracing::info!("Getting alarm rule details for: {}", id);
    
    ApiResponse::success(None)
}

/// 更新告警规则
async fn update_alarm_rule(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(req): Json<UpdateAlarmRuleRequest>,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现告警规则更新逻辑
    tracing::info!("Updating alarm rule: {}", id);
    
    ApiResponse::success(true)
}

/// 删除告警规则
async fn delete_alarm_rule(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现告警规则删除逻辑
    tracing::info!("Deleting alarm rule: {}", id);
    
    ApiResponse::success(true)
}

/// 启用告警规则
async fn enable_alarm_rule(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现告警规则启用逻辑
    tracing::info!("Enabling alarm rule: {}", id);
    
    ApiResponse::success(true)
}

/// 禁用告警规则
async fn disable_alarm_rule(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现告警规则禁用逻辑
    tracing::info!("Disabling alarm rule: {}", id);
    
    ApiResponse::success(true)
}
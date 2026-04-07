use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        entity::device_alarm_rule::{DeviceAlarmRule, DeviceAlarmRuleQuery},
        response::{ApiResponse, builder::ApiResponseBuilder, PaginatedResponse, PaginationInfo},
    },
    shared::security::jwt::Claims,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AlarmRuleQuery {
    pub device_id: Option<String>,
    pub rule_type: Option<String>,
    pub alarm_level: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
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
    State(state): State<AppState>,
    Query(query): Query<AlarmRuleQuery>,
    _claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<DeviceAlarmRule>>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let (rules_result, count_result) = tokio::join!(
        DeviceAlarmRule::find_all(state.database(), &query),
        DeviceAlarmRule::count(state.database(), &query),
    );

    match rules_result {
        Ok(rules) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            ApiResponseBuilder::success(PaginatedResponse {
                data: rules,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            })
        }
        Err(e) => {
            tracing::error!("Failed to fetch alarm rules: {}", e);
            ApiResponseBuilder::error("获取告警规则列表失败")
        }
    }
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
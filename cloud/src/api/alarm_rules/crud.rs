use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use crate::dto::entity::alarm::AlarmRuleDto;
use axum::{
    extract::{Path, Query, State},
    Json
};

use crate::{
    domain::alarm::{AlarmCondition, AlarmLevel, AlarmRule, NotificationConfig},
    dto::{
        request::{CreateAlarmRuleRequest, ToggleRuleRequest, UpdateAlarmRuleRequest},
        response::ApiResponse
    },
    shared::{app_state::AppState, error_handling::ErrorCode}
};

#[derive(serde::Deserialize)]
pub struct RuleQueryParams {
    pub device_id: Option<String>,
}

/// 查询报警规则列表
pub async fn list_alarm_rules(
    Query(params): Query<RuleQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<AlarmRuleDto>>> {
    let rules = if let Some(device_id) = params.device_id {
        state.alarm_service.get_rules_by_device(&device_id).await
    } else {
        state.alarm_service.get_all_rules().await
    };

    match rules {
        Ok(rules) => {
            let dtos: Vec<AlarmRuleDto> = rules.into_iter().map(AlarmRuleDto::from).collect();
            ApiResponseBuilder::success(dtos)
        }
        Err(e) => ApiResponseBuilder::error(format!("查询规则失败: {}", e)),
    }
}

/// 获取报警规则详情
pub async fn get_alarm_rule(
    Path(id): Path<String>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmRuleDto>> {
    match state.alarm_service.get_rule_by_id(&id).await {
        Ok(Some(rule)) => ApiResponseBuilder::success(AlarmRuleDto::from(rule)),
        Ok(None) => ApiResponseBuilder::error_with_code(ErrorCode::NotFound.as_i32(), "规则不存在"),
        Err(e) => ApiResponseBuilder::error(format!("获取规则失败: {}", e)),
    }
}

/// 创建报警规则
pub async fn create_alarm_rule(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateAlarmRuleRequest>,
) -> Json<ApiResponse<AlarmRuleDto>> {
    // 解析报警级别
    let alarm_level = match AlarmLevel::parse_str(&req.alarm_level) {
        Some(level) => level,
        None => return ApiResponseBuilder::error("无效的报警级别")
};

    // 解析条件
    let condition: AlarmCondition = match serde_json::from_value(req.condition) {
        Ok(c) => c,
        Err(e) => return ApiResponseBuilder::error(format!("无效的条件配置: {}", e))
};

    // 解析通知配置
    let notification_config: NotificationConfig =
        match serde_json::from_value(req.notification_config) {
            Ok(nc) => nc,
            Err(e) => return ApiResponseBuilder::error(format!("无效的通知配置: {}", e))
};

    // 创建规则
    let rule = match AlarmRule::new(
        req.name,
        req.description,
        req.device_id,
        req.property_id,
        req.rule_type,
        condition,
        alarm_level,
        notification_config,
    ) {
        Ok(r) => r,
        Err(e) => return ApiResponseBuilder::error(format!("创建规则失败: {}", e))
};

    match state.alarm_service.create_rule(rule.clone()).await {
        Ok(_) => ApiResponseBuilder::success(AlarmRuleDto::from(rule)),
        Err(e) => ApiResponseBuilder::error(format!("保存规则失败: {}", e)),
    }
}

/// 更新报警规则
pub async fn update_alarm_rule(
    State(state): State<AppState>,
    _claims: Claims,
    Path(id): Path<String>,
    Json(req): Json<UpdateAlarmRuleRequest>,
) -> Json<ApiResponse<AlarmRuleDto>> {
    // 获取现有规则
    let mut rule = match state.alarm_service.get_rule_by_id(&id).await {
        Ok(Some(r)) => r,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "规则不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("获取规则失败: {}", e))
};

    // 解析更新字段
    let condition = req.condition.and_then(|c| serde_json::from_value(c).ok());
    let alarm_level = req.alarm_level.and_then(|l| AlarmLevel::parse_str(&l));
    let notification_config =
        req.notification_config.and_then(|nc| serde_json::from_value(nc).ok());

    // 更新规则
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

/// 删除报警规则
pub async fn delete_alarm_rule(
    Path(id): Path<String>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<()>> {
    match state.alarm_service.delete_rule(&id).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("删除规则失败: {}", e)),
    }
}

/// 启用/禁用报警规则
pub async fn toggle_alarm_rule(
    State(state): State<AppState>,
    _claims: Claims,
    Path(id): Path<String>,
    Json(req): Json<ToggleRuleRequest>,
) -> Json<ApiResponse<()>> {
    match state.alarm_service.set_rule_enabled(&id, req.enabled).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("切换规则状态失败: {}", e)),
    }
}

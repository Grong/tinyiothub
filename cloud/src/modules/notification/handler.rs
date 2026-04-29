// Notification HTTP handlers
// Consolidated from api/notifications/management.rs and api/notification_channels/mod.rs

use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use tracing::{error, info};
use uuid::Uuid;

use crate::shared::security::jwt::Claims;
use crate::{
    shared::api_response::{ApiResponse, PaginatedResponse, PaginationInfo},
    shared::app_state::AppState,
};

use super::types::{
    convert_device_filter, device_filter_to_json, CreateNotificationRuleRequest,
    DeviceFilterResponse, DeviceFilterRequest, NotificationChannelType,
    NotificationHistoryQuery, NotificationHistoryResponse, NotificationLevel,
    NotificationRule, NotificationRuleQuery, NotificationRuleResponse,
    TestNotificationRequest, UpdateNotificationRuleRequest,
};
use super::service::NotificationMessage;
use crate::modules::notification::service::send_notification_message;
use crate::shared::persistence::repositories::{
    find_notification_channel_by_id, find_all_notification_channels,
    count_notification_channels, create_notification_channel,
    update_notification_channel, delete_notification_channel,
    get_notification_channel_statistics,
};
use tinyiothub_core::models::notification_channel::{
    ChannelStatistics, CreateNotificationChannelRequest, NotificationChannel,
    NotificationChannelQueryParams, SendMessageRequest, UpdateNotificationChannelRequest,
};

// ──────────────────────────────────────────────
// Notification Rules Router
// ──────────────────────────────────────────────

/// Create notification rules router
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/rules", get(get_notification_rules).post(create_notification_rule))
        .route(
            "/rules/{rule_id}",
            get(get_notification_rule)
                .put(update_notification_rule)
                .delete(delete_notification_rule),
        )
        .route("/history", get(get_notification_history))
        .route("/test", post(send_test_notification))
}

// ──────────────────────────────────────────────
// Notification Channels Router
// ──────────────────────────────────────────────

/// Create notification channels router
pub fn create_channel_router() -> Router<AppState> {
    Router::new()
        .route("/notification-channels", get(list_channels))
        .route("/notification-channels", post(create_channel))
        .route("/notification-channels/{id}", get(get_channel))
        .route("/notification-channels/{id}", put(update_channel))
        .route("/notification-channels/{id}", delete(delete_channel))
        .route("/notification-channels/{id}/test", post(test_channel))
        .route("/notification-channels/statistics", get(get_statistics))
}

// ──────────────────────────────────────────────
// Notification Rules Handlers
// ──────────────────────────────────────────────

/// Get all notification rules
#[axum::debug_handler]
pub async fn get_notification_rules(
    Query(query): Query<NotificationRuleQuery>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<NotificationRuleResponse>>> {
    match get_notification_rules_impl(&state, query, &claims.workspace_id).await {
        Ok(rules) => ApiResponseBuilder::success(rules),
        Err(e) => {
            error!("Failed to get notification rules: {}", e);
            ApiResponseBuilder::error("Failed to retrieve notification rules")
        }
    }
}

async fn get_notification_rules_impl(
    state: &AppState,
    query: NotificationRuleQuery,
    workspace_id: &str,
) -> Result<Vec<NotificationRuleResponse>, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let rules = notification_manager
        .get_rules()
        .await
        .map_err(|e| format!("Failed to get notification rules: {}", e))?;

    let filtered_rules: Vec<_> = rules
        .into_iter()
        .filter(|rule| {
            if let Some(ref rule_ws) = rule.workspace_id {
                if rule_ws != workspace_id {
                    return false;
                }
            }
            if let Some(enabled) = query.enabled
                && rule.enabled != enabled {
                    return false;
                }
            if let Some(ref event_type) = query.event_type
                && rule.event_type.as_ref() != Some(event_type) {
                    return false;
                }
            if let Some(ref method) = query.notification_method
                && let Some(channel) = NotificationChannelType::parse_str(method)
                    && !rule.notification_methods.contains(&channel) {
                        return false;
                    }
            true
        })
        .map(|rule| NotificationRuleResponse {
            id: rule.id,
            name: rule.name,
            description: rule.description,
            event_type: rule.event_type,
            event_subtype: rule.event_subtype,
            event_level: rule.event_level,
            device_filter: rule.device_filter.as_ref().map(convert_device_filter),
            notification_methods: rule
                .notification_methods
                .iter()
                .map(|m| m.as_str().to_string())
                .collect(),
            recipients: rule.recipients,
            enabled: rule.enabled,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        })
        .collect();

    Ok(filtered_rules)
}

/// Create a new notification rule
#[axum::debug_handler]
pub async fn create_notification_rule(
    State(state): State<AppState>,
    claims: Claims,
    Json(request): Json<CreateNotificationRuleRequest>,
) -> Json<ApiResponse<NotificationRuleResponse>> {
    match create_notification_rule_impl(&state, request, &claims.workspace_id).await {
        Ok(rule) => ApiResponseBuilder::success(rule),
        Err(e) => {
            error!("Failed to create notification rule: {}", e);
            ApiResponseBuilder::error("Failed to create notification rule")
        }
    }
}

async fn create_notification_rule_impl(
    state: &AppState,
    request: CreateNotificationRuleRequest,
    workspace_id: &str,
) -> Result<NotificationRuleResponse, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let notification_methods: Result<Vec<NotificationChannelType>, _> = request
        .notification_methods
        .iter()
        .map(|method| {
            NotificationChannelType::parse_str(method)
                .ok_or_else(|| format!("Invalid notification method: {}", method))
        })
        .collect();
    let notification_methods = notification_methods?;

    let device_filter = request.device_filter.map(|filter| device_filter_to_json(&filter));

    let mut rule = NotificationRule::new(
        Uuid::new_v4().to_string(),
        request.name,
        request.description,
        notification_methods.clone(),
        request.recipients.clone(),
    );
    rule.workspace_id = Some(workspace_id.to_string());

    if let Some(event_type) = request.event_type {
        rule = rule.with_event_type(event_type);
    }
    if let Some(event_subtype) = request.event_subtype {
        rule = rule.with_event_subtype(event_subtype);
    }
    if let Some(event_level) = request.event_level {
        rule = rule.with_event_level(event_level);
    }
    if let Some(device_filter) = device_filter {
        rule = rule.with_device_filter(device_filter);
    }
    if let Some(enabled) = request.enabled {
        rule = rule.set_enabled(enabled);
    }

    notification_manager
        .add_rule(rule.clone())
        .await
        .map_err(|e| format!("Failed to add rule: {}", e))?;

    info!("Created notification rule: {} ({})", rule.name, rule.id);

    Ok(NotificationRuleResponse {
        id: rule.id,
        name: rule.name,
        description: rule.description,
        event_type: rule.event_type,
        event_subtype: rule.event_subtype,
        event_level: rule.event_level,
        device_filter: rule.device_filter.as_ref().map(convert_device_filter),
        notification_methods: rule
            .notification_methods
            .iter()
            .map(|m| m.as_str().to_string())
            .collect(),
        recipients: rule.recipients,
        enabled: rule.enabled,
        created_at: rule.created_at,
        updated_at: rule.updated_at,
    })
}

/// Get a specific notification rule
#[axum::debug_handler]
pub async fn get_notification_rule(
    Path(rule_id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<NotificationRuleResponse>> {
    match get_notification_rule_impl(&state, &rule_id, &claims.workspace_id).await {
        Ok(Some(rule)) => ApiResponseBuilder::success(rule),
        Ok(None) => ApiResponseBuilder::error("Notification rule not found"),
        Err(e) => {
            error!("Failed to get notification rule {}: {}", rule_id, e);
            ApiResponseBuilder::error("Failed to retrieve notification rule")
        }
    }
}

async fn get_notification_rule_impl(
    state: &AppState,
    rule_id: &str,
    workspace_id: &str,
) -> Result<Option<NotificationRuleResponse>, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let rules = notification_manager
        .get_rules()
        .await
        .map_err(|e| format!("Failed to get notification rules: {}", e))?;

    let rule = rules.into_iter().find(|r| r.id == rule_id);

    if let Some(rule) = rule {
        // Verify workspace ownership
        if let Some(ref rule_ws) = rule.workspace_id {
            if rule_ws != workspace_id {
                return Ok(None);
            }
        }
        Ok(Some(NotificationRuleResponse {
            id: rule.id,
            name: rule.name,
            description: rule.description,
            event_type: rule.event_type,
            event_subtype: rule.event_subtype,
            event_level: rule.event_level,
            device_filter: rule.device_filter.as_ref().map(convert_device_filter),
            notification_methods: rule
                .notification_methods
                .iter()
                .map(|m| m.as_str().to_string())
                .collect(),
            recipients: rule.recipients,
            enabled: rule.enabled,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        }))
    } else {
        Ok(None)
    }
}

/// Update a notification rule
#[axum::debug_handler]
pub async fn update_notification_rule(
    State(state): State<AppState>,
    claims: Claims,
    Path(rule_id): Path<String>,
    Json(request): Json<UpdateNotificationRuleRequest>,
) -> Json<ApiResponse<NotificationRuleResponse>> {
    match update_notification_rule_impl(&state, &rule_id, request, &claims.workspace_id).await {
        Ok(rule) => ApiResponseBuilder::success(rule),
        Err(e) => {
            error!("Failed to update notification rule {}: {}", rule_id, e);
            ApiResponseBuilder::error("Failed to update notification rule")
        }
    }
}

async fn update_notification_rule_impl(
    state: &AppState,
    rule_id: &str,
    request: UpdateNotificationRuleRequest,
    workspace_id: &str,
) -> Result<NotificationRuleResponse, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let rules = notification_manager
        .get_rules()
        .await
        .map_err(|e| format!("Failed to get notification rules: {}", e))?;

    let mut rule =
        rules.into_iter().find(|r| r.id == rule_id).ok_or("Notification rule not found")?;

    // Verify workspace ownership
    if let Some(ref rule_ws) = rule.workspace_id {
        if rule_ws != workspace_id {
            return Err("Notification rule not found".to_string());
        }
    }

    if let Some(name) = request.name {
        rule.name = name;
    }
    if let Some(description) = request.description {
        rule.description = Some(description);
    }
    if let Some(event_type) = request.event_type {
        rule.event_type = Some(event_type);
    }
    if let Some(event_subtype) = request.event_subtype {
        rule.event_subtype = Some(event_subtype);
    }
    if let Some(event_level) = request.event_level {
        rule.event_level = Some(event_level);
    }
    if let Some(device_filter_req) = request.device_filter {
        rule.device_filter = Some(device_filter_to_json(&device_filter_req));
    }
    if let Some(notification_methods) = request.notification_methods {
        let methods: Result<Vec<NotificationChannelType>, _> = notification_methods
            .iter()
            .map(|method| {
                NotificationChannelType::parse_str(method)
                    .ok_or_else(|| format!("Invalid notification method: {}", method))
            })
            .collect();
        rule.notification_methods = methods?;
    }
    if let Some(recipients) = request.recipients {
        rule.recipients = recipients;
    }
    if let Some(enabled) = request.enabled {
        rule.enabled = enabled;
    }

    rule.updated_at = Utc::now();

    notification_manager
        .update_rule(rule_id, rule.clone())
        .await
        .map_err(|e| format!("Failed to update rule: {}", e))?;

    info!("Updated notification rule: {} ({})", rule.name, rule.id);

    Ok(NotificationRuleResponse {
        id: rule.id,
        name: rule.name,
        description: rule.description,
        event_type: rule.event_type,
        event_subtype: rule.event_subtype,
        event_level: rule.event_level,
        device_filter: rule.device_filter.as_ref().map(convert_device_filter),
        notification_methods: rule
            .notification_methods
            .iter()
            .map(|m| m.as_str().to_string())
            .collect(),
        recipients: rule.recipients,
        enabled: rule.enabled,
        created_at: rule.created_at,
        updated_at: rule.updated_at,
    })
}

/// Delete a notification rule
#[axum::debug_handler]
pub async fn delete_notification_rule(
    Path(rule_id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    match delete_notification_rule_impl(&state, &rule_id, &claims.workspace_id).await {
        Ok(()) => {
            info!("Deleted notification rule: {}", rule_id);
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            error!("Failed to delete notification rule {}: {}", rule_id, e);
            ApiResponseBuilder::error("Failed to delete notification rule")
        }
    }
}

async fn delete_notification_rule_impl(state: &AppState, rule_id: &str, workspace_id: &str) -> Result<(), String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    // Verify workspace ownership before delete
    let rules = notification_manager
        .get_rules()
        .await
        .map_err(|e| format!("Failed to get notification rules: {}", e))?;
    if let Some(rule) = rules.into_iter().find(|r| r.id == rule_id) {
        if let Some(ref rule_ws) = rule.workspace_id {
            if rule_ws != workspace_id {
                return Err("Notification rule not found".to_string());
            }
        }
    }

    notification_manager
        .remove_rule(rule_id)
        .await
        .map_err(|e| format!("Failed to remove rule: {}", e))?;
    Ok(())
}

/// Get notification history
#[axum::debug_handler]
pub async fn get_notification_history(
    Query(query): Query<NotificationHistoryQuery>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<NotificationHistoryResponse>>> {
    match get_notification_history_impl(&state, query, &claims.workspace_id).await {
        Ok(history) => ApiResponseBuilder::success(history),
        Err(e) => {
            error!("Failed to get notification history: {}", e);
            ApiResponseBuilder::error("Failed to retrieve notification history")
        }
    }
}

async fn get_notification_history_impl(
    state: &AppState,
    query: NotificationHistoryQuery,
    _workspace_id: &str,
) -> Result<Vec<NotificationHistoryResponse>, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let history = if let Some(event_id) = query.event_id {
        notification_manager
            .get_notification_history(&event_id)
            .await
            .map_err(|e| format!("Failed to get notification history: {}", e))?
    } else {
        Vec::new()
    };

    let response: Vec<NotificationHistoryResponse> = history
        .into_iter()
        .map(|record| NotificationHistoryResponse {
            id: record.id,
            event_id: record.event_id,
            rule_id: record.rule_id,
            notification_method: record.notification_method.as_str().to_string(),
            recipient: record.recipient,
            status: record.status.as_str().to_string(),
            sent_at: record.sent_at,
            error_message: record.error_message,
            created_at: record.created_at,
        })
        .collect();

    Ok(response)
}

/// Send a test notification
#[axum::debug_handler]
pub async fn send_test_notification(
    State(state): State<AppState>,
    claims: Claims,
    Json(request): Json<TestNotificationRequest>,
) -> Json<ApiResponse<bool>> {
    match send_test_notification_impl(&state, request, &claims.workspace_id).await {
        Ok(()) => {
            info!("Test notification sent successfully");
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            error!("Failed to send test notification: {}", e);
            ApiResponseBuilder::error("Failed to send test notification")
        }
    }
}

async fn send_test_notification_impl(
    state: &AppState,
    request: TestNotificationRequest,
    _workspace_id: &str,
) -> Result<(), String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let level = NotificationLevel::parse_str(&request.level)
        .map_err(|_e| format!("Invalid notification level: {}", request.level))?;

    let channels: Result<Vec<NotificationChannelType>, _> = request
        .channels
        .iter()
        .map(|channel| {
            NotificationChannelType::parse_str(channel)
                .ok_or_else(|| format!("Invalid notification channel: {}", channel))
        })
        .collect();
    let channels = channels?;

    let message = NotificationMessage::new(
        request.title,
        request.content,
        level,
        channels,
        request.recipients,
    );

    notification_manager
        .send_notification(&message)
        .await
        .map_err(|e| format!("Failed to send notification: {}", e))?;

    Ok(())
}

// ──────────────────────────────────────────────
// Notification Channels Handlers
// ──────────────────────────────────────────────

/// List notification channels
async fn list_channels(
    State(state): State<AppState>,
    Query(mut params): Query<NotificationChannelQueryParams>,
    claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<NotificationChannel>>> {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);

    params.workspace_id = Some(claims.workspace_id.clone());

    let (channels_result, count_result) = tokio::join!(
        find_all_notification_channels(&state.database, &params),
        count_notification_channels(&state.database, &params),
    );

    match channels_result {
        Ok(channels) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            ApiResponseBuilder::success(PaginatedResponse {
                data: channels,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list channels: {}", e);
            ApiResponseBuilder::error("获取通知渠道列表失败")
        }
    }
}

/// Get a single channel by ID
async fn get_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<NotificationChannel>> {
    let db = state.database.clone();
    match find_notification_channel_by_id(&db, &id).await {
        Ok(Some(channel)) => {
            if let Some(ref channel_ws) = channel.workspace_id {
                if channel_ws != &claims.workspace_id {
                    return ApiResponseBuilder::error_with_code(404, "通知渠道不存在");
                }
            }
            ApiResponseBuilder::success(channel)
        }
        Ok(None) => ApiResponseBuilder::error_with_code(404, "通知渠道不存在"),
        Err(e) => {
            tracing::error!("Failed to get channel: {}", e);
            ApiResponseBuilder::error("获取通知渠道失败")
        }
    }
}

/// Create a new notification channel
async fn create_channel(
    State(state): State<AppState>,
    claims: Claims,
    Json(payload): Json<CreateNotificationChannelRequest>,
) -> Json<ApiResponse<NotificationChannel>> {
    let db = state.database.clone();

    if !["sms", "email", "webhook"].contains(&payload.channel_type.as_str()) {
        return ApiResponseBuilder::error_with_code(400, "无效的通知渠道类型");
    }

    if let Err(e) = serde_json::from_str::<serde_json::Value>(&payload.config) {
        tracing::error!("Invalid config JSON: {}", e);
        return ApiResponseBuilder::error_with_code(400, "无效的配置 JSON");
    }

    match create_notification_channel(&db, &payload, Some(&claims.workspace_id)).await {
        Ok(channel) => ApiResponseBuilder::success(channel),
        Err(e) => {
            tracing::error!("Failed to create channel: {}", e);
            ApiResponseBuilder::error("创建通知渠道失败")
        }
    }
}

/// Update an existing notification channel
async fn update_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(payload): Json<UpdateNotificationChannelRequest>,
) -> Json<ApiResponse<NotificationChannel>> {
    let db = state.database.clone();

    // Verify workspace ownership
    if let Ok(Some(channel)) = find_notification_channel_by_id(&db, &id).await {
        if let Some(ref channel_ws) = channel.workspace_id {
            if channel_ws != &claims.workspace_id {
                return ApiResponseBuilder::error_with_code(404, "通知渠道不存在");
            }
        }
    }

    if let Some(ref channel_type) = payload.channel_type
        && !["sms", "email", "webhook"].contains(&channel_type.as_str()) {
            return ApiResponseBuilder::error_with_code(400, "无效的通知渠道类型");
        }

    if let Some(ref config) = payload.config
        && let Err(e) = serde_json::from_str::<serde_json::Value>(config) {
            tracing::error!("Invalid config JSON: {}", e);
            return ApiResponseBuilder::error_with_code(400, "无效的配置 JSON");
        }

    match update_notification_channel(&db, &id, &payload).await {
        Ok(channel) => ApiResponseBuilder::success(channel),
        Err(e) => {
            tracing::error!("Failed to update channel: {}", e);
            ApiResponseBuilder::error("更新通知渠道失败")
        }
    }
}

/// Delete a notification channel
async fn delete_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<bool>> {
    let db = state.database.clone();

    // Verify workspace ownership
    if let Ok(Some(channel)) = find_notification_channel_by_id(&db, &id).await {
        if let Some(ref channel_ws) = channel.workspace_id {
            if channel_ws != &claims.workspace_id {
                return ApiResponseBuilder::error_with_code(404, "通知渠道不存在");
            }
        }
    }

    match delete_notification_channel(&db, &id).await {
        Ok(_) => ApiResponseBuilder::success(true),
        Err(e) => {
            tracing::error!("Failed to delete channel: {}", e);
            ApiResponseBuilder::error("删除通知渠道失败")
        }
    }
}

/// Test a notification channel
async fn test_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(payload): Json<SendMessageRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db = state.database.clone();

    let channel = match find_notification_channel_by_id(&db, &id).await {
        Ok(Some(c)) => {
            if let Some(ref channel_ws) = c.workspace_id {
                if channel_ws != &claims.workspace_id {
                    return ApiResponseBuilder::error_with_code(404, "通知渠道不存在");
                }
            }
            c
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "通知渠道不存在"),
        Err(e) => {
            tracing::error!("Failed to get channel: {}", e);
            return ApiResponseBuilder::error("获取通知渠道失败");
        }
    };

    if !channel.is_enabled {
        return ApiResponseBuilder::error_with_code(400, "通知渠道未启用");
    }

    match send_notification_message(&channel, &payload).await {
        Ok(result) => {
            tracing::info!("Test message sent successfully: {}", result);
            ApiResponseBuilder::success(serde_json::json!({
                "success": true,
                "message": result
            }))
        }
        Err(e) => {
            tracing::error!("Failed to send test message: {}", e);
            ApiResponseBuilder::success(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

/// Get channel statistics
async fn get_statistics(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<ChannelStatistics>> {
    let db = state.database.clone();
    match get_notification_channel_statistics(&db, Some(&claims.workspace_id)).await {
        Ok(stats) => ApiResponseBuilder::success(stats),
        Err(e) => {
            tracing::error!("Failed to get statistics: {}", e);
            ApiResponseBuilder::error("获取统计信息失败")
        }
    }
}

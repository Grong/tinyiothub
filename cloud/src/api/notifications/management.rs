use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    domain::event::{
        services::notification_service::NotificationLevel, NotificationChannelType,
        NotificationRule,
    },
    dto::response::{api_response::ApiResponse, builder::ApiResponseBuilder},
    shared::{app_state::AppState, security::jwt::Claims},
};

/// Helper function to convert JsonValue device filter to DeviceFilterResponse
fn convert_device_filter(filter: &serde_json::Value) -> DeviceFilterResponse {
    DeviceFilterResponse {
        device_ids: filter.get("device_ids").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
        }),
        device_types: filter.get("device_types").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
        }),
        tags: filter.get("tags").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
        }),
    }
}

/// Helper function to convert DeviceFilterRequest to JsonValue
fn device_filter_to_json(filter: &DeviceFilterRequest) -> serde_json::Value {
    serde_json::json!({
        "device_ids": filter.device_ids,
        "device_types": filter.device_types,
        "tags": filter.tags
    })
}

/// Device filter for notification rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFilter {
    pub device_ids: Option<Vec<String>>,
    pub device_types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Request to create a new notification rule
#[derive(Debug, Deserialize)]
pub struct CreateNotificationRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<DeviceFilterRequest>,
    pub notification_methods: Vec<String>,
    pub recipients: Vec<String>,
    pub enabled: Option<bool>,
}

/// Request to update a notification rule
#[derive(Debug, Deserialize)]
pub struct UpdateNotificationRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<DeviceFilterRequest>,
    pub notification_methods: Option<Vec<String>>,
    pub recipients: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

/// Device filter request
#[derive(Debug, Deserialize)]
pub struct DeviceFilterRequest {
    pub device_ids: Option<Vec<String>>,
    pub device_types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Notification rule response
#[derive(Debug, Serialize)]
pub struct NotificationRuleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<DeviceFilterResponse>,
    pub notification_methods: Vec<String>,
    pub recipients: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Device filter response
#[derive(Debug, Serialize)]
pub struct DeviceFilterResponse {
    pub device_ids: Option<Vec<String>>,
    pub device_types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Notification history response
#[derive(Debug, Serialize)]
pub struct NotificationHistoryResponse {
    pub id: String,
    pub event_id: String,
    pub rule_id: String,
    pub notification_method: String,
    pub recipient: String,
    pub status: String,
    pub sent_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for notification rules
#[derive(Debug, Deserialize)]
pub struct NotificationRuleQuery {
    pub enabled: Option<bool>,
    pub event_type: Option<String>,
    pub notification_method: Option<String>,
}

/// Query parameters for notification history
#[derive(Debug, Deserialize)]
pub struct NotificationHistoryQuery {
    pub event_id: Option<String>,
    pub rule_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Test notification request
#[derive(Debug, Deserialize)]
pub struct TestNotificationRequest {
    pub title: String,
    pub content: String,
    pub level: String,
    pub channels: Vec<String>,
    pub recipients: Vec<String>,
}

/// Get all notification rules
#[axum::debug_handler]
pub async fn get_notification_rules(
    Query(query): Query<NotificationRuleQuery>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<NotificationRuleResponse>>> {
    match get_notification_rules_impl(&state, query).await {
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
            if let Some(enabled) = query.enabled {
                if rule.enabled != enabled {
                    return false;
                }
            }

            if let Some(ref event_type) = query.event_type {
                if rule.event_type.as_ref() != Some(event_type) {
                    return false;
                }
            }

            if let Some(ref method) = query.notification_method {
                if let Some(channel) = NotificationChannelType::from_str(method) {
                    if !rule.notification_methods.contains(&channel) {
                        return false;
                    }
                }
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
    _claims: Claims,
    Json(request): Json<CreateNotificationRuleRequest>,
) -> Json<ApiResponse<NotificationRuleResponse>> {
    match create_notification_rule_impl(&state, request).await {
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
) -> Result<NotificationRuleResponse, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    // Parse notification methods
    let notification_methods: Result<Vec<NotificationChannelType>, _> = request
        .notification_methods
        .iter()
        .map(|method| {
            NotificationChannelType::from_str(method)
                .ok_or_else(|| format!("Invalid notification method: {}", method))
        })
        .collect();
    let notification_methods = notification_methods?;

    // Create device filter if provided
    let device_filter = request.device_filter.map(|filter| device_filter_to_json(&filter));

    // Create the rule
    let mut rule = NotificationRule::new(
        Uuid::new_v4().to_string(),
        request.name,
        request.description,
        notification_methods.clone(),
        request.recipients.clone(),
    );

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

    // Add the rule to the manager
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
    _claims: Claims,
) -> Json<ApiResponse<NotificationRuleResponse>> {
    match get_notification_rule_impl(&state, &rule_id).await {
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
) -> Result<Option<NotificationRuleResponse>, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    let rules = notification_manager
        .get_rules()
        .await
        .map_err(|e| format!("Failed to get notification rules: {}", e))?;

    let rule = rules.into_iter().find(|r| r.id == rule_id);

    if let Some(rule) = rule {
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
    _claims: Claims,
    Path(rule_id): Path<String>,
    Json(request): Json<UpdateNotificationRuleRequest>,
) -> Json<ApiResponse<NotificationRuleResponse>> {
    match update_notification_rule_impl(&state, &rule_id, request).await {
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
) -> Result<NotificationRuleResponse, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    // Get existing rule
    let rules = notification_manager
        .get_rules()
        .await
        .map_err(|e| format!("Failed to get notification rules: {}", e))?;

    let mut rule =
        rules.into_iter().find(|r| r.id == rule_id).ok_or("Notification rule not found")?;

    // Update fields
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
                NotificationChannelType::from_str(method)
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

    // Update the rule
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
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    match delete_notification_rule_impl(&state, &rule_id).await {
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

async fn delete_notification_rule_impl(state: &AppState, rule_id: &str) -> Result<(), String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

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
    _claims: Claims,
) -> Json<ApiResponse<Vec<NotificationHistoryResponse>>> {
    match get_notification_history_impl(&state, query).await {
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
) -> Result<Vec<NotificationHistoryResponse>, String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    // For now, return empty history as we need to implement the history retrieval
    // In a full implementation, this would query the notification history store
    let history = if let Some(event_id) = query.event_id {
        notification_manager
            .get_notification_history(&event_id)
            .await
            .map_err(|e| format!("Failed to get notification history: {}", e))?
    } else {
        // Return empty for now - would need pagination support in the manager
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
    _claims: Claims,
    Json(request): Json<TestNotificationRequest>,
) -> Json<ApiResponse<bool>> {
    match send_test_notification_impl(&state, request).await {
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
) -> Result<(), String> {
    let notification_manager =
        state.get_notification_manager().ok_or("Notification manager not available")?;

    // Parse notification level
    let level = NotificationLevel::from_str(&request.level)
        .map_err(|_e| format!("Invalid notification level: {}", request.level))?;

    // Parse notification channels
    let channels: Result<Vec<NotificationChannelType>, _> = request
        .channels
        .iter()
        .map(|channel| {
            NotificationChannelType::from_str(channel)
                .ok_or_else(|| format!("Invalid notification channel: {}", channel))
        })
        .collect();
    let channels = channels?;

    // Create test notification message
    let message = crate::domain::event::services::NotificationMessage::new(
        request.title,
        request.content,
        level,
        channels,
        request.recipients,
    );

    // Send the notification
    notification_manager
        .send_notification(&message)
        .await
        .map_err(|e| format!("Failed to send notification: {}", e))?;

    Ok(())
}

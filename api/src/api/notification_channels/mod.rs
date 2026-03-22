// Notification Channels API Module
// 通知渠道配置 API

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};

use crate::{
    dto::entity::notification_channel::{
        ChannelStatistics, CreateNotificationChannelRequest, NotificationChannel,
        NotificationChannelQueryParams, SendMessageRequest, UpdateNotificationChannelRequest,
    },
    shared::app_state::AppState,
};

/// Create notification channels router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Channel CRUD
        .route("/notification-channels", get(list_channels))
        .route("/notification-channels", post(create_channel))
        .route("/notification-channels/{id}", get(get_channel))
        .route("/notification-channels/{id}", put(update_channel))
        .route("/notification-channels/{id}", delete(delete_channel))
        // Channel Actions
        .route("/notification-channels/{id}/enable", post(enable_channel))
        .route("/notification-channels/{id}/disable", post(disable_channel))
        .route("/notification-channels/{id}/test", post(test_channel))
        // Statistics
        .route("/notification-channels/statistics", get(get_statistics))
}

/// List notification channels
async fn list_channels(
    State(state): State<AppState>,
    Query(params): Query<NotificationChannelQueryParams>,
) -> Result<Json<Vec<NotificationChannel>>, StatusCode> {
    let db = state.database.clone();

    match NotificationChannel::find_all(&db, &params).await {
        Ok(channels) => Ok(Json(channels)),
        Err(e) => {
            tracing::error!("Failed to list channels: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get a single channel by ID
async fn get_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NotificationChannel>, StatusCode> {
    let db = state.database.clone();

    match NotificationChannel::find_by_id(&db, &id).await {
        Ok(Some(channel)) => Ok(Json(channel)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create a new notification channel
async fn create_channel(
    State(state): State<AppState>,
    Json(payload): Json<CreateNotificationChannelRequest>,
) -> Result<Json<NotificationChannel>, StatusCode> {
    let db = state.database.clone();

    // 验证渠道类型
    if !["sms", "email", "webhook"].contains(&payload.channel_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 验证配置 JSON
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&payload.config) {
        tracing::error!("Invalid config JSON: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }

    match NotificationChannel::create(&db, &payload).await {
        Ok(channel) => Ok(Json(channel)),
        Err(e) => {
            tracing::error!("Failed to create channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update an existing notification channel
async fn update_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateNotificationChannelRequest>,
) -> Result<Json<NotificationChannel>, StatusCode> {
    let db = state.database.clone();

    // 验证渠道类型
    if let Some(ref channel_type) = payload.channel_type {
        if !["sms", "email", "webhook"].contains(&channel_type.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // 验证配置 JSON
    if let Some(ref config) = payload.config {
        if let Err(e) = serde_json::from_str::<serde_json::Value>(config) {
            tracing::error!("Invalid config JSON: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    match NotificationChannel::update(&db, &id, &payload).await {
        Ok(channel) => Ok(Json(channel)),
        Err(e) => {
            tracing::error!("Failed to update channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete a notification channel
async fn delete_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let db = state.database.clone();

    match NotificationChannel::delete(&db, &id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            tracing::error!("Failed to delete channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Enable a notification channel
async fn enable_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NotificationChannel>, StatusCode> {
    let db = state.database.clone();

    match NotificationChannel::set_enabled(&db, &id, true).await {
        Ok(channel) => Ok(Json(channel)),
        Err(e) => {
            tracing::error!("Failed to enable channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Disable a notification channel
async fn disable_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NotificationChannel>, StatusCode> {
    let db = state.database.clone();

    match NotificationChannel::set_enabled(&db, &id, false).await {
        Ok(channel) => Ok(Json(channel)),
        Err(e) => {
            tracing::error!("Failed to disable channel: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Test a notification channel
async fn test_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();

    // 获取渠道配置
    let channel = match NotificationChannel::find_by_id(&db, &id).await {
        Ok(Some(c)) => c,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get channel: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // 检查是否启用
    if !channel.is_enabled {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 发送测试消息
    match channel.send_message(&payload).await {
        Ok(result) => {
            tracing::info!("Test message sent successfully: {}", result);
            Ok(Json(serde_json::json!({
                "success": true,
                "message": result
            })))
        }
        Err(e) => {
            tracing::error!("Failed to send test message: {}", e);
            Ok(Json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

/// Get channel statistics
async fn get_statistics(
    State(state): State<AppState>,
) -> Result<Json<ChannelStatistics>, StatusCode> {
    let db = state.database.clone();

    match NotificationChannel::get_statistics(&db).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            tracing::error!("Failed to get statistics: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

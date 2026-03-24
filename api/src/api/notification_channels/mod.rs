// Notification Channels API Module
// 通知渠道配置 API

use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json,
    Router,
};

use crate::{
    dto::entity::notification_channel::{
        ChannelStatistics, CreateNotificationChannelRequest, NotificationChannel,
        NotificationChannelQueryParams, SendMessageRequest, UpdateNotificationChannelRequest,
    },
    dto::response::{ApiResponse, builder::ApiResponseBuilder},
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
        // 复杂业务动作，保持 RPC 风格
        .route("/notification-channels/{id}/test", post(test_channel))
        // Statistics
        .route("/notification-channels/statistics", get(get_statistics))
}

/// List notification channels
async fn list_channels(
    State(state): State<AppState>,
    Query(params): Query<NotificationChannelQueryParams>,
) -> Json<ApiResponse<Vec<NotificationChannel>>> {
    let db = state.database.clone();

    match NotificationChannel::find_all(&db, &params).await {
        Ok(channels) => ApiResponseBuilder::success(channels),
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
) -> Json<ApiResponse<NotificationChannel>> {
    let db = state.database.clone();

    match NotificationChannel::find_by_id(&db, &id).await {
        Ok(Some(channel)) => ApiResponseBuilder::success(channel),
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
    Json(payload): Json<CreateNotificationChannelRequest>,
) -> Json<ApiResponse<NotificationChannel>> {
    let db = state.database.clone();

    // 验证渠道类型
    if !["sms", "email", "webhook"].contains(&payload.channel_type.as_str()) {
        return ApiResponseBuilder::error_with_code(400, "无效的通知渠道类型");
    }

    // 验证配置 JSON
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&payload.config) {
        tracing::error!("Invalid config JSON: {}", e);
        return ApiResponseBuilder::error_with_code(400, "无效的配置 JSON");
    }

    match NotificationChannel::create(&db, &payload).await {
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
    Json(payload): Json<UpdateNotificationChannelRequest>,
) -> Json<ApiResponse<NotificationChannel>> {
    let db = state.database.clone();

    // 验证渠道类型
    if let Some(ref channel_type) = payload.channel_type {
        if !["sms", "email", "webhook"].contains(&channel_type.as_str()) {
            return ApiResponseBuilder::error_with_code(400, "无效的通知渠道类型");
        }
    }

    // 验证配置 JSON
    if let Some(ref config) = payload.config {
        if let Err(e) = serde_json::from_str::<serde_json::Value>(config) {
            tracing::error!("Invalid config JSON: {}", e);
            return ApiResponseBuilder::error_with_code(400, "无效的配置 JSON");
        }
    }

    match NotificationChannel::update(&db, &id, &payload).await {
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
) -> Json<ApiResponse<bool>> {
    let db = state.database.clone();

    match NotificationChannel::delete(&db, &id).await {
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
    Json(payload): Json<SendMessageRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db = state.database.clone();

    // 获取渠道配置
    let channel = match NotificationChannel::find_by_id(&db, &id).await {
        Ok(Some(c)) => c,
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "通知渠道不存在"),
        Err(e) => {
            tracing::error!("Failed to get channel: {}", e);
            return ApiResponseBuilder::error("获取通知渠道失败");
        }
    };

    // 检查是否启用
    if !channel.is_enabled {
        return ApiResponseBuilder::error_with_code(400, "通知渠道未启用");
    }

    // 发送测试消息
    match channel.send_message(&payload).await {
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
                "error": e
            }))
        }
    }
}

/// Get channel statistics
async fn get_statistics(State(state): State<AppState>) -> Json<ApiResponse<ChannelStatistics>> {
    let db = state.database.clone();

    match NotificationChannel::get_statistics(&db).await {
        Ok(stats) => ApiResponseBuilder::success(stats),
        Err(e) => {
            tracing::error!("Failed to get statistics: {}", e);
            ApiResponseBuilder::error("获取统计信息失败")
        }
    }
}

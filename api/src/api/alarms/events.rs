use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        entity::device_event_trigger::{DeviceEventTrigger, DeviceEventTriggerQueryParams},
        response::{ApiResponse, builder::ApiResponseBuilder, PaginatedResponse, PaginationInfo},
    },
    shared::security::jwt::Claims,
};

pub struct CreateEventTriggerRequest {
    pub device_id: String,
    pub trigger_name: String,
    pub event_type: String,
    pub condition: String,
    pub action: String,
    pub enabled: Option<bool>,
}

pub struct UpdateEventTriggerRequest {
    pub trigger_name: Option<String>,
    pub event_type: Option<String>,
    pub condition: Option<String>,
    pub action: Option<String>,
    pub enabled: Option<bool>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_event_triggers).post(create_event_trigger))
        .route("/{id}", get(get_event_trigger).put(update_event_trigger).delete(delete_event_trigger))
        .route("/{id}/enable", post(enable_event_trigger))
        .route("/{id}/disable", post(disable_event_trigger))
}

/// 获取事件触发器列表
async fn list_event_triggers(
    State(state): State<AppState>,
    Query(query): Query<DeviceEventTriggerQueryParams>,
    _claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<DeviceEventTrigger>>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let (triggers_result, count_result) = tokio::join!(
        DeviceEventTrigger::find_all(state.database(), &query),
        DeviceEventTrigger::count(state.database(), &query),
    );

    match triggers_result {
        Ok(triggers) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            ApiResponseBuilder::success(PaginatedResponse {
                data: triggers,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            })
        }
        Err(e) => {
            tracing::error!("Failed to fetch event triggers: {}", e);
            ApiResponseBuilder::error("获取事件触发器列表失败")
        }
    }
}

/// 创建事件触发器
async fn create_event_trigger(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateEventTriggerRequest>,
) -> Json<ApiResponse<DeviceEventTrigger>> {
    // TODO: 实现事件触发器创建逻辑
    tracing::info!("Creating event trigger: {}", req.trigger_name);
    
    // 由于字段不匹配，先返回错误
    ApiResponse::error("事件触发器创建功能尚未实现".to_string())
}

/// 获取事件触发器详情
async fn get_event_trigger(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DeviceEventTrigger>>> {
    // TODO: 实现事件触发器详情查询逻辑
    tracing::info!("Getting event trigger details for: {}", id);
    
    ApiResponse::success(None)
}

/// 更新事件触发器
async fn update_event_trigger(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(req): Json<UpdateEventTriggerRequest>,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现事件触发器更新逻辑
    tracing::info!("Updating event trigger: {}", id);
    
    ApiResponse::success(true)
}

/// 删除事件触发器
async fn delete_event_trigger(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现事件触发器删除逻辑
    tracing::info!("Deleting event trigger: {}", id);
    
    ApiResponse::success(true)
}

/// 启用事件触发器
async fn enable_event_trigger(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现事件触发器启用逻辑
    tracing::info!("Enabling event trigger: {}", id);
    
    ApiResponse::success(true)
}

/// 禁用事件触发器
async fn disable_event_trigger(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // TODO: 实现事件触发器禁用逻辑
    tracing::info!("Disabling event trigger: {}", id);
    
    ApiResponse::success(true)
}
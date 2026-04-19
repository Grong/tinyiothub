use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    api::middleware::WorkspaceScope,
    dto::response::{
        builder::ApiResponseBuilder, ApiResponse, DeviceStatusDistribution, QuickDevice,
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

#[derive(Debug, Deserialize)]
pub struct QuickDevicesQuery {
    limit: Option<i32>,
}

/// 获取设备状态分布
pub async fn get_device_distribution(
    State(state): State<AppState>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<DeviceStatusDistribution>> {
    info!("Getting device status distribution");

    match state.device_query_service.get_device_status_distribution(workspace_id.as_deref()).await {
        Ok(distribution) => ApiResponseBuilder::success(distribution),
        Err(e) => {
            error!("Failed to get device status distribution: {}", e);
            ApiResponseBuilder::error("获取设备状态分布失败")
        }
    }
}

/// 获取关键设备列表
pub async fn get_quick_devices(
    State(state): State<AppState>,
    Query(query): Query<QuickDevicesQuery>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<QuickDevice>>> {
    info!("Getting quick devices list with limit: {:?}", query.limit);

    let limit = query.limit.unwrap_or(8);
    match state.device_query_service.get_quick_devices_list(limit, workspace_id.as_deref()).await {
        Ok(devices) => ApiResponseBuilder::success(devices),
        Err(e) => {
            error!("Failed to get quick devices list: {}", e);
            ApiResponseBuilder::error("获取关键设备列表失败")
        }
    }
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/distribution", get(get_device_distribution))
        .route("/quick", get(get_quick_devices))
}

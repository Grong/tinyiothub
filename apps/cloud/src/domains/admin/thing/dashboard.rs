use crate::domains::admin::AdminState;
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;
use tracing::{error, info};

use tinyiothub_storage::thing::{QuickThing, ThingStatusDistribution};

use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::middleware::workspace::WorkspaceScope;

#[derive(Debug, Deserialize)]
pub struct QuickThingsQuery {
    limit: Option<i32>,
}

/// 获取设备状态分布
pub async fn get_device_distribution(
    State(state): State<AdminState>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<ThingStatusDistribution>> {
    info!("Getting device status distribution");

    match state
        .device_query_service
        .get_device_status_distribution(workspace_id.as_deref())
        .await
    {
        Ok(distribution) => ApiResponseBuilder::success(distribution),
        Err(e) => {
            error!("Failed to get device status distribution: {}", e);
            ApiResponseBuilder::error("获取设备状态分布失败")
        }
    }
}

/// 获取关键设备列表
pub async fn get_quick_things(
    State(state): State<AdminState>,
    Query(query): Query<QuickThingsQuery>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<QuickThing>>> {
    info!("Getting quick things list with limit: {:?}", query.limit);

    let limit = query.limit.unwrap_or(8);
    match state
        .device_query_service
        .get_quick_devices_list(limit, workspace_id.as_deref())
        .await
    {
        Ok(things) => ApiResponseBuilder::success(things),
        Err(e) => {
            error!("Failed to get quick things list: {}", e);
            ApiResponseBuilder::error("获取关键设备列表失败")
        }
    }
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/distribution", get(get_device_distribution))
        .route("/quick", get(get_quick_things))
}

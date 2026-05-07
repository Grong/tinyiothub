use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use serde::Deserialize;
use tinyiothub_core::models::device_property::DeviceProperty;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    api::middleware::WorkspaceScope,
    shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims},
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePropertyValueRequest {
    pub value: String,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/{device_id}/properties", get(get_device_properties))
        .route("/{device_id}/properties/{property_id}/value", put(update_property_value))
        .route(
            "/by-name/{device_name}/properties/{property_name}",
            get(get_device_property_by_name),
        )
}

/// 获取设备属性列表
async fn get_device_properties(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<DeviceProperty>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.get_device_properties(&device_id).await {
        Ok(properties) => ApiResponseBuilder::success(properties),
        Err(e) => {
            tracing::error!("Failed to get device properties for {}: {}", device_id, e);
            ApiResponseBuilder::error("获取设备属性失败")
        }
    }
}

/// 通过设备名称和属性名称获取属性
async fn get_device_property_by_name(
    State(state): State<AppState>,
    Path((device_name, property_name)): Path<(String, String)>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Option<DeviceProperty>>> {
    // 先通过名称查找设备，再验证租户
    let tenant_device_service = state.tenant_device_service(&workspace_id);
    let _device = match tenant_device_service.get_device_by_name(&device_name).await {
        Ok(Some(d)) => d,
        Ok(None) => return ApiResponseBuilder::error("设备不存在"),
        Err(e) => {
            tracing::error!("Failed to find device by name {}: {}", device_name, e);
            return ApiResponseBuilder::error("查询设备失败");
        }
    };
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id
    let property = state.get_device_prop_by_name(&device_name, &property_name);
    ApiResponseBuilder::success(property)
}

/// 更新设备属性值
async fn update_property_value(
    State(state): State<AppState>,
    Path((device_id, property_id)): Path<(String, String)>,
    claims: Claims,
    Json(req): Json<UpdatePropertyValueRequest>,
) -> Json<ApiResponse<bool>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id
    match state
        .update_device_property_value(&claims.workspace_id, &device_id, &property_id, &req.value)
        .await
    {
        Ok(()) => {
            tracing::info!(
                "Property value updated: device={}, property={}, value={}",
                device_id,
                property_id,
                req.value
            );
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            tracing::error!("Failed to update property value: {}", e);
            match e {
                crate::shared::error::Error::NotFound => {
                    ApiResponseBuilder::error("设备或属性不存在")
                }
                crate::shared::error::Error::ValidationError(msg) => ApiResponseBuilder::error(msg),
                _ => ApiResponseBuilder::error("更新属性值失败"),
            }
        }
    }
}

use crate::domains::admin::AdminState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use serde::Deserialize;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::middleware::workspace::WorkspaceScope;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePropertyValueRequest {
    pub value: String,
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{thing_id}/properties", get(get_device_properties))
        .route("/{thing_id}/properties/{property_id}/value", put(update_property_value))
        .route(
            "/by-name/{device_name}/properties/{property_name}",
            get(get_device_property_by_name),
        )
}

/// 获取设备属性列表
async fn get_device_properties(
    State(state): State<AdminState>,
    Path(thing_id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<ThingProperty>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters things by workspace_id

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.get_device_properties(&thing_id).await {
        Ok(properties) => ApiResponseBuilder::success(properties),
        Err(e) => {
            tracing::error!("Failed to get device properties for {}: {}", thing_id, e);
            ApiResponseBuilder::error("获取设备属性失败")
        }
    }
}

/// 通过设备名称和属性名称获取属性
async fn get_device_property_by_name(
    State(state): State<AdminState>,
    Path((device_name, property_name)): Path<(String, String)>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Option<ThingProperty>>> {
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
    // which automatically filters things by workspace_id
    let property = state.get_device_prop_by_name(&device_name, &property_name);
    ApiResponseBuilder::success(property)
}

/// 更新设备属性值
async fn update_property_value(
    State(state): State<AdminState>,
    Path((thing_id, property_id)): Path<(String, String)>,
    claims: Claims,
    Json(req): Json<UpdatePropertyValueRequest>,
) -> Json<ApiResponse<bool>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters things by workspace_id
    match state
        .update_device_property_value(&claims.workspace_id, &thing_id, &property_id, &req.value)
        .await
    {
        Ok(()) => {
            tracing::info!(
                "Property value updated: device={}, property={}, value={}",
                thing_id,
                property_id,
                req.value
            );
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            tracing::error!("Failed to update property value: {}", e);
            match e {
                tinyiothub_core::error::Error::NotFound => ApiResponseBuilder::error("设备或属性不存在"),
                tinyiothub_core::error::Error::ValidationError(msg) => ApiResponseBuilder::error(msg),
                _ => ApiResponseBuilder::error("更新属性值失败"),
            }
        }
    }
}

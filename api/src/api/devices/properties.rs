use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    dto::{
        entity::device_property::DeviceProperty,
        response::{builder::ApiResponseBuilder, ApiResponse},
    },
    shared::{app_state::AppState, security::jwt::Claims},
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
        .route("/by-name/{device_name}/properties/{property_name}", get(get_device_property_by_name))
}

/// 获取设备属性列表
async fn get_device_properties(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    claims: Claims,
) -> Json<ApiResponse<Vec<DeviceProperty>>> {
    if let Err(e) = super::verify_device_tenant(&state, &device_id, &claims.tenant_id).await {
        return match e {
            crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
            _ => ApiResponseBuilder::error("查询设备失败"),
        };
    }

    match state.device_service.get_device_properties(&device_id).await {
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
    claims: Claims,
) -> Json<ApiResponse<Option<DeviceProperty>>> {
    // 先通过名称查找设备，再验证租户
    let device = match state.device_service.get_device_by_name(&device_name).await {
        Ok(Some(d)) => d,
        Ok(None) => return ApiResponseBuilder::error("设备不存在"),
        Err(e) => {
            tracing::error!("Failed to find device by name {}: {}", device_name, e);
            return ApiResponseBuilder::error("查询设备失败");
        }
    };
    if let Err(e) = super::verify_device_tenant(&state, &device.id, &claims.tenant_id).await {
        return match e {
            crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
            _ => ApiResponseBuilder::error("查询设备失败"),
        };
    }
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
    if let Err(e) = super::verify_device_tenant(&state, &device_id, &claims.tenant_id).await {
        return match e {
            crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
            _ => ApiResponseBuilder::error("查询设备失败"),
        };
    }
    match state.update_device_property_value(&device_id, &property_id, &req.value).await {
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

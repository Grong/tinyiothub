/// Device Data API Handlers
/// 设备历史数据 API 实现

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;

use crate::dto::{
    entity::device_data::{
        BatchCreateDeviceDataRequest, CreateDeviceDataRequest, DeviceData, DeviceDataQuery,
        DeviceDataStats,
    },
    response::api_response::ApiResponse,
};
use crate::shared::app_state::AppState;

#[derive(Debug, Deserialize)]
pub struct DataQueryParams {
    pub property_name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 获取设备历史数据
pub async fn get_device_data(
    Path(device_id): Path<String>,
    Query(params): Query<DataQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DeviceData>>>, StatusCode> {
    let query = DeviceDataQuery {
        property_name: params.property_name,
        start_time: params.start_time,
        end_time: params.end_time,
        page: params.page,
        page_size: params.page_size,
    };

    match DeviceData::find_by_device(state.database(), &device_id, &query).await {
        Ok(data) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Success".to_string(),
            result: Some(data),
        })),
        Err(e) => {
            tracing::error!("Failed to fetch device data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取设备最新数据
pub async fn get_device_latest_data(
    Path(device_id): Path<String>,
    Query(params): Query<DataQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::dto::entity::device_data::LatestDeviceData>>>, StatusCode> {
    match DeviceData::find_latest(state.database(), &device_id, params.property_name.as_deref()).await {
        Ok(data) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Success".to_string(),
            result: Some(data),
        })),
        Err(e) => {
            tracing::error!("Failed to fetch latest device data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 上报设备数据
pub async fn create_device_data(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<CreateDeviceDataRequest>,
) -> Result<Json<ApiResponse<DeviceData>>, StatusCode> {
    // 验证设备是否存在
    if let Ok(Some(_)) = crate::dto::entity::device::Device::find_by_id(state.database(), &device_id).await {
        // 设备存在，继续
    } else {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut req = payload;
    req.device_id = device_id;

    match DeviceData::create(state.database(), &req).await {
        Ok(data) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Data created successfully".to_string(),
            result: Some(data),
        })),
        Err(e) => {
            tracing::error!("Failed to create device data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 批量上报设备数据
pub async fn batch_create_device_data(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<BatchCreateDeviceDataRequest>,
) -> Result<Json<ApiResponse<Vec<DeviceData>>>, StatusCode> {
    // 验证设备是否存在
    if let Ok(Some(_)) = crate::dto::entity::device::Device::find_by_id(state.database(), &device_id).await {
        // 设备存在，继续
    } else {
        return Err(StatusCode::NOT_FOUND);
    }

    match DeviceData::create_batch(state.database(), &device_id, &payload.data_points).await {
        Ok(data) => Ok(Json(ApiResponse {
            code: 0,
            msg: format!("Created {} data points", data.len()),
            result: Some(data),
        })),
        Err(e) => {
            tracing::error!("Failed to batch create device data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 删除历史数据
pub async fn delete_device_data(
    Path(device_id): Path<String>,
    Query(params): Query<DataQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    // 支持删除特定天数之前的数据
    let days = params.page_size.unwrap_or(7) as i64;
    
    match DeviceData::delete_old(state.database(), days).await {
        Ok(count) => Ok(Json(ApiResponse {
            code: 0,
            msg: format!("Deleted {} old data records", count),
            result: Some(()),
        })),
        Err(e) => {
            tracing::error!("Failed to delete device data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取设备数据统计
pub async fn get_device_data_stats(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DeviceDataStats>>>, StatusCode> {
    // TODO: 实现统计数据查询
    Ok(Json(ApiResponse {
        code: 0,
        msg: "Success".to_string(),
        result: Some(vec![]),
    }))
}

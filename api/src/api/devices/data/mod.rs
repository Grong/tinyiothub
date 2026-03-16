/// Device Data API Module
/// 设备历史数据 API
/// 路由: /api/v1/devices/:device_id/data/*

pub mod management;

// Re-export API handlers
pub use management::*;

use crate::shared::app_state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

/// Create device data API router
/// 注意：此路由已嵌套在 /devices 下，所以直接使用 /data
pub fn create_router() -> Router<AppState> {
    Router::new()
        // 获取设备历史数据: /devices/{id}/data
        .route("/data", get(get_device_data))
        // 获取设备最新数据: /devices/{id}/data/latest
        .route("/data/latest", get(get_device_latest_data))
        // 上报设备数据: /devices/{id}/data
        .route("/data", post(create_device_data))
        // 批量上报设备数据: /devices/{id}/data/batch
        .route("/data/batch", post(batch_create_device_data))
        // 删除历史数据: /devices/{id}/data
        .route("/data", delete(delete_device_data))
        // 获取数据统计: /devices/{id}/data/stats
        .route("/data/stats", get(get_device_data_stats))
}

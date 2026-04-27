// Drivers API — moved from api/drivers/

use tinyiothub_core::models::component::{Component, ComponentOption};
use std::collections::HashMap;

use axum::{
    extract::{Path, Query},
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    modules::device::driver::get_driver_list,
    shared::api_response::{ApiResponse, PaginatedResponse, PaginationInfo},
    shared::app_state::AppState,
};
use tinyiothub_web::response::ApiResponseBuilder;

/// 驱动详情响应
#[derive(Serialize, Deserialize)]
pub struct DriverDetailResponse {
    pub driver: Component,
}

/// 驱动配置参数响应
#[derive(Serialize, Deserialize)]
pub struct DriverConfigResponse {
    pub driver_name: String,
    pub config_options: Vec<ComponentOption>,
    pub default_config: HashMap<String, String>,
}

/// 创建驱动 API 路由
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_drivers))
        .route("/names", get(list_driver_names))
        .route("/{name}", get(get_driver_detail))
        .route("/{name}/config", get(get_driver_config))
        .route("/{name}/supported", get(check_driver_support))
}

/// 获取驱动列表
async fn list_drivers(
    Query(params): Query<HashMap<String, String>>,
) -> Json<ApiResponse<PaginatedResponse<Component>>> {
    tracing::info!("Getting driver list, params: {:?}", params);

    let mut drivers = get_driver_list();

    if let Some(filter_name) = params.get("name") {
        drivers.retain(|driver| driver.name.to_lowercase().contains(&filter_name.to_lowercase()));
    }

    let total = drivers.len();
    let page: u32 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: u32 = params.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);

    let total_count = total as u64;
    let total_pages = if page_size > 0 {
        ((total as f64) / (page_size as f64)).ceil() as u32
    } else {
        0
    };

    let start = ((page.saturating_sub(1)) * page_size) as usize;
    let end = (start + page_size as usize).min(total);
    let paged = if start < total { &drivers[start..end] } else { &[] };

    tracing::info!("Found {} drivers, page {} of {}", total, page, total_pages);

    ApiResponseBuilder::success(PaginatedResponse {
        data: paged.to_vec(),
        pagination: PaginationInfo {
            page,
            page_size,
            total_pages,
            total_count,
        },
    })
}

/// 获取驱动详情
async fn get_driver_detail(Path(name): Path<String>) -> Json<ApiResponse<DriverDetailResponse>> {
    tracing::info!("Getting driver details for: {}", name);

    let drivers = get_driver_list();
    if let Some(driver) = drivers.into_iter().find(|d| d.name == name) {
        tracing::info!("Found driver: {}", driver.name);
        return ApiResponseBuilder::success(DriverDetailResponse { driver });
    }

    tracing::warn!("Driver not found: {}", name);
    ApiResponseBuilder::error(format!("Driver '{}' not found", name))
}

/// 检查驱动支持状态
async fn check_driver_support(Path(name): Path<String>) -> Json<ApiResponse<PaginatedResponse<Component>>> {
    tracing::info!("Checking if driver is supported: {}", name);

    let is_supported = crate::modules::device::driver::has_driver(&name);

    let total_count = if is_supported { 1 } else { 0 };
    let response = PaginatedResponse {
        data: vec![],
        pagination: PaginationInfo {
            page: 1,
            page_size: 1,
            total_pages: 1,
            total_count,
        },
    };

    tracing::info!("Driver {} support status: {}", name, is_supported);
    ApiResponseBuilder::success(response)
}

/// 获取驱动配置参数
async fn get_driver_config(Path(name): Path<String>) -> Json<ApiResponse<DriverConfigResponse>> {
    tracing::info!("Getting driver config for: {}", name);

    let drivers = get_driver_list();

    if let Some(driver) = drivers.into_iter().find(|d| d.name == name) {
        let config_options: Vec<ComponentOption> =
            if let Ok(options) = serde_json::from_str(&driver.options_descriptors) {
                options
            } else {
                vec![]
            };

        let mut default_config = HashMap::new();
        for option in &config_options {
            default_config.insert(option.name.clone(), option.default_value.clone());
        }

        tracing::info!("Found {} config options for driver: {}", config_options.len(), driver.name);

        ApiResponseBuilder::success(DriverConfigResponse {
            driver_name: driver.name,
            config_options,
            default_config,
        })
    } else {
        tracing::warn!("Driver not found: {}", name);
        ApiResponseBuilder::error(format!("Driver '{}' not found", name))
    }
}

/// 获取支持的驱动名称列表
async fn list_driver_names() -> Json<ApiResponse<Vec<String>>> {
    tracing::info!("Getting supported driver names");

    let driver_names = crate::modules::device::driver::get_all_driver_names();

    tracing::info!("Found {} supported driver names", driver_names.len());
    ApiResponseBuilder::success(driver_names)
}

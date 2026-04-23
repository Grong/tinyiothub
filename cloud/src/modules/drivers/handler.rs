// Drivers API — moved from api/drivers/

use tinyiothub_core::models::component::{Component, ComponentOption};
use std::collections::HashMap;

use axum::{
    extract::{Path, Query},
    response::Json,
    routing::{delete, get, post},
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
        // 动态驱动管理
        .route("/dynamic/load", post(dynamic_load_driver))
        .route("/dynamic/{name}/unload", delete(dynamic_unload_driver))
        .route("/dynamic/list", get(dynamic_list_all_drivers))
        .route("/dynamic/reload", post(dynamic_reload_drivers_dir))
}

/// 加载动态驱动请求
#[derive(Debug, Deserialize)]
pub struct LoadDriverRequest {
    pub path: String,
}

/// 驱动信息响应
#[derive(Debug, Serialize)]
pub struct DriverInfo {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub is_loaded: bool,
    pub path: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// 所有驱动列表响应
#[derive(Debug, Serialize)]
pub struct AllDriversResponse {
    pub static_drivers: Vec<DriverInfo>,
    pub dynamic: Vec<DriverInfo>,
}

/// 获取驱动列表
async fn list_drivers(
    Query(params): Query<HashMap<String, String>>,
) -> Json<ApiResponse<PaginatedResponse<Component>>> {
    tracing::info!("Getting driver list, params: {:?}", params);

    let mut drivers = get_driver_list();

    let registry = crate::modules::device::driver::dynamic::registry::get_global_registry();
    for driver_name in registry.get_driver_names() {
        if let Ok(driver_info) = registry.get_dynamic_driver_info(&driver_name) {
            drivers.push(driver_info);
        }
    }

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

    tracing::info!("Found {} drivers (static + dynamic), page {} of {}", total, page, total_pages);

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
        tracing::info!("Found static driver: {}", driver.name);
        return ApiResponseBuilder::success(DriverDetailResponse { driver });
    }

    let registry = crate::modules::device::driver::dynamic::registry::get_global_registry();
    if let Ok(driver) = registry.get_dynamic_driver_info(&name) {
        tracing::info!("Found dynamic driver: {}", driver.name);
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
async fn list_driver_names() -> Json<ApiResponse<Vec<Component>>> {
    tracing::info!("Getting supported driver names");

    let driver_names = crate::modules::device::driver::get_all_driver_names();

    let drivers: Vec<Component> = driver_names
        .into_iter()
        .map(|name| {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            Component {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.clone(),
                version: "unknown".to_string(),
                class_name: name,
                device_num: 0,
                description: None,
                options_descriptors: "[]".to_string(),
                location: None,
                created_at: now.clone(),
                updated_at: now,
            }
        })
        .collect();

    tracing::info!("Found {} supported driver names (static + dynamic)", drivers.len());
    ApiResponseBuilder::success(drivers)
}

/// 加载动态驱动
async fn dynamic_load_driver(
    axum::extract::State(_state): axum::extract::State<AppState>,
    _claims: crate::shared::security::jwt::Claims,
    Json(req): Json<LoadDriverRequest>,
) -> Json<ApiResponse<String>> {
    use crate::modules::device::driver;
    tracing::info!("Loading dynamic driver from: {}", req.path);

    match driver::load_dynamic_driver(&req.path) {
        Ok(driver_name) => {
            tracing::info!("Successfully loaded dynamic driver: {}", driver_name);
            ApiResponseBuilder::success(driver_name)
        }
        Err(e) => {
            tracing::error!("Failed to load dynamic driver: {}", e);
            ApiResponseBuilder::error(format!("Failed to load driver: {}", e))
        }
    }
}

/// 卸载动态驱动
async fn dynamic_unload_driver(
    axum::extract::State(_state): axum::extract::State<AppState>,
    _claims: crate::shared::security::jwt::Claims,
    Path(name): Path<String>,
) -> Json<ApiResponse<bool>> {
    use crate::modules::device::driver;
    tracing::info!("Unloading dynamic driver: {}", name);

    match driver::unload_dynamic_driver(&name) {
        Ok(_) => {
            tracing::info!("Successfully unloaded dynamic driver: {}", name);
            ApiResponseBuilder::success(true)
        }
        Err(e) => {
            tracing::error!("Failed to unload dynamic driver: {}", e);
            ApiResponseBuilder::error(format!("Failed to unload driver: {}", e))
        }
    }
}

/// 获取所有驱动列表（包括静态和动态）
async fn dynamic_list_all_drivers(
    axum::extract::State(_state): axum::extract::State<AppState>,
    _claims: crate::shared::security::jwt::Claims,
) -> Json<ApiResponse<AllDriversResponse>> {
    use crate::modules::device::driver;
    let all_names = driver::get_all_driver_names();
    let registry = driver::dynamic::registry::get_global_registry();

    let mut static_drivers = Vec::new();
    let mut dynamic_drivers = Vec::new();

    for name in all_names {
        let is_dynamic = registry.has_driver(&name);
        let driver_info = DriverInfo {
            name: name.clone(),
            version: Some("1.0.0".to_string()),
            description: Some(format!("{} driver", name)),
            is_loaded: true,
            path: if is_dynamic { registry.get_driver_path(&name) } else { None },
            category: Some("protocol".to_string()),
            tags: Some(vec!["industrial".to_string()]),
        };

        if is_dynamic {
            dynamic_drivers.push(driver_info);
        } else {
            static_drivers.push(driver_info);
        }
    }

    ApiResponseBuilder::success(AllDriversResponse { static_drivers, dynamic: dynamic_drivers })
}

/// 重新加载驱动目录
async fn dynamic_reload_drivers_dir(
    axum::extract::State(_state): axum::extract::State<AppState>,
    _claims: crate::shared::security::jwt::Claims,
) -> Json<ApiResponse<Vec<String>>> {
    use crate::modules::device::driver;
    let config = crate::shared::config::get();
    let drivers_dir = &config.device.drivers.dynamic_drivers_dir;

    tracing::info!("Reloading drivers from: {}", drivers_dir);

    match driver::dynamic::auto_load_drivers(drivers_dir) {
        Ok(loaded) => {
            tracing::info!("Reloaded {} driver(s)", loaded.len());
            ApiResponseBuilder::success(loaded)
        }
        Err(e) => {
            tracing::error!("Failed to reload drivers: {}", e);
            ApiResponseBuilder::error(format!("Failed to reload drivers: {}", e))
        }
    }
}

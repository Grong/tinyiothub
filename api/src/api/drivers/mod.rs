use axum::{
    extract::{Path, Query},
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    api_error, api_success,
    domain::device::driver::get_driver_list,
    dto::entity::component::{Component, ComponentOption},
    dto::response::ApiResponse,
    shared::app_state::AppState,
};

pub mod dynamic;

/// 驱动列表响应
#[derive(Serialize, Deserialize)]
pub struct DriverListResponse {
    /// 驱动列表
    pub drivers: Vec<Component>,
    /// 总数量
    pub total: usize,
}

/// 驱动详情响应
#[derive(Serialize, Deserialize)]
pub struct DriverDetailResponse {
    /// 驱动信息
    pub driver: Component,
}

/// 驱动配置参数响应
#[derive(Serialize, Deserialize)]
pub struct DriverConfigResponse {
    /// 驱动名称
    pub driver_name: String,
    /// 配置参数列表
    pub config_options: Vec<ComponentOption>,
    /// 默认配置值
    pub default_config: HashMap<String, String>,
}

/// 创建驱动 API 路由
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_drivers))
        .route("/names", get(list_driver_names))
        .route("/:name", get(get_driver_detail))
        .route("/:name/config", get(get_driver_config))
        .route("/:name/supported", get(check_driver_support))
        // 动态驱动管理
        .route("/dynamic/load", post(dynamic::load_driver))
        .route("/dynamic/:name/unload", delete(dynamic::unload_driver))
        .route("/dynamic/list", get(dynamic::list_all_drivers))
        .route("/dynamic/reload", post(dynamic::reload_drivers_dir))
}

/// 获取驱动列表的处理函数
async fn list_drivers(
    Query(params): Query<HashMap<String, String>>,
) -> Json<ApiResponse<DriverListResponse>> {
    tracing::info!("Getting driver list, params: {:?}", params);

    // 获取静态驱动列表
    let mut drivers = get_driver_list();

    // 获取动态驱动列表
    let registry = crate::domain::device::driver::dynamic::registry::get_global_registry();
    for driver_name in registry.get_driver_names() {
        if let Ok(driver_info) = registry.get_dynamic_driver_info(&driver_name) {
            drivers.push(driver_info);
        }
    }

    // 如果提供了名称过滤器，进行过滤
    if let Some(filter_name) = params.get("name") {
        drivers.retain(|driver| {
            driver
                .name
                .to_lowercase()
                .contains(&filter_name.to_lowercase())
        });
    }

    let total = drivers.len();

    tracing::info!("Found {} drivers (static + dynamic)", total);

    api_success!(DriverListResponse { drivers, total })
}

/// 获取驱动详情的处理函数
async fn get_driver_detail(Path(name): Path<String>) -> Json<ApiResponse<DriverDetailResponse>> {
    tracing::info!("Getting driver details for: {}", name);

    // 先从静态驱动查找
    let drivers = get_driver_list();
    if let Some(driver) = drivers.into_iter().find(|d| d.name == name) {
        tracing::info!("Found static driver: {}", driver.name);
        return api_success!(DriverDetailResponse { driver });
    }

    // 再从动态驱动查找
    let registry = crate::domain::device::driver::dynamic::registry::get_global_registry();
    if let Ok(driver) = registry.get_dynamic_driver_info(&name) {
        tracing::info!("Found dynamic driver: {}", driver.name);
        return api_success!(DriverDetailResponse { driver });
    }

    tracing::warn!("Driver not found: {}", name);
    api_error!(format!("Driver '{}' not found", name))
}

/// 检查驱动支持状态的处理函数
async fn check_driver_support(Path(name): Path<String>) -> Json<ApiResponse<DriverListResponse>> {
    tracing::info!("Checking if driver is supported: {}", name);

    // 检查静态和动态驱动
    let is_supported = crate::domain::device::driver::has_driver(&name);

    let response = if is_supported {
        DriverListResponse {
            drivers: vec![],
            total: 1,
        }
    } else {
        DriverListResponse {
            drivers: vec![],
            total: 0,
        }
    };

    tracing::info!("Driver {} support status: {}", name, is_supported);

    api_success!(response)
}

/// 获取驱动配置参数的处理函数
async fn get_driver_config(Path(name): Path<String>) -> Json<ApiResponse<DriverConfigResponse>> {
    tracing::info!("Getting driver config for: {}", name);

    let drivers = get_driver_list();

    if let Some(driver) = drivers.into_iter().find(|d| d.name == name) {
        // 解析配置选项
        let config_options: Vec<ComponentOption> =
            if let Ok(options) = serde_json::from_str(&driver.options_descriptors) {
                options
            } else {
                vec![]
            };

        // 生成默认配置
        let mut default_config = HashMap::new();
        for option in &config_options {
            default_config.insert(option.name.clone(), option.default_value.clone());
        }

        tracing::info!(
            "Found {} config options for driver: {}",
            config_options.len(),
            driver.name
        );

        api_success!(DriverConfigResponse {
            driver_name: driver.name,
            config_options,
            default_config,
        })
    } else {
        tracing::warn!("Driver not found: {}", name);
        api_error!(format!("Driver '{}' not found", name))
    }
}

/// 获取支持的驱动名称列表的处理函数
async fn list_driver_names() -> Json<ApiResponse<DriverListResponse>> {
    tracing::info!("Getting supported driver names");

    // 获取所有驱动名称（静态+动态）
    let driver_names = crate::domain::device::driver::get_all_driver_names();

    // 将驱动名称转换为简化的 Component 结构
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

    let total = drivers.len();

    tracing::info!("Found {} supported driver names (static + dynamic)", total);

    api_success!(DriverListResponse { drivers, total })
}

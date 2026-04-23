//! 动态驱动管理API（内部管理用）

use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::device::driver,
    dto::response::{ApiResponse},
    shared::{app_state::AppState},
};

/// 加载动态驱动请求
#[derive(Debug, Deserialize)]
pub struct LoadDriverRequest {
    /// 驱动文件路径
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

/// 加载动态驱动
pub async fn load_driver(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(req): Json<LoadDriverRequest>,
) -> Json<ApiResponse<String>> {
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
pub async fn unload_driver(
    State(_state): State<AppState>,
    _claims: Claims,
    Path(name): Path<String>,
) -> Json<ApiResponse<bool>> {
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
pub async fn list_all_drivers(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AllDriversResponse>> {
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
pub async fn reload_drivers_dir(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<String>>> {
    let config = crate::infrastructure::config::get();
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

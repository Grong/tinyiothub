use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        marketplace::{
            client::MarketplaceClient, driver_installer::DriverInstaller, metadata::*,
            template_installer::TemplateInstaller,
        },
        template::repository::TemplateRepository,
    },
    dto::response::{builder::ApiResponseBuilder, ApiResponse},
    infrastructure::config,
    shared::{app_state::AppState, security::jwt::Claims},
};

pub fn create_router() -> Router<AppState> {
    Router::new()
        // 模板市场
        .route("/templates", get(list_marketplace_templates))
        .route("/templates/:id", get(get_marketplace_template))
        .route("/templates/:id/install", post(install_marketplace_template))
        // 驱动市场
        .route("/drivers", get(list_marketplace_drivers))
        .route("/drivers/:id", get(get_marketplace_driver))
        .route("/drivers/:id/install", post(install_marketplace_driver))
}

/// 安装请求
#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    pub version: Option<String>,
}

/// 获取市场模板列表
async fn list_marketplace_templates(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<TemplateMetadata>>> {
    let config = config::get();

    let client = match MarketplaceClient::new(config.marketplace.clone()) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(&format!("市场客户端初始化失败: {}", e));
        }
    };

    match client.fetch_templates().await {
        Ok(templates) => ApiResponseBuilder::success(templates),
        Err(e) => {
            tracing::error!("Failed to fetch marketplace templates: {}", e);
            ApiResponseBuilder::error(&format!("获取市场模板失败: {}", e))
        }
    }
}

/// 获取市场模板详情
async fn get_marketplace_template(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<TemplateMetadata>>> {
    let config = config::get();

    let client = match MarketplaceClient::new(config.marketplace.clone()) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(&format!("市场客户端初始化失败: {}", e));
        }
    };

    match client.fetch_templates().await {
        Ok(templates) => {
            let template = templates.into_iter().find(|t| t.id == id);
            ApiResponseBuilder::success(template)
        }
        Err(e) => {
            tracing::error!("Failed to fetch marketplace template {}: {}", id, e);
            ApiResponseBuilder::error(&format!("获取模板详情失败: {}", e))
        }
    }
}

/// 从市场安装模板
async fn install_marketplace_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(req): Json<InstallRequest>,
) -> Json<ApiResponse<String>> {
    let config = config::get();

    let client = match MarketplaceClient::new(config.marketplace.clone()) {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(&format!("市场客户端初始化失败: {}", e));
        }
    };

    let repository = Arc::new(TemplateRepository::new(
        state.database.clone(),
        std::path::PathBuf::from("templates"),
    ));

    let installer =
        TemplateInstaller::new(client, repository, std::path::PathBuf::from("templates"));

    match installer.install_from_marketplace(&id, req.version.as_deref()).await {
        Ok(template_id) => {
            tracing::info!("Successfully installed template: {}", template_id);
            ApiResponseBuilder::success(template_id)
        }
        Err(e) => {
            tracing::error!("Failed to install template {}: {}", id, e);
            ApiResponseBuilder::error(&format!("安装模板失败: {}", e))
        }
    }
}

/// 获取市场驱动列表
async fn list_marketplace_drivers(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<DriverMetadata>>> {
    let config = config::get();

    let client = match MarketplaceClient::new(config.marketplace.clone()) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(&format!("市场客户端初始化失败: {}", e));
        }
    };

    match client.fetch_drivers().await {
        Ok(drivers) => ApiResponseBuilder::success(drivers),
        Err(e) => {
            tracing::error!("Failed to fetch marketplace drivers: {}", e);
            ApiResponseBuilder::error(&format!("获取市场驱动失败: {}", e))
        }
    }
}

/// 获取市场驱动详情
async fn get_marketplace_driver(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DriverMetadata>>> {
    let config = config::get();

    let client = match MarketplaceClient::new(config.marketplace.clone()) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(&format!("市场客户端初始化失败: {}", e));
        }
    };

    match client.fetch_drivers().await {
        Ok(drivers) => {
            let driver = drivers.into_iter().find(|d| d.id == id);
            ApiResponseBuilder::success(driver)
        }
        Err(e) => {
            tracing::error!("Failed to fetch marketplace driver {}: {}", id, e);
            ApiResponseBuilder::error(&format!("获取驱动详情失败: {}", e))
        }
    }
}

/// 从市场安装驱动
async fn install_marketplace_driver(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(req): Json<InstallRequest>,
) -> Json<ApiResponse<String>> {
    let config = config::get();

    let client = match MarketplaceClient::new(config.marketplace.clone()) {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(&format!("市场客户端初始化失败: {}", e));
        }
    };

    let installer = DriverInstaller::new(
        client,
        std::path::PathBuf::from(&config.device.drivers.dynamic_drivers_dir),
    );

    match installer.install_from_marketplace(&id, req.version.as_deref()).await {
        Ok(driver_name) => {
            tracing::info!("Successfully installed driver: {}", driver_name);
            ApiResponseBuilder::success(driver_name)
        }
        Err(e) => {
            tracing::error!("Failed to install driver {}: {}", id, e);
            ApiResponseBuilder::error(&format!("安装驱动失败: {}", e))
        }
    }
}

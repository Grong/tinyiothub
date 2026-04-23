// Marketplace API — moved from api/marketplace/mod.rs

use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::Deserialize;

use crate::{
    modules::marketplace::{
        client::MarketplaceClient, driver_installer::DriverInstaller,
        template_installer::TemplateInstaller,
    },
    modules::template::TemplateRepository,
    shared::api_response::ApiResponse,
    shared::config,
    shared::app_state::AppState,
};

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/templates", get(proxy_marketplace_templates))
        .route("/templates/{id}", get(proxy_marketplace_template))
        .route("/templates/{id}/install", post(install_marketplace_template))
        .route("/drivers", get(proxy_marketplace_drivers))
        .route("/drivers/{id}", get(proxy_marketplace_driver))
        .route("/drivers/{id}/install", post(install_marketplace_driver))
}

const EXTERNAL_MARKETPLACE_API: &str = "https://marketplace.tinyiothub.com/api/v1";

static HTTP_CLIENT: std::sync::LazyLock<Client, fn() -> Client> =
    std::sync::LazyLock::new(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client")
    });

#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    pub version: Option<String>,
}

async fn proxy_marketplace_templates(
    State(_state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let mut url = format!("{}/templates", EXTERNAL_MARKETPLACE_API);

    if !params.is_empty() {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        url = format!("{}?{}", url, query_string);
    }

    tracing::info!("Proxying marketplace templates request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => Json(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                Json(serde_json::json!({
                    "code": -1,
                    "msg": format!("解析市场响应失败: {}", e),
                    "result": null
                }))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace templates: {}", e);
            Json(serde_json::json!({
                "code": -1,
                "msg": format!("获取市场模板失败: {}", e),
                "result": null
            }))
        }
    }
}

async fn proxy_marketplace_template(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let url = format!("{}/templates/{}", EXTERNAL_MARKETPLACE_API, id);
    tracing::info!("Proxying marketplace template request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => Json(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                Json(serde_json::json!({
                    "code": -1,
                    "msg": format!("解析市场响应失败: {}", e),
                    "result": null
                }))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace template {}: {}", id, e);
            Json(serde_json::json!({
                "code": -1,
                "msg": format!("获取模板详情失败: {}", e),
                "result": null
            }))
        }
    }
}

async fn proxy_marketplace_drivers(
    State(_state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let mut url = format!("{}/drivers", EXTERNAL_MARKETPLACE_API);

    if !params.is_empty() {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        url = format!("{}?{}", url, query_string);
    }

    tracing::info!("Proxying marketplace drivers request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => Json(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                Json(serde_json::json!({
                    "code": -1,
                    "msg": format!("解析市场响应失败: {}", e),
                    "result": null
                }))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace drivers: {}", e);
            Json(serde_json::json!({
                "code": -1,
                "msg": format!("获取市场驱动失败: {}", e),
                "result": null
            }))
        }
    }
}

async fn proxy_marketplace_driver(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let url = format!("{}/drivers/{}", EXTERNAL_MARKETPLACE_API, id);
    tracing::info!("Proxying marketplace driver request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => Json(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                Json(serde_json::json!({
                    "code": -1,
                    "msg": format!("解析市场响应失败: {}", e),
                    "result": null
                }))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace driver {}: {}", id, e);
            Json(serde_json::json!({
                "code": -1,
                "msg": format!("获取驱动详情失败: {}", e),
                "result": null
            }))
        }
    }
}

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
            return ApiResponseBuilder::error(format!("市场客户端初始化失败: {}", e));
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
            ApiResponseBuilder::error(format!("安装模板失败: {}", e))
        }
    }
}

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
            return ApiResponseBuilder::error(format!("市场客户端初始化失败: {}", e));
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
            ApiResponseBuilder::error(format!("安装驱动失败: {}", e))
        }
    }
}

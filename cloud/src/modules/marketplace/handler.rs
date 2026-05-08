// Marketplace API — moved from api/marketplace/mod.rs

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use reqwest::Client;
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    api::middleware::WorkspaceScope,
    modules::{
        marketplace::{
            client::MarketplaceClient, driver_installer::DriverInstaller,
            template_installer::TemplateInstaller,
        },
        template::TemplateRepository,
    },
    shared::{
        api_response::ApiResponse, app_state::AppState, config, error_handling::AuthHelper,
        security::jwt::Claims,
    },
};

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/templates", get(proxy_marketplace_templates))
        .route("/templates/{id}", get(proxy_marketplace_template))
        .route("/templates/{id}/install", post(install_marketplace_template))
        .route("/drivers", get(proxy_marketplace_drivers))
        .route("/drivers/{id}", get(proxy_marketplace_driver))
        .route("/drivers/{id}/install", post(install_marketplace_driver))
        .route("/publish/template", post(publish_template_handler))
}

const EXTERNAL_MARKETPLACE_API: &str = "https://marketplace.tinyiothub.com/api/v1";

static HTTP_CLIENT: std::sync::LazyLock<Client, fn() -> Client> = std::sync::LazyLock::new(|| {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});

#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    pub version: Option<String>,
}

/// 将外部市场 API 的响应统一包装为 ApiResponse 格式。
/// 如果外部响应已经是 ApiResponse 格式（包含 code + result），则直接透传。
fn normalize_marketplace_response(data: serde_json::Value) -> Json<ApiResponse<serde_json::Value>> {
    if data.get("code").is_some() && data.get("result").is_some() {
        let code = data["code"].as_i64().unwrap_or(0) as i32;
        let msg = data["msg"].as_str().unwrap_or("").to_string();
        let result = data.get("result").cloned();
        Json(ApiResponse { code, msg, result })
    } else {
        ApiResponseBuilder::success(data)
    }
}

async fn proxy_marketplace_templates(
    State(_state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let mut url = format!("{}/templates", EXTERNAL_MARKETPLACE_API);

    if !params.is_empty() {
        let query_string =
            params.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&");
        url = format!("{}?{}", url, query_string);
    }

    tracing::info!("Proxying marketplace templates request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => normalize_marketplace_response(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                ApiResponseBuilder::error(format!("解析市场响应失败: {}", e))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace templates: {}", e);
            ApiResponseBuilder::error(format!("获取市场模板失败: {}", e))
        }
    }
}

async fn proxy_marketplace_template(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let url = format!("{}/templates/{}", EXTERNAL_MARKETPLACE_API, id);
    tracing::info!("Proxying marketplace template request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => normalize_marketplace_response(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                ApiResponseBuilder::error(format!("解析市场响应失败: {}", e))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace template {}: {}", id, e);
            ApiResponseBuilder::error(format!("获取模板详情失败: {}", e))
        }
    }
}

async fn proxy_marketplace_drivers(
    State(_state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let mut url = format!("{}/drivers", EXTERNAL_MARKETPLACE_API);

    if !params.is_empty() {
        let query_string =
            params.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&");
        url = format!("{}?{}", url, query_string);
    }

    tracing::info!("Proxying marketplace drivers request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => normalize_marketplace_response(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                ApiResponseBuilder::error(format!("解析市场响应失败: {}", e))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace drivers: {}", e);
            ApiResponseBuilder::error(format!("获取市场驱动失败: {}", e))
        }
    }
}

async fn proxy_marketplace_driver(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let url = format!("{}/drivers/{}", EXTERNAL_MARKETPLACE_API, id);
    tracing::info!("Proxying marketplace driver request to: {}", url);

    match HTTP_CLIENT.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(data) => normalize_marketplace_response(data),
            Err(e) => {
                tracing::error!("Failed to parse marketplace response: {}", e);
                ApiResponseBuilder::error(format!("解析市场响应失败: {}", e))
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch marketplace driver {}: {}", id, e);
            ApiResponseBuilder::error(format!("获取驱动详情失败: {}", e))
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
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    Json(req): Json<InstallRequest>,
) -> Json<ApiResponse<String>> {
    match AuthHelper::check_role(&state, &claims.user_id, "admin").await {
        Ok(true) => {}
        Ok(false) => return ApiResponseBuilder::error("需要管理员权限"),
        Err(e) => {
            tracing::warn!("权限检查失败: {}", e);
            return ApiResponseBuilder::error("权限检查失败");
        }
    }

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

#[derive(serde::Deserialize)]
pub struct PublishTemplateApiRequest {
    pub template_id: String,
}

async fn publish_template_handler(
    State(state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
    claims: Claims,
    Json(req): Json<PublishTemplateApiRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    match AuthHelper::check_role(&state, &claims.user_id, "admin").await {
        Ok(true) => {}
        Ok(false) => return ApiResponseBuilder::error("需要管理员权限"),
        Err(e) => {
            tracing::warn!("权限检查失败: {}", e);
            return ApiResponseBuilder::error("权限检查失败");
        }
    }
    let config = crate::shared::config::get();
    let marketplace_config = &config.marketplace;
    if !marketplace_config.enabled {
        return ApiResponseBuilder::error("市场未启用");
    }
    if marketplace_config.api_url.is_none() || marketplace_config.api_key.is_none() {
        return ApiResponseBuilder::error("市场未配置");
    }

    let workspace_id_str = workspace_id.as_deref().unwrap_or("");
    let template = match crate::modules::template::types::DeviceTemplate::find_by_id(
        &state.database,
        &req.template_id,
        workspace_id_str,
    )
    .await
    {
        Ok(Some(t)) => t,
        Ok(None) => {
            return ApiResponseBuilder::error("模板不存在");
        }
        Err(e) => {
            return ApiResponseBuilder::error(format!("数据库错误: {}", e));
        }
    };

    let publisher =
        match crate::modules::marketplace::MarketplacePublisher::new(&marketplace_config) {
            Ok(p) => p,
            Err(e) => {
                return ApiResponseBuilder::error(format!("发布器初始化失败: {}", e));
            }
        };

    match publisher.publish_template(&template).await {
        Ok(result) => ApiResponseBuilder::success(result),
        Err(e) => ApiResponseBuilder::error(format!("发布失败: {}", e)),
    }
}

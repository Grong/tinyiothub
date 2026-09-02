// Marketplace API — moved from api/marketplace/mod.rs

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use reqwest::Client;
use serde::Deserialize;
use tinyiothub_storage::scene_template::{SceneTemplateFile, ThingNodeDef};
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::{
    api::middleware::WorkspaceScope,
    domains::marketplace::{
        client::MarketplaceClient,
        driver_installer::DriverInstaller,
        error::MarketplaceError,
        scene_instantiator::{InstantiateParams, SceneInstantiator},
        template_installer::TemplateInstaller,
        thing_template_installer::ThingTemplateInstaller,
    },
    shared::{api_response::ApiResponse, error_handling::AuthHelper},
    state::AppState,
};

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/templates", get(proxy_marketplace_templates))
        .route("/templates/{name}", get(proxy_marketplace_template))
        .route("/templates/{name}/install", post(install_marketplace_template))
        .route("/drivers", get(proxy_marketplace_drivers))
        .route("/drivers/{id}", get(proxy_marketplace_driver))
        .route("/drivers/{id}/install", post(install_marketplace_driver))
        .route("/publish/template", post(publish_template_handler))
        .route("/thing-templates", get(list_thing_templates))
        .route("/thing-templates/{id}", get(get_thing_template_detail))
        .route("/thing-templates/{id}/install", post(install_thing_template))
        .route("/thing-templates/{id}/instantiate", post(instantiate_thing_template))
}

fn marketplace_api_url(state: &AppState) -> String {
    state
        .marketplace
        .api_url
        .clone()
        .unwrap_or_else(|| "https://marketplace.tinyiothub.com/api/v1".to_string())
}

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
/// - 如果外部响应已经是 ApiResponse 格式（包含 code + result），则直接透传，
///   同时把 result 内部的 `items` 重命名为 `data`，并把分页元数据规范化为
///   PaginatedResponse 格式（对齐项目规范）。
/// - 否则将原始数据包装为 ApiResponse::success。
fn normalize_marketplace_response(data: serde_json::Value) -> Json<ApiResponse<serde_json::Value>> {
    if data.get("code").is_some() && data.get("result").is_some() {
        let code = data["code"].as_i64().unwrap_or(0) as i32;
        let msg = data["msg"].as_str().unwrap_or("").to_string();
        let mut result = data.get("result").cloned();
        if let Some(ref mut obj) = result {
            // 外部市场使用 `items`，内部规范使用 `data`
            if obj.get("items").is_some()
                && obj.get("data").is_none()
                && let Some(items) = obj.as_object_mut().and_then(|m| m.remove("items"))
            {
                obj["data"] = items;
            }
            // 本地市场 JSON 使用 `templates` → 重命名为 `data`
            if obj.get("templates").is_some()
                && obj.get("data").is_none()
                && let Some(templates) = obj.as_object_mut().and_then(|m| m.remove("templates"))
            {
                obj["data"] = templates;
            }
            // 本地市场 JSON 使用 `drivers` → 重命名为 `data`
            if obj.get("drivers").is_some()
                && obj.get("data").is_none()
                && let Some(drivers) = obj.as_object_mut().and_then(|m| m.remove("drivers"))
            {
                obj["data"] = drivers;
            }
            // 规范化分页元数据为 PaginatedResponse 格式
            if obj.get("data").is_some() && obj.get("pagination").is_none() {
                let data_arr = obj["data"].as_array();
                let page = obj
                    .get("page")
                    .and_then(|v| v.as_u64())
                    .or_else(|| obj.get("current_page").and_then(|v| v.as_u64()))
                    .unwrap_or(1) as u32;
                let page_size = obj
                    .get("page_size")
                    .and_then(|v| v.as_u64())
                    .or_else(|| obj.get("pageSize").and_then(|v| v.as_u64()))
                    .or_else(|| obj.get("per_page").and_then(|v| v.as_u64()))
                    .unwrap_or(20) as u32;
                let total_count = obj
                    .get("total_count")
                    .and_then(|v| v.as_u64())
                    .or_else(|| obj.get("totalCount").and_then(|v| v.as_u64()))
                    .or_else(|| obj.get("total").and_then(|v| v.as_u64()))
                    .or_else(|| data_arr.map(|a| a.len() as u64))
                    .unwrap_or(0);
                let total_pages = if page_size > 0 {
                    ((total_count as f64) / (page_size as f64)).ceil() as u32
                } else {
                    0
                };
                obj["pagination"] = serde_json::json!({
                    "page": page,
                    "page_size": page_size,
                    "total_pages": total_pages,
                    "total_count": total_count
                });
            }
        }
        Json(ApiResponse { code, msg, result })
    } else {
        ApiResponseBuilder::success(data)
    }
}

async fn proxy_marketplace_templates(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let mut url = format!("{}/templates", marketplace_api_url(&state));

    if !params.is_empty() {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
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
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let url = format!("{}/templates/{}", marketplace_api_url(&state), name);
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
            tracing::error!("Failed to fetch marketplace template {}: {}", name, e);
            ApiResponseBuilder::error(format!("获取模板详情失败: {}", e))
        }
    }
}

async fn proxy_marketplace_drivers(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let mut url = format!("{}/drivers", marketplace_api_url(&state));

    if !params.is_empty() {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
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
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let url = format!("{}/drivers/{}", marketplace_api_url(&state), id);
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
    Path(name): Path<String>,
    _claims: Claims,
    Json(req): Json<InstallRequest>,
) -> Json<ApiResponse<String>> {
    let client = match MarketplaceClient::new(state.marketplace.clone()) {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(format!("市场客户端初始化失败: {}", e));
        }
    };

    let db = state.db.clone();

    let installer = TemplateInstaller::new(client, db);

    match installer.install_from_marketplace(&name, req.version.as_deref()).await {
        Ok(template_id) => {
            tracing::info!("Successfully installed template: {}", template_id);
            ApiResponseBuilder::success(template_id)
        }
        Err(e) => {
            tracing::error!("Failed to install template {}: {}", name, e);
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

    let client = match MarketplaceClient::new(state.marketplace.clone()) {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::error!("Failed to create marketplace client: {}", e);
            return ApiResponseBuilder::error(format!("市场客户端初始化失败: {}", e));
        }
    };

    let installer = DriverInstaller::new(client, std::path::PathBuf::from(&state.dynamic_drivers_dir));

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
    let marketplace_config = &state.marketplace;
    if !marketplace_config.enabled {
        return ApiResponseBuilder::error("市场未启用");
    }
    if marketplace_config.api_url.is_none() || marketplace_config.api_key.is_none() {
        return ApiResponseBuilder::error("市场未配置");
    }

    let workspace_id_str = workspace_id.as_deref().unwrap_or("");
    let template = match state
        .db
        .find_thing_template_by_id(&req.template_id, workspace_id_str)
        .await
    {
        Ok(Some(t)) => {
            if t.is_builtin != 0 {
                return ApiResponseBuilder::error("内置模板不能发布到市场");
            }
            t
        }
        Ok(None) => {
            return ApiResponseBuilder::error("模板不存在");
        }
        Err(e) => {
            return ApiResponseBuilder::error(format!("数据库错误: {}", e));
        }
    };

    let publisher = match crate::domains::marketplace::MarketplacePublisher::new(marketplace_config) {
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

// ──────────────────────────────────────────────────────────────────
// Thing Template Marketplace (local DB, not proxy)
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListThingTemplatesQuery {
    /// true 只返回场景包（组合模板），false 只返回 entity 模板；缺省返回全部。
    pub composition: Option<bool>,
}

/// List thing_templates as local marketplace items.
/// Shows especially built-in templates (workspace_id IS NULL).
async fn list_thing_templates(
    State(state): State<AppState>,
    Query(query): Query<ListThingTemplatesQuery>,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = workspace_id.as_deref().unwrap_or("");

    match ThingTemplateInstaller::list(state.db.as_ref(), ws).await {
        Ok(mut items) => {
            // v1 模板数量少，应用层过滤后分页
            if let Some(want) = query.composition {
                items.retain(|i| i.is_composition == want);
            }
            let total = items.len() as u64;
            let result = serde_json::json!({
                "data": items,
                "pagination": {
                    "page": 1u32,
                    "page_size": total as u32,
                    "total_pages": 1u32,
                    "total_count": total,
                }
            });
            ApiResponseBuilder::success(result)
        }
        Err(e) => {
            tracing::error!("Failed to list thing_templates: {}", e);
            ApiResponseBuilder::error(format!("获取物模板列表失败: {}", e))
        }
    }
}

/// Install (copy) a thing_template into the caller's workspace.
/// Handles name conflict by appending " (来自市场)" suffix.
async fn install_thing_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = workspace_id.as_deref().unwrap_or("");
    if ws.is_empty() {
        return ApiResponseBuilder::error("需要指定工作空间");
    }

    match ThingTemplateInstaller::install(state.db.as_ref(), &id, ws).await {
        Ok(installed) => {
            tracing::info!(
                "Installed thing_template {} as '{}' (id={})",
                id,
                installed.name,
                installed.id
            );
            ApiResponseBuilder::success(serde_json::json!({
                "id": installed.id,
                "name": installed.name,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to install thing_template {}: {}", id, e);
            ApiResponseBuilder::error(format!("安装物模板失败: {}", e))
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Scene pack (composition template) detail + instantiate
// ──────────────────────────────────────────────────────────────────

/// MarketplaceError → HTTP 状态码 + ApiResponse。
/// Validation/InvalidConfig → 400，NotFound → 404，其余 → 500。
fn marketplace_error_response(e: &MarketplaceError) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let status = match e {
        MarketplaceError::Validation(_) | MarketplaceError::InvalidConfig(_) => StatusCode::BAD_REQUEST,
        MarketplaceError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        ApiResponseBuilder::error_with_code(status.as_u16() as i32, e.to_string()),
    )
}

/// 组合模板静态深度：根=1，children 递归最深；template_ref/scene_ref 计 1 层不深入。
fn scene_max_depth(nodes: &[ThingNodeDef], depth: usize) -> usize {
    nodes
        .iter()
        .map(|n| scene_max_depth(&n.children, depth + 1))
        .max()
        .unwrap_or(depth)
}

fn json_array_len(s: &str) -> usize {
    serde_json::from_str::<Vec<serde_json::Value>>(s)
        .map(|v| v.len())
        .unwrap_or(0)
}

/// 物模板详情：组合模板附带 parameters 与 structureSummary。
async fn get_thing_template_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let ws = workspace_id.as_deref().unwrap_or("");

    let template = match state.db.find_thing_template_by_id(&id, ws).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return marketplace_error_response(&MarketplaceError::NotFound(format!("模板不存在: {id}")));
        }
        Err(e) => {
            tracing::error!("Failed to load thing_template {}: {}", id, e);
            return marketplace_error_response(&MarketplaceError::Template(e.to_string()));
        }
    };

    let is_composition = template.is_composition();
    let (parameters, structure_summary) = if is_composition {
        match SceneTemplateFile::from_json(&template.device_info) {
            Ok(scene) => {
                let max_depth = scene_max_depth(&scene.children, 1);
                (
                    serde_json::to_value(&scene.parameters).unwrap_or_else(|_| serde_json::json!([])),
                    serde_json::json!({
                        "parameterCount": scene.parameters.len(),
                        "maxDepth": max_depth,
                    }),
                )
            }
            Err(e) => {
                tracing::error!("Failed to parse scene template {}: {}", id, e);
                return marketplace_error_response(&MarketplaceError::Template(format!("场景包解析失败: {e}")));
            }
        }
    } else {
        (
            serde_json::json!([]),
            serde_json::json!({"parameterCount": 0, "maxDepth": 1}),
        )
    };

    let result = serde_json::json!({
        "id": template.id,
        "name": template.name,
        "description": template.description,
        "category": template.category,
        "isBuiltin": template.is_builtin != 0,
        "isComposition": is_composition,
        "propertyCount": json_array_len(&template.properties),
        "actionCount": json_array_len(&template.actions),
        "eventCount": json_array_len(&template.events),
        "createdAt": template.created_at,
        "parameters": parameters,
        "structureSummary": structure_summary,
    });
    (StatusCode::OK, ApiResponseBuilder::success(result))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstantiateRequestBody {
    pub scene_name: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub parameter_values: std::collections::HashMap<String, i64>,
    #[serde(default)]
    pub dry_run: bool,
}

/// 实例化场景包：展开 → 配额校验 → 单事务落库；dry_run 只读预览。
async fn instantiate_thing_template(
    State(state): State<AppState>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
    Json(body): Json<InstantiateRequestBody>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let ws = workspace_id.as_deref().unwrap_or("");
    if ws.is_empty() {
        return marketplace_error_response(&MarketplaceError::Validation("需要指定工作空间".to_string()));
    }

    let params = InstantiateParams {
        scene_name: body.scene_name,
        parent_id: body.parent_id,
        parameter_values: body.parameter_values,
        dry_run: body.dry_run,
    };
    match SceneInstantiator::instantiate(state.db.as_ref(), ws, &id, &params).await {
        Ok(outcome) => {
            let result = serde_json::to_value(&outcome).unwrap_or(serde_json::Value::Null);
            (StatusCode::OK, ApiResponseBuilder::success(result))
        }
        Err(e) => {
            tracing::error!("Failed to instantiate thing_template {}: {}", id, e);
            marketplace_error_response(&e)
        }
    }
}

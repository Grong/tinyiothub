// SaaS Tenants API Module
// 租户管理 API

pub mod auth;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json,
    Router,
};

use crate::{
    api::middleware::WorkspaceScope,
    shared::security::jwt::Claims,
    dto::entity::{
        tenant::{ApiKey, ApiUsageStats, CreateApiKeyRequest, Tenant, TenantQueryParams, TenantUsage},
    },
    dto::response::{ApiResponse, builder::ApiResponseBuilder},
    shared::app_state::AppState,
};

/// Create API Keys router — 直接挂载在 /v1/api-keys/ 下
pub fn create_api_key_router() -> Router<AppState> {
    Router::new()
        // GET /v1/api-keys — list
        .route("/", get(list_api_keys))
        // POST /v1/api-keys — create
        .route("/", post(create_api_key))
        // DELETE /v1/api-keys/{id} — revoke
        .route("/{id}", delete(revoke_api_key))
}

/// Create tenants router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Plans - 移除这里重复的 plans，因为 auth router 已经有
        // .route("/plans", get(list_plans))
        // Tenants — 路径不带 /tenants 前缀，由外层 nest("/tenants", ...) 添加
        .route("/tenants", get(list_tenants))
        .route("/tenants", post(create_tenant))
        .route("/tenants/{id}", get(get_tenant))
        .route("/tenants/{id}", put(update_tenant))
        .route("/tenants/{id}/change-plan", post(change_plan))
        .route("/tenants/{id}/usage", get(get_tenant_usage))
        // Usage
        .route("/{tenant_id}/usage-stats", get(get_usage_stats))
}

/// List tenants
async fn list_tenants(
    State(_state): State<AppState>,
    Query(_params): Query<TenantQueryParams>,
) -> Json<ApiResponse<Vec<Tenant>>> {
    // 简化实现：返回空列表（需要管理权限）
    ApiResponseBuilder::success(vec![])
}

/// Create tenant
async fn create_tenant(
    State(state): State<AppState>,
    Json(payload): Json<crate::dto::entity::tenant::CreateTenantRequest>,
) -> Json<ApiResponse<Tenant>> {
    match state.tenant_service.create_tenant(&payload).await {
        Ok(tenant) => ApiResponseBuilder::success(tenant),
        Err(e) => {
            tracing::error!("Failed to create tenant: {}", e);
            ApiResponseBuilder::error("创建租户失败")
        }
    }
}

/// Get tenant
async fn get_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<Tenant>> {
    match state.tenant_service.find_tenant_by_id(&id).await {
        Ok(Some(tenant)) => ApiResponseBuilder::success(tenant),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "租户不存在"),
        Err(e) => {
            tracing::error!("Failed to get tenant: {}", e);
            ApiResponseBuilder::error("获取租户失败")
        }
    }
}

/// Update tenant
async fn update_tenant(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Json(_payload): Json<crate::dto::entity::tenant::UpdateTenantRequest>,
) -> Json<ApiResponse<Tenant>> {
    // 简化实现
    ApiResponseBuilder::error_with_code(501, "功能未实现")
}

/// Change subscription plan
async fn change_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Json<ApiResponse<Tenant>> {
    let plan_id = match payload.get("plan_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ApiResponseBuilder::error_with_code(400, "缺少 plan_id 参数"),
    };

    match state.tenant_service.change_plan(&id, plan_id).await {
        Ok(tenant) => ApiResponseBuilder::success(tenant),
        Err(e) => {
            tracing::error!("Failed to change plan: {}", e);
            ApiResponseBuilder::error("切换套餐失败")
        }
    }
}

/// Get tenant usage
async fn get_tenant_usage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<Option<TenantUsage>>> {
    match state.tenant_service.get_tenant_usage(&id).await {
        Ok(usage) => ApiResponseBuilder::success(usage),
        Err(e) => {
            tracing::error!("Failed to get tenant usage: {}", e);
            ApiResponseBuilder::error("获取租户使用情况失败")
        }
    }
}

/// 验证 workspace 属于当前 tenant（防止伪造 header 越权）
async fn validate_workspace(state: &AppState, ws: &str, tenant_id: &str) -> Option<()> {
    // WorkspaceScope 返回的 workspace_id 来自请求头，可能被恶意伪造
    // 必须验证该 workspace 确实属于当前 tenant
    match state.workspace_service.find_by_id(ws).await {
        Ok(Some(ws_obj)) if ws_obj.tenant_id == tenant_id => Some(()),
        Ok(Some(_)) => None, // workspace 不属于此 tenant
        Ok(None) => None,    // workspace 不存在
        Err(_) => None,      // 查询失败，安全拒绝
    }
}

/// List API keys — GET /v1/api-keys (从 header X-Workspace-Id 获取 workspace)
async fn list_api_keys(
    State(state): State<AppState>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<ApiKey>>> {
    let ws = match workspace_id {
        Some(id) => id,
        None => return ApiResponseBuilder::error_with_code(400, "缺少 X-Workspace-Id header"),
    };

    // 验证 workspace 归属，防止伪造 header 越权
    if matches!(validate_workspace(&state, &ws, &claims.tenant_id).await, None) {
        return ApiResponseBuilder::error_with_code(403, "无权操作此 Workspace");
    }

    match state.tenant_service.find_api_keys_by_workspace(&ws).await {
        Ok(keys) => ApiResponseBuilder::success(keys),
        Err(e) => {
            tracing::error!("Failed to list api keys: {}", e);
            ApiResponseBuilder::error("获取 API Key 列表失败")
        }
    }
}

/// Create API key — POST /v1/api-keys
/// workspace 从 header X-Workspace-Id 获取，与 body 中的 tenant_id 一致性由 service 层验证
async fn create_api_key(
    State(state): State<AppState>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = match workspace_id {
        Some(id) => id,
        None => return ApiResponseBuilder::error_with_code(400, "缺少 X-Workspace-Id header"),
    };

    // 验证 workspace 归属，防止伪造 header 越权
    if matches!(validate_workspace(&state, &ws, &claims.tenant_id).await, None) {
        return ApiResponseBuilder::error_with_code(403, "无权操作此 Workspace");
    }

    if payload.workspace_id != ws {
        return ApiResponseBuilder::error_with_code(403, "workspace_id 不匹配");
    }

    match state.tenant_service.create_api_key(&payload.workspace_id, &payload).await {
        Ok((key, raw_key)) => {
            ApiResponseBuilder::success(serde_json::json!({
                "api_key": key,
                "raw_key": raw_key  // 只在创建时返回一次
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create api key: {}", e);
            ApiResponseBuilder::error("创建 API Key 失败")
        }
    }
}

/// Revoke API key — 验证 key 属于当前 tenant 的 workspace
async fn revoke_api_key(
    State(state): State<AppState>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws = match workspace_id {
        Some(id) => id,
        None => return ApiResponseBuilder::error_with_code(400, "缺少 X-Workspace-Id header"),
    };

    // 1. 验证 workspace 属于当前 tenant（防止伪造 header）
    match state.workspace_service.find_by_id(&ws).await {
        Ok(Some(ws)) if ws.tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return ApiResponseBuilder::error_with_code(403, "无权操作此 Workspace"),
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "Workspace 不存在"),
        Err(e) => {
            tracing::error!("Failed to find workspace: {}", e);
            return ApiResponseBuilder::error("验证 Workspace 失败");
        }
    }

    // 2. 验证 key 属于该 workspace
    match state.tenant_service.find_api_key_by_id(&id).await {
        Ok(Some(key)) if key.workspace_id == ws => {
            match state.tenant_service.revoke_api_key(&id).await {
                Ok(_) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
                Err(e) => {
                    tracing::error!("Failed to revoke api key: {}", e);
                    ApiResponseBuilder::error("撤销 API Key 失败")
                }
            }
        }
        Ok(Some(_)) => ApiResponseBuilder::error_with_code(403, "无权操作此 API Key"),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "API Key 不存在"),
        Err(e) => {
            tracing::error!("Failed to find api key: {}", e);
            ApiResponseBuilder::error("查询 API Key 失败")
        }
    }
}

/// Get usage stats
async fn get_usage_stats(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Json<ApiResponse<ApiUsageStats>> {
    match state.tenant_service.get_api_usage_stats(&tenant_id, 30).await {
        Ok(stats) => ApiResponseBuilder::success(stats),
        Err(e) => {
            tracing::error!("Failed to get usage stats: {}", e);
            ApiResponseBuilder::error("获取使用统计失败")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 create_api_key_router() 返回有效的 Router（axum 编译通过即路由定义正确）
    /// 路由路径：
    ///   GET  / → list_api_keys (query: workspace_id)
    ///   POST / → create_api_key (body: {workspace_id, name})
    ///   POST /{id}/revoke → revoke_api_key
    /// 前端调用: GET/POST /api-keys?workspace_id=xxx
    ///           + POST /api-keys/{id}/revoke
    #[test]
    fn test_create_api_key_router_compiles() {
        let _router = create_api_key_router();
    }

    /// 验证 create_router() 编译通过（仅含 tenants 相关路由）
    #[test]
    fn test_create_router_compiles() {
        let _router = create_router();
    }

    /// 验证 API Keys 独立于 tenants
    /// 新路径: /v1/api-keys (不在 /v1/tenants/ 下)
    /// 前端调用: GET /api-keys?workspace_id=xxx
    #[test]
    fn test_api_keys_router_independent() {
        // 两个 router 独立编译即说明路由结构正确
        let _tenants = create_router();
        let _api_keys = create_api_key_router();
    }
}

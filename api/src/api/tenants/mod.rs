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
    dto::entity::tenant::{
        ApiKey, ApiUsageStats, CreateApiKeyRequest, SubscriptionPlan, Tenant, TenantQueryParams,
        TenantUsage,
    },
    dto::response::{ApiResponse, builder::ApiResponseBuilder},
    shared::app_state::AppState,
};

/// Create tenants router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Plans - 移除这里重复的 plans，因为 auth router 已经有
        // .route("/plans", get(list_plans))
        // Tenants
        .route("/tenants", get(list_tenants))
        .route("/tenants", post(create_tenant))
        .route("/tenants/{id}", get(get_tenant))
        .route("/tenants/{id}", put(update_tenant))
        .route("/tenants/{id}/change-plan", post(change_plan))
        .route("/tenants/{id}/usage", get(get_tenant_usage))
        // API Keys
        .route("/tenants/{tenant_id}/api-keys", get(list_api_keys))
        .route("/tenants/{tenant_id}/api-keys", post(create_api_key))
        // 撤销 API Key 是不可逆动作，保持 RPC 风格
        .route("/api-keys/{id}/revoke", post(revoke_api_key))
        // Usage
        .route("/tenants/{tenant_id}/usage-stats", get(get_usage_stats))
}

/// List tenants
async fn list_tenants(
    State(state): State<AppState>,
    Query(params): Query<TenantQueryParams>,
) -> Json<ApiResponse<Vec<Tenant>>> {
    let db = state.database.clone();

    // 简化实现：返回空列表（需要管理权限）
    ApiResponseBuilder::success(vec![])
}

/// Create tenant
async fn create_tenant(
    State(state): State<AppState>,
    Json(payload): Json<crate::dto::entity::tenant::CreateTenantRequest>,
) -> Json<ApiResponse<Tenant>> {
    let db = state.database.clone();

    match Tenant::create(&db, &payload).await {
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
    let db = state.database.clone();

    match Tenant::find_by_id(&db, &id).await {
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
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<crate::dto::entity::tenant::UpdateTenantRequest>,
) -> Json<ApiResponse<Tenant>> {
    let db = state.database.clone();

    // 简化实现
    ApiResponseBuilder::error_with_code(501, "功能未实现")
}

/// Change subscription plan
async fn change_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Json<ApiResponse<Tenant>> {
    let db = state.database.clone();

    let plan_id = match payload.get("plan_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ApiResponseBuilder::error_with_code(400, "缺少 plan_id 参数"),
    };

    match Tenant::change_plan(&db, &id, plan_id).await {
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
    let db = state.database.clone();

    match Tenant::get_usage(&db, &id).await {
        Ok(usage) => ApiResponseBuilder::success(usage),
        Err(e) => {
            tracing::error!("Failed to get tenant usage: {}", e);
            ApiResponseBuilder::error("获取租户使用情况失败")
        }
    }
}

/// List API keys
async fn list_api_keys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Json<ApiResponse<Vec<ApiKey>>> {
    let db = state.database.clone();

    match ApiKey::find_by_tenant(&db, &tenant_id).await {
        Ok(keys) => ApiResponseBuilder::success(keys),
        Err(e) => {
            tracing::error!("Failed to list api keys: {}", e);
            ApiResponseBuilder::error("获取 API Key 列表失败")
        }
    }
}

/// Create API key
async fn create_api_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db = state.database.clone();

    match ApiKey::create(&db, &tenant_id, &payload).await {
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

/// Revoke API key
async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db = state.database.clone();

    match ApiKey::revoke(&db, &id).await {
        Ok(_) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!("Failed to revoke api key: {}", e);
            ApiResponseBuilder::error("撤销 API Key 失败")
        }
    }
}

/// Get usage stats
async fn get_usage_stats(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Json<ApiResponse<ApiUsageStats>> {
    let db = state.database.clone();

    match ApiKey::get_usage_stats(&db, &tenant_id, 30).await {
        Ok(stats) => ApiResponseBuilder::success(stats),
        Err(e) => {
            tracing::error!("Failed to get usage stats: {}", e);
            ApiResponseBuilder::error("获取使用统计失败")
        }
    }
}

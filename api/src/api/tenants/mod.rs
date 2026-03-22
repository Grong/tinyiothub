// SaaS Tenants API Module
// 租户管理 API

pub mod auth;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};

use crate::dto::entity::tenant::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, SubscriptionPlan, Tenant, TenantQueryParams,
    TenantUsage,
};
use crate::shared::app_state::AppState;

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
        .route("/tenants/{id}/suspend", post(suspend_tenant))
        .route("/tenants/{id}/activate", post(activate_tenant))
        .route("/tenants/{id}/change-plan", post(change_plan))
        .route("/tenants/{id}/usage", get(get_tenant_usage))
        // API Keys
        .route("/tenants/{tenant_id}/api-keys", get(list_api_keys))
        .route("/tenants/{tenant_id}/api-keys", post(create_api_key))
        .route("/api-keys/{id}/enable", post(enable_api_key))
        .route("/api-keys/{id}/disable", post(disable_api_key))
        .route("/api-keys/{id}/revoke", post(revoke_api_key))
        // Usage
        .route("/tenants/{tenant_id}/usage-stats", get(get_usage_stats))
}

/// List tenants
async fn list_tenants(
    State(state): State<AppState>,
    Query(params): Query<TenantQueryParams>,
) -> Result<Json<Vec<Tenant>>, StatusCode> {
    let db = state.database.clone();

    // 简化实现：返回空列表（需要管理权限）
    Ok(Json(vec![]))
}

/// Create tenant
async fn create_tenant(
    State(state): State<AppState>,
    Json(payload): Json<crate::dto::entity::tenant::CreateTenantRequest>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();

    match Tenant::create(&db, &payload).await {
        Ok(tenant) => Ok(Json(tenant)),
        Err(e) => {
            tracing::error!("Failed to create tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get tenant
async fn get_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();

    match Tenant::find_by_id(&db, &id).await {
        Ok(Some(tenant)) => Ok(Json(tenant)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update tenant
async fn update_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<crate::dto::entity::tenant::UpdateTenantRequest>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();

    // 简化实现
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Suspend tenant
async fn suspend_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();

    match Tenant::suspend(&db, &id).await {
        Ok(tenant) => Ok(Json(tenant)),
        Err(e) => {
            tracing::error!("Failed to suspend tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Activate tenant
async fn activate_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();

    match Tenant::activate(&db, &id).await {
        Ok(tenant) => Ok(Json(tenant)),
        Err(e) => {
            tracing::error!("Failed to activate tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Change subscription plan
async fn change_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();

    let plan_id = payload
        .get("plan_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    match Tenant::change_plan(&db, &id, plan_id).await {
        Ok(tenant) => Ok(Json(tenant)),
        Err(e) => {
            tracing::error!("Failed to change plan: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get tenant usage
async fn get_tenant_usage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Option<TenantUsage>>, StatusCode> {
    let db = state.database.clone();

    match Tenant::get_usage(&db, &id).await {
        Ok(usage) => Ok(Json(usage)),
        Err(e) => {
            tracing::error!("Failed to get tenant usage: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List API keys
async fn list_api_keys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Vec<ApiKey>>, StatusCode> {
    let db = state.database.clone();

    match ApiKey::find_by_tenant(&db, &tenant_id).await {
        Ok(keys) => Ok(Json(keys)),
        Err(e) => {
            tracing::error!("Failed to list api keys: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create API key
async fn create_api_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();

    match ApiKey::create(&db, &tenant_id, &payload).await {
        Ok((key, raw_key)) => {
            Ok(Json(serde_json::json!({
                "api_key": key,
                "raw_key": raw_key  // 只在创建时返回一次
            })))
        }
        Err(e) => {
            tracing::error!("Failed to create api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Enable API key
async fn enable_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();

    match ApiKey::enable(&db, &id).await {
        Ok(_) => Ok(Json(serde_json::json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to enable api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Disable API key
async fn disable_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();

    match ApiKey::disable(&db, &id).await {
        Ok(_) => Ok(Json(serde_json::json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to disable api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Revoke API key
async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();

    match ApiKey::revoke(&db, &id).await {
        Ok(_) => Ok(Json(serde_json::json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to revoke api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get usage stats
async fn get_usage_stats(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<ApiUsageStats>, StatusCode> {
    let db = state.database.clone();

    match ApiKey::get_usage_stats(&db, &tenant_id, 30).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            tracing::error!("Failed to get usage stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

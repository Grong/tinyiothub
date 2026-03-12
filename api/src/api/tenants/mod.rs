// SaaS 租户管理 API Module

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};

use crate::dto::entity::tenant::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, CreateTenantRequest, SubscriptionPlan,
    Tenant, TenantQueryParams, TenantUsage, UpdateTenantRequest,
};
use crate::shared::app_state::AppState;

/// 创建租户管理路由
pub fn create_router() -> Router<AppState> {
    Router::new()
        // 订阅计划 (公开)
        .route("/plans", get(list_plans))
        
        // 租户
        .route("/tenant", get(get_current_tenant))
        .route("/tenant", put(update_current_tenant))
        .route("/tenant/subscription", get(get_subscription))
        .route("/tenant/usage", get(get_usage))
        
        // 租户管理 (仅管理员)
        .route("/admin/tenants", get(list_tenants))
        .route("/admin/tenants", post(create_tenant))
        .route("/admin/tenants/{id}", get(get_tenant))
        .route("/admin/tenants/{id}", put(update_tenant))
        .route("/admin/tenants/{id}/suspend", post(suspend_tenant))
        .route("/admin/tenants/{id}/activate", post(activate_tenant))
        .route("/admin/tenants/{id}/change-plan", post(change_tenant_plan))
        
        // API Keys
        .route("/tenant/api-keys", get(list_api_keys))
        .route("/tenant/api-keys", post(create_api_key))
        .route("/tenant/api-keys/{id}/enable", post(enable_api_key))
        .route("/tenant/api-keys/{id}/disable", post(disable_api_key))
        .route("/tenant/api-keys/{id}/revoke", post(revoke_api_key))
        .route("/tenant/api-keys/{id}", delete(delete_api_key))
        
        // 使用统计
        .route("/tenant/usage/stats", get(get_usage_stats))
}

/// 列出所有订阅计划
async fn list_plans(
    State(state): State<AppState>,
) -> Result<Json<Vec<SubscriptionPlan>>, StatusCode> {
    let db = state.database.clone();
    
    match SubscriptionPlan::find_all(&db).await {
        Ok(plans) => Ok(Json(plans)),
        Err(e) => {
            tracing::error!("Failed to list plans: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取当前租户信息
async fn get_current_tenant(
    State(state): State<AppState>,
) -> Result<Json<Tenant>, StatusCode> {
    // 从 JWT 或 session 获取租户 ID
    // TODO: 实现租户上下文
    let tenant_id = "default"; // 临时
    
    let db = state.database.clone();
    
    match Tenant::find_by_id(&db, tenant_id).await {
        Ok(Some(tenant)) => Ok(Json(tenant)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 更新当前租户信息
async fn update_current_tenant(
    State(state): State<AppState>,
    Json(payload): Json<UpdateTenantRequest>,
) -> Result<Json<Tenant>, StatusCode> {
    let tenant_id = "default"; // TODO: 从上下文获取
    
    let db = state.database.clone();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    
    let mut updates = vec![format!("updated_at = '{}'", now)];
    
    if let Some(ref name) = payload.name {
        updates.push(format!("name = '{}'", name));
    }
    if let Some(ref billing_email) = payload.billing_email {
        updates.push(format!("billing_email = '{}'", billing_email));
    }
    if let Some(ref billing_contact) = payload.billing_contact {
        updates.push(format!("billing_contact = '{}'", billing_contact));
    }
    if let Some(ref timezone) = payload.timezone {
        updates.push(format!("timezone = '{}'", timezone));
    }
    if let Some(ref locale) = payload.locale {
        updates.push(format!("locale = '{}'", locale));
    }
    if let Some(ref custom_logo) = payload.custom_logo {
        updates.push(format!("custom_logo = '{}'", custom_logo));
    }
    if let Some(ref custom_theme) = payload.custom_theme {
        updates.push(format!("custom_theme = '{}'", custom_theme));
    }
    
    let sql = format!("UPDATE tenants SET {} WHERE id = '{}'", updates.join(", "), tenant_id);
    let _ = db.execute(&sql).await;
    
    match Tenant::find_by_id(&db, tenant_id).await {
        Ok(Some(tenant)) => Ok(Json(tenant)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取订阅信息
async fn get_subscription(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let tenant_id = "default"; // TODO: 从上下文获取
    
    let db = state.database.clone();
    
    let tenant = match Tenant::find_by_id(&db, tenant_id).await {
        Ok(Some(t)) => t,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    
    let plan = match SubscriptionPlan::find_by_id(&db, &tenant.plan_id).await {
        Ok(Some(p)) => p,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    
    Ok(Json(serde_json::json!({
        "tenant": tenant,
        "plan": plan
    })))
}

/// 获取使用量
async fn get_usage(
    State(state): State<AppState>,
) -> Result<Json<TenantUsage>, StatusCode> {
    let tenant_id = "default"; // TODO: 从上下文获取
    
    let db = state.database.clone();
    
    match Tenant::get_usage(&db, tenant_id).await {
        Ok(Some(usage)) => Ok(Json(usage)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get usage: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 列出租户 (管理员)
async fn list_tenants(
    State(state): State<AppState>,
    Query(params): Query<TenantQueryParams>,
) -> Result<Json<Vec<Tenant>>, StatusCode> {
    let db = state.database.clone();
    
    let mut sql = String::from("SELECT * FROM tenants WHERE 1=1");
    
    if let Some(ref status) = params.status {
        sql.push_str(&format!(" AND status = '{}'", status));
    }
    if let Some(ref plan_id) = params.plan_id {
        sql.push_str(&format!(" AND plan_id = '{}'", plan_id));
    }
    
    sql.push_str(" ORDER BY created_at DESC");
    
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;
    sql.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));
    
    match db.query(&sql, |row| {
        Ok(Tenant {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            slug: row.try_get("slug")?,
            status: row.try_get("status")?,
            plan_id: row.try_get("plan_id")?,
            subscription_status: row.try_get("subscription_status")?,
            trial_expires_at: row.try_get("trial_expires_at")?,
            billing_email: row.try_get("billing_email")?,
            billing_contact: row.try_get("billing_contact")?,
            timezone: row.try_get("timezone")?,
            locale: row.try_get("locale")?,
            custom_logo: row.try_get("custom_logo")?,
            custom_theme: row.try_get("custom_theme")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }).await {
        Ok(tenants) => Ok(Json(tenants)),
        Err(e) => {
            tracing::error!("Failed to list tenants: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 创建租户
async fn create_tenant(
    State(state): State<AppState>,
    Json(payload): Json<CreateTenantRequest>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();
    
    // 检查 slug 是否已存在
    if let Ok(Some(_)) = Tenant::find_by_slug(&db, &payload.slug).await {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    match Tenant::create(&db, &payload).await {
        Ok(tenant) => Ok(Json(tenant)),
        Err(e) => {
            tracing::error!("Failed to create tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取租户 (管理员)
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

/// 更新租户 (管理员)
async fn update_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateTenantRequest>,
) -> Result<Json<Tenant>, StatusCode> {
    let db = state.database.clone();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    
    let mut updates = vec![format!("updated_at = '{}'", now)];
    
    if let Some(ref name) = payload.name {
        updates.push(format!("name = '{}'", name));
    }
    if let Some(ref billing_email) = payload.billing_email {
        updates.push(format!("billing_email = '{}'", billing_email));
    }
    if let Some(ref billing_contact) = payload.billing_contact {
        updates.push(format!("billing_contact = '{}'", billing_contact));
    }
    if let Some(ref timezone) = payload.timezone {
        updates.push(format!("timezone = '{}'", timezone));
    }
    if let Some(ref locale) = payload.locale {
        updates.push(format!("locale = '{}'", locale));
    }
    
    let sql = format!("UPDATE tenants SET {} WHERE id = '{}'", updates.join(", "), id);
    let _ = db.execute(&sql).await;
    
    match Tenant::find_by_id(&db, &id).await {
        Ok(Some(tenant)) => Ok(Json(tenant)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 暂停租户
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

/// 激活租户
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

/// 更改租户计划
async fn change_tenant_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Tenant>, StatusCode> {
    let plan_id = payload.get("plan_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    let db = state.database.clone();
    
    match Tenant::change_plan(&db, &id, plan_id).await {
        Ok(tenant) => Ok(Json(tenant)),
        Err(e) => {
            tracing::error!("Failed to change plan: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 列出 API Keys
async fn list_api_keys(
    State(state): State<AppState>,
) -> Result<Json<Vec<ApiKey>>, StatusCode> {
    let tenant_id = "default"; // TODO: 从上下文获取
    
    let db = state.database.clone();
    
    match ApiKey::find_by_tenant(&db, tenant_id).await {
        Ok(keys) => Ok(Json(keys)),
        Err(e) => {
            tracing::error!("Failed to list api keys: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 创建 API Key
async fn create_api_key(
    State(state): State<AppState>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let tenant_id = "default"; // TODO: 从上下文获取
    
    let db = state.database.clone();
    
    match ApiKey::create(&db, tenant_id, &payload).await {
        Ok((key, raw_key)) => {
            Ok(Json(serde_json::json!({
                "key": key,
                "raw_key": raw_key  // 只在创建时返回一次
            })))
        }
        Err(e) => {
            tracing::error!("Failed to create api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 启用 API Key
async fn enable_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let db = state.database.clone();
    
    match ApiKey::enable(&db, &id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            tracing::error!("Failed to enable api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 禁用 API Key
async fn disable_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let db = state.database.clone();
    
    match ApiKey::disable(&db, &id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            tracing::error!("Failed to disable api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 撤销 API Key
async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let db = state.database.clone();
    
    match ApiKey::revoke(&db, &id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            tracing::error!("Failed to revoke api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 删除 API Key
async fn delete_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let db = state.database.clone();
    
    let sql = format!("DELETE FROM api_keys WHERE id = '{}'", id);
    match db.execute(&sql).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            tracing::error!("Failed to delete api key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取使用统计
async fn get_usage_stats(
    State(state): State<AppState>,
    Query(params): Query<serde_json::Value>,
) -> Result<Json<ApiUsageStats>, StatusCode> {
    let tenant_id = "default"; // TODO: 从上下文获取
    let days = params.get("days")
        .and_then(|v| v.as_i64())
        .unwrap_or(30) as i32;
    
    let db = state.database.clone();
    
    match ApiKey::get_usage_stats(&db, tenant_id, days).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            tracing::error!("Failed to get usage stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

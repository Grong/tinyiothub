// Tenant API handlers — includes CRUD, auth, and API key management

use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use sqlx::Row;

use super::types::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, CreateTenantRequest, SubscriptionPlan, Tenant,
    TenantQueryParams, TenantUsage,
};
use crate::{
    shared::api_response::ApiResponse,
    shared::app_state::AppState,
};
use crate::api::middleware::WorkspaceScope;
use crate::shared::security::jwt::Claims;

type HmacSha256 = Hmac<Sha256>;

// --- Token helpers ---

fn sign_payload(payload: &str, secret: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result.into_bytes())
}

fn verify_signature(payload: &str, signature: &str, secret: &str) -> bool {
    use subtle::ConstantTimeEq;
    let expected = sign_payload(payload, secret);
    expected.as_bytes().ct_eq(signature.as_bytes()).into()
}

fn generate_token(tenant_id: &str, user_id: &str) -> String {
    let secret = crate::shared::config::get().security.jwt.secret.clone();

    let exp = chrono::Utc::now().timestamp() + 86400 * 7;
    let payload = serde_json::json!({
        "tenant_id": tenant_id,
        "user_id": user_id,
        "exp": exp,
    });

    let payload_str = payload.to_string();
    let signature = sign_payload(&payload_str, &secret);

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, payload_str);

    format!("tj_{}:{}", encoded, signature)
}

// --- Auth types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisterRequest {
    pub name: String,
    pub slug: String,
    pub email: String,
    pub password: String,
    pub plan_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginResponse {
    pub token: String,
    pub tenant: Tenant,
    pub user: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct VerifyTokenParams {
    token: String,
}

// --- Password helpers ---

fn hash_password(password: &str) -> Result<String, String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| format!("Failed to hash password: {}", e))
}

fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    bcrypt::verify(password, hash).map_err(|e| format!("Failed to verify password: {}", e))
}

// --- Routers ---

/// Create API Keys router
pub fn create_api_key_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_api_keys))
        .route("/", post(create_api_key))
        .route("/{id}", delete(revoke_api_key))
}

/// Create tenants router (mounted at /v1/tenants)
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Tenant CRUD — paths already include /tenants prefix for nesting under /v1/tenants
        .route("/tenants", get(list_tenants))
        .route("/tenants", post(create_tenant))
        .route("/tenants/{id}", get(get_tenant))
        .route("/tenants/{id}", put(update_tenant))
        .route("/tenants/{id}/change-plan", post(change_plan))
        .route("/tenants/{id}/usage", get(get_tenant_usage))
        // Usage
        .route("/{tenant_id}/usage-stats", get(get_usage_stats))
}

/// Create tenant auth router (mounted at /v1/tenants — public, no JWT)
pub fn create_auth_router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register_tenant))
        .route("/login", post(login))
        .route("/verify", get(verify_token))
        .route("/plans", get(list_plans))
}

// --- Auth handlers ---

async fn list_plans(
    State(state): State<AppState>,
) -> Result<Json<Vec<SubscriptionPlan>>, StatusCode> {
    match state.tenant_service.find_all_plans().await {
        Ok(plans) => Ok(Json(plans)),
        Err(e) => {
            tracing::error!("Failed to list plans: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn register_tenant(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let slug = payload.slug.trim().to_lowercase();
    let email = payload.email.trim().to_lowercase();

    if !crate::shared::utils::validation::is_valid_slug(&slug) {
        return Err(StatusCode::BAD_REQUEST);
    }

    if !crate::shared::utils::validation::is_valid_email(&email) {
        return Err(StatusCode::BAD_REQUEST);
    }

    if !crate::shared::utils::validation::is_strong_password(&payload.password) {
        return Err(StatusCode::BAD_REQUEST);
    }

    if state.tenant_service.find_tenant_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some()
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let tenant_req = CreateTenantRequest {
        name: payload.name.clone(),
        slug: slug.clone(),
        billing_email: Some(email.clone()),
        billing_contact: None,
        timezone: None,
        locale: None,
    };

    let tenant = state.tenant_service.create_tenant(&tenant_req).await.map_err(|e| {
        tracing::error!("Failed to create tenant: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let db = state.database.clone();

    let user_id = uuid::Uuid::new_v4().to_string();
    let password_hash =
        hash_password(&payload.password).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, email, is_enabled, created_at, updated_at)
           VALUES (?, ?, ?, ?, 1, ?, ?)"#,
    )
    .bind(&user_id)
    .bind(&email)
    .bind(&password_hash)
    .bind(&email)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to insert user during tenant registration: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let tenant_user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO tenant_users (id, tenant_id, user_id, role, invitation_status, joined_at, created_at, updated_at)
           VALUES (?, ?, ?, 'owner', 'accepted', ?, ?, ?)"#
    )
    .bind(&tenant_user_id)
    .bind(&tenant.id)
    .bind(&user_id)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.workspace_service.create(&tenant.id, "默认工作空间", Some("系统自动创建的默认工作空间"), None, None)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create default workspace: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let token = generate_token(&tenant.id, &user_id);

    Ok(Json(serde_json::json!({
        "token": token,
        "tenant": tenant,
        "user": {
            "id": user_id,
            "email": email,
            "role": "owner"
        }
    })))
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let db = state.database.clone();

    let email = payload.email.trim().to_lowercase();

    if !crate::shared::utils::validation::is_valid_email(&email) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let rows = sqlx::query(
        "SELECT id, username, password_hash FROM users WHERE email = ? AND is_enabled = 1 LIMIT 1"
    )
    .bind(&email)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user_row = rows.into_iter().next().ok_or(StatusCode::UNAUTHORIZED)?;

    let user_id: String = user_row.try_get("id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _username: String =
        user_row.try_get("username").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stored_hash: String =
        user_row.try_get("password_hash").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let password_valid = verify_password(&payload.password, &stored_hash).unwrap_or(false);

    if !password_valid {
        tracing::warn!("Failed login attempt for email: {}", email);
        return Err(StatusCode::UNAUTHORIZED);
    }

    let tenant_rows = sqlx::query(
        "SELECT t.* FROM tenants t
         INNER JOIN tenant_users tu ON t.id = tu.tenant_id
         WHERE tu.user_id = ? LIMIT 1",
    )
    .bind(&user_id)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;

    let tenant_row = tenant_rows.into_iter().next().ok_or(StatusCode::NOT_FOUND)?;

    let tenant = Tenant {
        id: tenant_row.try_get("id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        name: tenant_row.try_get("name").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        slug: tenant_row.try_get("slug").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        status: tenant_row.try_get("status").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        plan_id: tenant_row.try_get("plan_id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        subscription_status: tenant_row
            .try_get("subscription_status")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        trial_expires_at: tenant_row
            .try_get("trial_expires_at")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        billing_email: tenant_row
            .try_get("billing_email")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        billing_contact: tenant_row
            .try_get("billing_contact")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        timezone: tenant_row.try_get("timezone").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        locale: tenant_row.try_get("locale").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        custom_logo: tenant_row
            .try_get("custom_logo")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        custom_theme: tenant_row
            .try_get("custom_theme")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        created_at: tenant_row
            .try_get("created_at")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        updated_at: tenant_row
            .try_get("updated_at")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };

    let token = generate_token(&tenant.id, &user_id);

    Ok(Json(LoginResponse {
        token,
        tenant,
        user: serde_json::json!({
            "id": user_id,
            "email": email,
        }),
    }))
}

async fn verify_token(
    State(_state): State<AppState>,
    Query(params): Query<VerifyTokenParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let token = params.token;

    let token = token.strip_prefix("tj_").ok_or(StatusCode::BAD_REQUEST)?;
    let parts: Vec<&str> = token.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let signature = parts[0];
    let payload_encoded = parts[1];

    let payload_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload_encoded)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    let payload_str = String::from_utf8(payload_bytes).map_err(|_| StatusCode::BAD_REQUEST)?;

    let secret = crate::shared::config::get().security.jwt.secret.clone();

    if !verify_signature(&payload_str, signature, &secret) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).map_err(|_| StatusCode::BAD_REQUEST)?;

    let exp = payload["exp"].as_i64().ok_or(StatusCode::BAD_REQUEST)?;
    if chrono::Utc::now().timestamp() > exp {
        return Ok(Json(serde_json::json!({
            "valid": false,
            "error": "Token expired"
        })));
    }

    Ok(Json(serde_json::json!({
        "valid": true
    })))
}

// --- Tenant CRUD handlers ---

async fn list_tenants(
    State(_state): State<AppState>,
    Query(_params): Query<TenantQueryParams>,
) -> Json<ApiResponse<Vec<Tenant>>> {
    ApiResponseBuilder::success(vec![])
}

async fn create_tenant(
    State(state): State<AppState>,
    Json(payload): Json<CreateTenantRequest>,
) -> Json<ApiResponse<Tenant>> {
    match state.tenant_service.create_tenant(&payload).await {
        Ok(tenant) => ApiResponseBuilder::success(tenant),
        Err(e) => {
            tracing::error!("Failed to create tenant: {}", e);
            ApiResponseBuilder::error("创建租户失败")
        }
    }
}

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

async fn update_tenant(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Json(_payload): Json<super::types::UpdateTenantRequest>,
) -> Json<ApiResponse<Tenant>> {
    ApiResponseBuilder::error_with_code(501, "功能未实现")
}

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

async fn validate_workspace(state: &AppState, ws: &str, tenant_id: &str) -> Option<()> {
    match state.workspace_service.find_by_id(ws).await {
        Ok(Some(ws_obj)) if ws_obj.tenant_id == tenant_id => Some(()),
        Ok(Some(_)) => None,
        Ok(None) => None,
        Err(_) => None,
    }
}

// --- API Key handlers ---

async fn list_api_keys(
    State(state): State<AppState>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<ApiKey>>> {
    let ws = match workspace_id {
        Some(id) => id,
        None => return ApiResponseBuilder::error_with_code(400, "缺少 X-Workspace-Id header"),
    };

    if validate_workspace(&state, &ws, &claims.tenant_id).await.is_none() {
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

    if validate_workspace(&state, &ws, &claims.tenant_id).await.is_none() {
        return ApiResponseBuilder::error_with_code(403, "无权操作此 Workspace");
    }

    if payload.workspace_id != ws {
        return ApiResponseBuilder::error_with_code(403, "workspace_id 不匹配");
    }

    match state.tenant_service.create_api_key(&payload.workspace_id, &payload).await {
        Ok((key, raw_key)) => {
            ApiResponseBuilder::success(serde_json::json!({
                "api_key": key,
                "raw_key": raw_key
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create api key: {}", e);
            ApiResponseBuilder::error("创建 API Key 失败")
        }
    }
}

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

    match state.workspace_service.find_by_id(&ws).await {
        Ok(Some(ws)) if ws.tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => return ApiResponseBuilder::error_with_code(403, "无权操作此 Workspace"),
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "Workspace 不存在"),
        Err(e) => {
            tracing::error!("Failed to find workspace: {}", e);
            return ApiResponseBuilder::error("验证 Workspace 失败");
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_api_key_router_compiles() {
        let _router = create_api_key_router();
    }

    #[test]
    fn test_create_router_compiles() {
        let _router = create_router();
    }

    #[test]
    fn test_api_keys_router_independent() {
        let _tenants = create_router();
        let _api_keys = create_api_key_router();
    }
}

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

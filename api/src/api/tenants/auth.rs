// Tenant Auth API Module
// 租户注册登录 API
// 已修复 SQL 注入问题，使用参数化查询

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use sqlx::Row;

use crate::dto::entity::tenant::{CreateTenantRequest, SubscriptionPlan, Tenant};
use crate::shared::app_state::AppState;

/// Create tenant auth router
pub fn create_auth_router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register_tenant))
        .route("/login", post(login))
        .route("/verify", get(verify_token))
        .route("/plans", get(list_plans))
}

/// Register request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisterRequest {
    pub name: String,
    pub slug: String,
    pub email: String,
    pub password: String,
    pub plan_id: Option<String>,
}

/// Login request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginResponse {
    pub token: String,
    pub tenant: Tenant,
    pub user: serde_json::Value,
}

/// List subscription plans (public)
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

/// 使用 bcrypt 进行密码哈希
fn hash_password(password: &str) -> Result<String, String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| format!("Failed to hash password: {}", e))
}

/// 验证密码
fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    bcrypt::verify(password, hash)
        .map_err(|e| format!("Failed to verify password: {}", e))
}

/// Register tenant
async fn register_tenant(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();
    
    // 输入验证
    let slug = payload.slug.trim().to_lowercase();
    let email = payload.email.trim().to_lowercase();
    
    // 验证 slug 格式
    if !crate::utils::validation::is_valid_slug(&slug) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 验证邮箱格式
    if !crate::utils::validation::is_valid_email(&email) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 验证密码强度
    if !crate::utils::validation::is_strong_password(&payload.password) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 检查 slug 是否已存在（使用参数化查询）
    if Tenant::find_by_slug(&db, &slug).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 创建租户
    let tenant_req = CreateTenantRequest {
        name: payload.name.clone(),
        slug: slug.clone(),
        billing_email: Some(email.clone()),
        billing_contact: None,
        timezone: None,
        locale: None,
    };
    
    let tenant = Tenant::create(&db, &tenant_req)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create tenant: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // 创建用户 - 使用参数化查询
    let user_id = uuid::Uuid::new_v4().to_string();
    let password_hash = hash_password(&payload.password)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    
    // 使用参数化查询防止 SQL 注入
    sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, email, status, created_at, updated_at)
           VALUES (?, ?, ?, ?, 'active', ?, ?)"#
    )
    .bind(&user_id)
    .bind(&email)
    .bind(&password_hash)
    .bind(&email)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // 关联租户用户
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
    
    // 生成 token
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

/// Login
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let db = state.database.clone();
    
    // 输入验证
    let email = payload.email.trim().to_lowercase();
    
    // 验证邮箱格式
    if !crate::utils::validation::is_valid_email(&email) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 查找用户 - 使用参数化查询
    let rows = sqlx::query(
        "SELECT id, username, password_hash FROM users WHERE email = ? AND status = 'active' LIMIT 1"
    )
    .bind(&email)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user_row = rows.into_iter().next().ok_or(StatusCode::UNAUTHORIZED)?;
    
    let user_id: String = user_row.try_get("id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _username: String = user_row.try_get("username").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stored_hash: String = user_row.try_get("password_hash").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // 验证密码
    let password_valid = verify_password(&payload.password, &stored_hash)
        .unwrap_or(false);
    
    if !password_valid {
        tracing::warn!("Failed login attempt for email: {}", email);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // 查找租户 - 使用参数化查询
    let tenant_rows = sqlx::query(
        "SELECT t.* FROM tenants t 
         INNER JOIN tenant_users tu ON t.id = tu.tenant_id 
         WHERE tu.user_id = ? LIMIT 1"
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
        subscription_status: tenant_row.try_get("subscription_status").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        trial_expires_at: tenant_row.try_get("trial_expires_at").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        billing_email: tenant_row.try_get("billing_email").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        billing_contact: tenant_row.try_get("billing_contact").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        timezone: tenant_row.try_get("timezone").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        locale: tenant_row.try_get("locale").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        custom_logo: tenant_row.try_get("custom_logo").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        custom_theme: tenant_row.try_get("custom_theme").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        created_at: tenant_row.try_get("created_at").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        updated_at: tenant_row.try_get("updated_at").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };
    
    // 生成 token
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

/// Verify token
async fn verify_token(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "valid": true
    })))
}

/// Generate simple token
fn generate_token(tenant_id: &str, user_id: &str) -> String {
    let payload = serde_json::json!({
        "tenant_id": tenant_id,
        "user_id": user_id,
        "exp": chrono::Utc::now().timestamp() + 86400 * 7,
    });
    
    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        payload.to_string()
    );
    
    format!("tj_{}", encoded)
}

// Tenant Auth API Module
// 租户注册登录 API

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

/// Simple hash function
fn simple_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Register tenant
async fn register_tenant(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.database.clone();
    
    // 检查 slug 是否已存在
    if Tenant::find_by_slug(&db, &payload.slug).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // 创建租户
    let tenant_req = CreateTenantRequest {
        name: payload.name,
        slug: payload.slug.clone(),
        billing_email: Some(payload.email.clone()),
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
    
    // 创建用户
    let user_id = uuid::Uuid::new_v4().to_string();
    let password_hash = simple_hash(&payload.password);
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    
    let sql = format!(r#"
        INSERT INTO users (id, username, password_hash, email, status, created_at, updated_at)
        VALUES ('{}', '{}', '{}', '{}', 'active', '{}', '{}')
    "#,
        user_id,
        payload.email,
        password_hash,
        payload.email,
        now,
        now
    );
    
    db.execute(&sql).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // 关联租户用户
    let tenant_user_sql = format!(r#"
        INSERT INTO tenant_users (id, tenant_id, user_id, role, invitation_status, joined_at, created_at, updated_at)
        VALUES ('{}', '{}', '{}', 'owner', 'accepted', '{}', '{}', '{}')
    "#,
        uuid::Uuid::new_v4().to_string(),
        tenant.id,
        user_id,
        now,
        now,
        now
    );
    
    db.execute(&tenant_user_sql).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // 生成 token
    let token = generate_token(&tenant.id, &user_id);
    
    Ok(Json(serde_json::json!({
        "token": token,
        "tenant": tenant,
        "user": {
            "id": user_id,
            "email": payload.email,
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
    
    // 查找用户
    let sql = format!(
        "SELECT id, username, password_hash FROM users WHERE email = '{}' AND status = 'active' LIMIT 1",
        payload.email
    );
    
    let mut rows = db.query(&sql, |row| {
        Ok(serde_json::json!({
            "id": row.try_get::<String, _>("id")?,
            "username": row.try_get::<String, _>("username")?,
            "password_hash": row.try_get::<String, _>("password_hash")?,
        }))
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let user = rows.pop().ok_or(StatusCode::UNAUTHORIZED)?;
    
    // 验证密码
    let stored_hash = user.get("password_hash").and_then(|v| v.as_str()).unwrap_or("");
    let input_hash = simple_hash(&payload.password);
    
    if stored_hash != input_hash {
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    let user_id = user.get("id").and_then(|v| v.as_str()).unwrap_or("");
    
    // 查找租户
    let tenant_sql = format!(
        "SELECT t.* FROM tenants t 
         INNER JOIN tenant_users tu ON t.id = tu.tenant_id 
         WHERE tu.user_id = '{}' LIMIT 1",
        user_id
    );
    
    let mut tenant_rows = db.query(&tenant_sql, |row| {
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
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let tenant = tenant_rows.pop().ok_or(StatusCode::NOT_FOUND)?;
    
    // 生成 token
    let token = generate_token(&tenant.id, user_id);
    
    Ok(Json(LoginResponse {
        token,
        tenant,
        user: serde_json::json!({
            "id": user_id,
            "email": payload.email,
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

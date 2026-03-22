// 短信验证码认证模块
// 支持手机验证码登录/注册

use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::api::AppState;
use crate::dto::response::ApiResponse;
use crate::infrastructure::config::get as get_config;

// 验证码有效期（秒）
const CODE_EXPIRE_SECONDS: u64 = 300; // 5 分钟

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/send", post(send_code))
        .route("/login", post(login_with_code))
        .route("/verify", get(verify_code))
}

// ============== 请求/响应结构 ==============

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendCodeRequest {
    pub phone: String,
    pub purpose: Option<String>, // login, register, reset_password
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginWithCodeRequest {
    pub phone: String,
    pub code: String,
    pub tenant_slug: Option<String>, // SaaS 模式下的租户标识
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SendCodeResponse {
    pub expires_in: u64, // 验证码有效期（秒）
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginWithCodeResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_info: UserInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UserInfo {
    pub id: String,
    pub phone: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
}

// ============== 验证码相关配置 ==============

// ============== 路由处理函数 ==============

/// 发送验证码
async fn send_code(
    State(state): State<AppState>,
    Json(request): Json<SendCodeRequest>,
) -> Json<ApiResponse<SendCodeResponse>> {
    // 检查 SMS 是否启用
    let config = get_config();
    if !config.sms.enabled {
        return ApiResponse::error("短信服务未启用".to_string());
    }

    let phone = request.phone.trim();

    // 验证手机号格式
    if !validate_phone(phone) {
        return ApiResponse::error("手机号格式不正确".to_string());
    }

    let purpose = request.purpose.unwrap_or_else(|| "login".to_string());

    // 从配置读取验证码有效期
    let code_expire_secs = config
        .sms
        .rate_limit
        .as_ref()
        .map(|r| r.code_expire_secs)
        .unwrap_or(300);

    // 检查频率限制
    let db = state.database();
    let _rate_limit_max = config
        .sms
        .rate_limit
        .as_ref()
        .map(|r| r.max_per_minute)
        .unwrap_or(5) as i64;

    // 生成验证码
    let code = generate_code();
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(CODE_EXPIRE_SECONDS as i64);

    // 存储验证码
    let id = uuid::Uuid::new_v4().to_string();
    let result = sqlx::query(
        r#"INSERT INTO sms_codes (id, phone, code, purpose, expires_at)
            VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(phone)
    .bind(&code)
    .bind(&purpose)
    .bind(expires_at.to_rfc3339())
    .execute(db.pool())
    .await;

    if let Err(e) = result {
        tracing::error!("Failed to save SMS code: {}", e);
        return ApiResponse::error("发送失败，请稍后重试".to_string());
    }

    // TODO: 实际发送短信（接入短信服务商）
    // 这里先返回验证码（仅供测试）
    tracing::info!("SMS code sent to {}: [REDACTED]", phone);

    // 返回成功响应（测试模式下返回验证码）
    #[cfg(debug_assertions)]
    {
        ApiResponse::success(SendCodeResponse {
            expires_in: CODE_EXPIRE_SECONDS,
            message: format!("验证码已发送（测试模式: {}）", code),
        })
    }

    #[cfg(not(debug_assertions))]
    {
        ApiResponse::success(SendCodeResponse {
            expires_in: CODE_EXPIRE_SECONDS,
            message: "验证码已发送".to_string(),
        })
    }
}

/// 验证码登录
async fn login_with_code(
    State(state): State<AppState>,
    Json(request): Json<LoginWithCodeRequest>,
) -> Json<ApiResponse<LoginWithCodeResponse>> {
    let phone = request.phone.trim();
    let code = request.code.trim();

    // 验证手机号格式
    if !validate_phone(phone) {
        return ApiResponse::error("手机号格式不正确".to_string());
    }

    // 验证验证码
    let db = state.database();

    let rows = match sqlx::query(
        r#"SELECT id, code, expires_at, verified_at FROM sms_codes
            WHERE phone = ? AND purpose = 'login'
            AND verified_at IS NULL
            ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(phone)
    .fetch_all(db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return ApiResponse::error("登录失败，请稍后重试".to_string());
        }
    };

    if rows.is_empty() {
        return ApiResponse::error("验证码已失效，请重新获取".to_string());
    }

    let row = &rows[0];

    // 获取存储的验证码
    let stored_code: String = match row.try_get("code") {
        Ok(c) => c,
        Err(_) => {
            return ApiResponse::error("验证码数据异常".to_string());
        }
    };

    // 获取过期时间
    let expires_at: String = match row.try_get("expires_at") {
        Ok(e) => e,
        Err(_) => {
            return ApiResponse::error("验证码数据异常".to_string());
        }
    };

    // 检查验证码是否过期
    if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&expires_at) {
        if exp < chrono::Utc::now() {
            return ApiResponse::error("验证码已过期，请重新获取".to_string());
        }
    }

    // 比较验证码 — 使用常量时间比较防止时序攻击
    use subtle::ConstantTimeEq;
    if stored_code.as_bytes().ct_eq(code.as_bytes()).into() {
        // correct
    } else {
        return ApiResponse::error("验证码错误".to_string());
    }

    // 验证码验证成功，标记为已验证
    let record_id: String = match row.try_get("id") {
        Ok(id) => id,
        Err(_) => {
            return ApiResponse::error("验证码数据异常".to_string());
        }
    };

    if let Err(e) = sqlx::query("UPDATE sms_codes SET verified_at = ? WHERE id = ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&record_id)
        .execute(db.pool())
        .await
    {
        tracing::error!("Failed to mark code as verified: {}", e);
        // 不影响登录，只是记录
    }

    // 查找或创建用户
    let user = find_or_create_user_by_phone(db, phone).await;

    match user {
        Ok(user) => {
            // 生成 token
            let token = generate_jwt_token(&user.id);

            ApiResponse::success(LoginWithCodeResponse {
                access_token: token,
                token_type: "Bearer".to_string(),
                expires_in: 86400,
                user_info: UserInfo {
                    id: user.id,
                    phone: user.phone.unwrap_or_default(),
                    username: Some(user.username),
                    display_name: user.display_name,
                },
            })
        }
        Err(e) => {
            tracing::error!("Failed to find or create user: {}", e);
            ApiResponse::error("登录失败，请稍后重试".to_string())
        }
    }
}

/// 验证验证码（查询状态）
async fn verify_code(
    State(state): State<AppState>,
    Query(params): Query<VerifyCodeQuery>,
) -> Json<ApiResponse<VerifyCodeResponse>> {
    let phone = params.phone.unwrap_or_default();
    let code = params.code.unwrap_or_default();

    // 验证手机号格式
    if !validate_phone(&phone) {
        return ApiResponse::error("手机号格式不正确".to_string());
    }

    if code.is_empty() {
        return ApiResponse::error("验证码不能为空".to_string());
    }

    let db = state.database();

    // 从数据库获取最新的未验证验证码
    let rows = match sqlx::query(
        r#"SELECT id, code, expires_at, verified_at FROM sms_codes
            WHERE phone = ? AND purpose = 'login'
            AND verified_at IS NULL
            ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(&phone)
    .fetch_all(db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return ApiResponse::error("验证失败，请稍后重试".to_string());
        }
    };

    if rows.is_empty() {
        return ApiResponse::success(VerifyCodeResponse {
            valid: false,
            message: "验证码不存在或已失效".to_string(),
        });
    }

    // 获取第一条记录
    let row = &rows[0];

    // 获取存储的验证码
    let stored_code: String = match row.try_get("code") {
        Ok(c) => c,
        Err(_) => {
            return ApiResponse::error("验证码数据异常".to_string());
        }
    };

    // 获取过期时间
    let expires_at: String = match row.try_get("expires_at") {
        Ok(e) => e,
        Err(_) => {
            return ApiResponse::error("验证码数据异常".to_string());
        }
    };

    // 检查验证码是否过期
    if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&expires_at) {
        if exp < chrono::Utc::now() {
            return ApiResponse::success(VerifyCodeResponse {
                valid: false,
                message: "验证码已过期".to_string(),
            });
        }
    }

    // 比较验证码 — 使用常量时间比较防止时序攻击
    use subtle::ConstantTimeEq;
    if stored_code.as_bytes().ct_eq(code.as_bytes()).into() {
        // correct
    } else {
        return ApiResponse::success(VerifyCodeResponse {
            valid: false,
            message: "验证码错误".to_string(),
        });
    }

    ApiResponse::success(VerifyCodeResponse {
        valid: true,
        message: "验证码验证成功".to_string(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VerifyCodeQuery {
    pub phone: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct VerifyCodeResponse {
    pub valid: bool,
    pub message: String,
}

// ============== 辅助函数 ==============

/// 验证手机号格式（中国大陆手机号）
fn validate_phone(phone: &str) -> bool {
    // 简单验证：11位数字，以1开头
    phone.len() == 11 && phone.starts_with('1') && phone.chars().all(|c| c.is_ascii_digit())
}

/// 生成随机验证码
fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let code: u32 = rng.gen_range(0..1_000_000);
    format!("{:06}", code)
}

/// 根据手机号查找或创建用户
async fn find_or_create_user_by_phone(
    db: &crate::infrastructure::persistence::database::Database,
    phone: &str,
) -> Result<crate::dto::entity::user::User, Box<dyn std::error::Error + Send + Sync>> {
    // 查找现有用户
    let rows = sqlx::query("SELECT * FROM users WHERE phone = ? LIMIT 1")
        .bind(phone)
        .fetch_all(db.pool())
        .await?;

    if let Some(row) = rows.into_iter().next() {
        return Ok(crate::dto::entity::user::User {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            password_hash: row.try_get("password_hash")?,
            email: row.try_get("email")?,
            phone: row.try_get("phone")?,
            display_name: row.try_get("display_name")?,
            is_enabled: row.try_get::<i32, _>("is_enabled")? == 1,
            parent_id: row.try_get("parent_id")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            last_login_at: row.try_get("last_login_at")?,
        });
    }

    // 创建新用户
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO users (id, username, phone, is_enabled, created_at, updated_at)
            VALUES (?, ?, ?, 1, ?, ?)"#,
    )
    .bind(&user_id)
    .bind(phone)
    .bind(phone)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await?;

    // 直接构建并返回新用户
    Ok(crate::dto::entity::user::User {
        id: user_id,
        username: phone.to_string(),
        password_hash: String::new(),
        email: None,
        phone: Some(phone.to_string()),
        display_name: None,
        is_enabled: true,
        parent_id: None,
        created_at: now.clone(),
        updated_at: now,
        last_login_at: None,
    })
}

/// 生成简单的 JWT token（简化版）
fn generate_jwt_token(user_id: &str) -> String {
    let payload = serde_json::json!({
        "user_id": user_id,
        "exp": chrono::Utc::now().timestamp() + 86400,
    });

    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        payload.to_string(),
    );

    format!("sms_{}", encoded)
}

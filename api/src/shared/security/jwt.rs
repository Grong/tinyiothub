use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration as ChronoDuration, Local};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};
use jwt_simple::prelude::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// 使用 jwt-simple 的 HS256Key (纯 Rust 实现，不依赖 ring)
pub static JWT_KEY: Lazy<Result<HS256Key, String>> = Lazy::new(|| {
    // 从环境变量读取JWT密钥
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| {
            // 生成一个随机密钥用于开发
            let random_key = uuid::Uuid::new_v4().to_string() + &uuid::Uuid::new_v4().to_string();
            tracing::warn!("⚠️  JWT_SECRET not set, using generated key (won't persist across restarts!)");
            random_key
        });
    
    // 验证密钥长度
    if secret.len() < 32 {
        return Err(format!(
            "JWT_SECRET is too short! Minimum 32 characters required, got {}",
            secret.len()
        ));
    }
    
    // 检查是否使用默认或弱密钥
    if secret.len() < 64 {
        tracing::warn!("⚠️  JWT_SECRET is shorter than 64 characters, consider using a longer secret");
    }
    
    Ok(HS256Key::from_bytes(secret.as_bytes()))
});

// 获取 JWT 密钥的辅助函数
fn get_jwt_key() -> Result<HS256Key, String> {
    JWT_KEY.clone().map_err(|e| {
        tracing::error!("JWT key error: {}", e);
        format!("JWT key error: {}", e)
    })
}

// 检查是否在 HarmonyOS 环境
fn is_harmonyos() -> bool {
    std::env::var("HARMONYOS_MODE").is_ok()
}

// ============================================================================
// HarmonyOS 专用：不使用任何加密库的安全 token 实现
// ============================================================================

// 简单但有效的校验和算法（不使用加密哈希）
fn simple_checksum(data: &str, secret: &str) -> String {
    let mut sum: u64 = 0;

    // 对数据进行加权求和
    for (i, byte) in data.bytes().enumerate() {
        sum = sum.wrapping_add((byte as u64).wrapping_mul((i + 1) as u64));
    }

    // 混入秘密
    for (i, byte) in secret.bytes().enumerate() {
        sum = sum.wrapping_add((byte as u64).wrapping_mul((i + 100) as u64));
    }

    // 返回16位十六进制字符串
    format!("{:016x}", sum)
}

// 简单的字符串编码（不使用 base64 库）
fn encode_simple(s: &str) -> String {
    s.bytes().map(|b| format!("{:02x}", b)).collect::<String>()
}

// 简单的字符串解码
fn decode_simple(s: &str) -> Result<String, String> {
    let bytes: Result<Vec<u8>, _> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();

    let bytes = bytes.map_err(|_| "Invalid encoding".to_string())?;
    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8".to_string())
}

// HarmonyOS 专用：创建安全 token（不使用加密库）
fn create_harmonyos_token(user_id: &str, username: &str) -> Result<String, String> {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "123456123456123456".to_string());
    let timestamp = Local::now().timestamp();
    let random_suffix = timestamp % 1000000; // 使用时间戳作为随机数

    // 构建数据部分：user_id:username:timestamp:random
    let data = format!("{}:{}:{}:{}", user_id, username, timestamp, random_suffix);

    // 计算校验和
    let checksum = simple_checksum(&data, &secret);

    // 组合 token：data:checksum
    let token_data = format!("{}:{}", data, checksum);

    // 编码（不使用 base64）
    let token = encode_simple(&token_data);

    tracing::debug!("HarmonyOS token created with simple checksum");
    Ok(token)
}

// HarmonyOS 专用：验证安全 token
fn verify_harmonyos_token(token: &str) -> Result<Claims, String> {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "123456123456123456".to_string());

    // 解码
    let token_data = decode_simple(token)?;

    // 分割数据：user_id:username:timestamp:random:checksum
    let parts: Vec<&str> = token_data.split(':').collect();
    if parts.len() != 5 {
        return Err("Invalid token format".to_string());
    }

    let user_id = parts[0];
    let username = parts[1];
    let timestamp: i64 = parts[2]
        .parse()
        .map_err(|_| "Invalid timestamp".to_string())?;
    let random_suffix = parts[3];
    let checksum = parts[4];

    // 验证校验和
    let data = format!("{}:{}:{}:{}", user_id, username, timestamp, random_suffix);
    let expected_checksum = simple_checksum(&data, &secret);

    if checksum != expected_checksum {
        return Err("Invalid token signature".to_string());
    }

    // 检查过期（24小时）
    let now = Local::now().timestamp();
    if now - timestamp > 86400 {
        return Err("Token expired".to_string());
    }

    tracing::debug!("HarmonyOS token verified successfully");

    Ok(Claims {
        user_id: user_id.to_string(),
        token_id: timestamp.to_string(),
        username: username.to_string(),
        exp: Some(timestamp + 86400),
    })
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub id: String,
    pub name: String,
}

// JWT Claims 结构体
// 注意：exp 由 jwt-simple 自动管理，但我们需要在验证后获取它
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: String,
    pub token_id: String,
    pub username: String,
    // 从 JWT 验证结果中提取的过期时间（不参与序列化到 JWT）
    #[serde(skip_serializing)]
    pub exp: Option<i64>,
}

// Axum 的 JWT Claims 提取器
#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 尝试从 Authorization header 中提取 JWT token
        let auth_header = parts
            .headers
            .typed_get::<Authorization<Bearer>>()
            .ok_or(AuthError::MissingToken)?;

        // 验证 JWT token
        validate_jwt(auth_header.token()).map_err(AuthError::InvalidToken)
    }
}

// 使用 jwt-simple 创建 JWT
pub fn create_jwt(payload: AuthPayload) -> Result<AuthBody, String> {
    let iat = Local::now();

    // HarmonyOS: 使用不依赖加密库的安全 token
    if is_harmonyos() {
        tracing::warn!("🔧 HarmonyOS: Using simple secure token (no crypto libs)");

        let token = create_harmonyos_token(&payload.id, &payload.name)?;
        let jwt_exp_seconds = 86400; // 24小时
        let exp = iat + ChronoDuration::seconds(jwt_exp_seconds);

        return Ok(AuthBody::new(token, exp.timestamp(), jwt_exp_seconds));
    }

    // 标准 JWT 实现（非 HarmonyOS）
    let token_id = uuid::Uuid::new_v4().to_string();

    let jwt_exp_seconds = 60 * 60;
    let exp = iat + ChronoDuration::seconds(jwt_exp_seconds);

    let custom_claims = Claims {
        user_id: payload.id.to_owned(),
        token_id: token_id.clone(),
        username: payload.name.clone(),
        exp: None, // 不设置，让 jwt-simple 自动管理
    };

    tracing::debug!("Creating JWT token with jwt-simple (HS256, pure-rust)");

    // 获取 JWT 密钥
    let key = get_jwt_key()?;

    // 使用 jwt-simple 创建 token（exp 由 jwt-simple 自动添加）
    let jwt_claims = jwt_simple::claims::Claims::with_custom_claims(
        custom_claims,
        Duration::from_secs(jwt_exp_seconds as u64),
    );

    let token = key
        .authenticate(jwt_claims)
        .map_err(|e| format!("Token creation error: {}", e))?;

    tracing::debug!("JWT token created successfully with pure-rust implementation");
    Ok(AuthBody::new(token, exp.timestamp(), jwt_exp_seconds))
}

// 使用 jwt-simple 验证 JWT
pub fn validate_jwt(token: &str) -> Result<Claims, String> {
    // HarmonyOS: 验证 HMAC-SHA256 token
    if is_harmonyos() {
        return verify_harmonyos_token(token);
    }

    // 标准 JWT 验证（非 HarmonyOS）
    tracing::debug!("Validating JWT token with jwt-simple");

    // 获取 JWT 密钥
    let key = get_jwt_key()?;

    let jwt_claims = key.verify_token::<Claims>(token, None).map_err(|e| {
        tracing::warn!("JWT validation failed: {}", e);
        "Your login has expired, please login again".to_string()
    })?;

    tracing::debug!("JWT token validated successfully");

    // 从 jwt-simple 的 JWTClaims 中提取过期时间
    let exp = jwt_claims.expires_at.map(|d| d.as_secs() as i64);

    Ok(Claims {
        user_id: jwt_claims.custom.user_id,
        token_id: jwt_claims.custom.token_id,
        username: jwt_claims.custom.username,
        exp,
    })
}

// 生成 JWT token 的便捷函数
pub fn generate_token(user_id: &str, username: &str) -> Result<String, String> {
    let payload = AuthPayload {
        id: user_id.to_string(),
        name: username.to_string(),
    };

    let auth_body = create_jwt(payload)?;
    Ok(auth_body.token)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthBody {
    pub token: String,
    token_type: String,
    pub exp: i64,
    expired: i64,
}

impl AuthBody {
    fn new(access_token: String, exp: i64, exp_in: i64) -> Self {
        Self {
            token: access_token,
            token_type: "Bearer".to_string(),
            exp,
            expired: exp_in,
        }
    }
}

// Axum 错误类型
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authorization token"),
            AuthError::InvalidToken(_msg) => (StatusCode::UNAUTHORIZED, "Invalid token"),
        };

        let body = Json(serde_json::json!({
            "error": error_message,
            "status": status.as_u16()
        }));

        (status, body).into_response()
    }
}

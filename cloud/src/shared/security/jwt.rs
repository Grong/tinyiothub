use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use chrono::{Duration as ChronoDuration, Local};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};
use hmac::{Hmac, Mac};
use jwt_simple::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use tinyiothub_web::security::{AuthBody, Claims as WebClaims, AuthError as WebAuthError};

type HmacSha256 = Hmac<Sha256>;

/// Cloud-specific JWT claims with tenant and workspace isolation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: String,
    pub token_id: String,
    pub username: String,
    pub tenant_id: String,
    pub workspace_id: String,
    /// Expiration timestamp (seconds since epoch), extracted from JWT validation
    #[serde(skip_serializing)]
    pub exp: Option<i64>,
}

impl From<Claims> for WebClaims {
    fn from(claims: Claims) -> Self {
        WebClaims {
            user_id: claims.user_id,
            token_id: claims.token_id,
            username: claims.username,
            exp: claims.exp,
        }
    }
}

impl From<WebClaims> for Claims {
    fn from(web_claims: WebClaims) -> Self {
        Claims {
            user_id: web_claims.user_id,
            token_id: web_claims.token_id,
            username: web_claims.username,
            tenant_id: String::new(), // Default empty, caller should fill from JWT custom claims
            workspace_id: String::new(),
            exp: web_claims.exp,
        }
    }
}

// 获取 JWT 密钥的辅助函数 - 从统一配置读取
fn get_jwt_key() -> Result<HS256Key, String> {
    let secret = crate::shared::config::get().security.jwt.secret.clone();

    // 验证密钥长度
    if secret.len() < 32 {
        return Err(format!(
            "JWT secret is too short! Minimum 32 characters required, got {}",
            secret.len()
        ));
    }

    // 检查是否使用弱密钥
    if secret.len() < 64 {
        tracing::warn!(
            "⚠️  JWT secret is shorter than 64 characters, consider using a longer secret"
        );
    }

    Ok(HS256Key::from_bytes(secret.as_bytes()))
}

// 检查是否在 HarmonyOS 环境
fn is_harmonyos() -> bool {
    crate::shared::config::get().harmonyos.enabled
}

// ============================================================================
// HarmonyOS 专用：使用 HMAC-SHA256 的安全 token 实现
// ============================================================================

// 使用 HMAC-SHA256 计算消息认证码
fn hmac_sha256(message: &str, key: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    // 返回十六进制编码的 HMAC
    hex::encode(result.into_bytes())
}

// 简单的字符串编码（不使用 base64 库）
fn encode_simple(s: &str) -> String {
    s.bytes().map(|b| format!("{:02x}", b)).collect::<String>()
}

// 简单的字符串解码
fn decode_simple(s: &str) -> Result<String, String> {
    let bytes: Result<Vec<u8>, _> =
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16)).collect();

    let bytes = bytes.map_err(|_| "Invalid encoding".to_string())?;
    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8".to_string())
}

// HarmonyOS 专用：创建安全 token（使用 HMAC-SHA256）
fn create_harmonyos_token(user_id: &str, username: &str, tenant_id: &str) -> Result<String, String> {
    let secret = crate::shared::config::get().security.jwt.secret.clone();
    let timestamp = Local::now().timestamp();
    let random_suffix = timestamp % 1000000; // 使用时间戳作为随机数

    // 构建数据部分：user_id:username:tenant_id:timestamp:random
    let data = format!("{}:{}:{}:{}:{}", user_id, username, tenant_id, timestamp, random_suffix);

    // 计算 HMAC-SHA256 签名
    let signature = hmac_sha256(&data, &secret);

    // 组合 token：data:signature (hex encoded)
    let token_data = format!("{}:{}", data, signature);
    let token = encode_simple(&token_data);

    tracing::debug!("HarmonyOS token created with HMAC-SHA256");
    Ok(token)
}

// HarmonyOS 专用：验证安全 token（使用 HMAC-SHA256）
fn verify_harmonyos_token(token: &str) -> Result<Claims, String> {
    let secret = crate::shared::config::get().security.jwt.secret.clone();

    // 解码
    let token_data = decode_simple(token)?;

    // 分割数据：user_id:username:tenant_id:timestamp:random:signature
    let parts: Vec<&str> = token_data.split(':').collect();
    if parts.len() != 6 {
        return Err("Invalid token format".to_string());
    }

    let user_id = parts[0];
    let username = parts[1];
    let tenant_id = parts[2];
    let timestamp: i64 = parts[3].parse().map_err(|_| "Invalid timestamp".to_string())?;
    let random_suffix = parts[4];
    let signature = parts[5];

    // 验证 HMAC-SHA256 签名
    let data = format!("{}:{}:{}:{}:{}", user_id, username, tenant_id, timestamp, random_suffix);
    let expected_signature = hmac_sha256(&data, &secret);

    if signature != expected_signature {
        return Err("Invalid token signature".to_string());
    }

    // 检查过期（24小时）
    let now = Local::now().timestamp();
    if now - timestamp > 86400 {
        return Err("Token expired".to_string());
    }

    tracing::debug!("HarmonyOS token verified successfully with HMAC-SHA256");

    Ok(Claims {
        user_id: user_id.to_string(),
        token_id: timestamp.to_string(),
        username: username.to_string(),
        tenant_id: tenant_id.to_string(),
        workspace_id: String::new(),
        exp: Some(timestamp + 86400),
    })
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
}

// 使用 jwt-simple 创建 JWT
pub fn create_jwt(payload: AuthPayload) -> Result<AuthBody, String> {
    let iat = Local::now();

    // HarmonyOS: 使用不依赖加密库的安全 token
    if is_harmonyos() {
        tracing::warn!("🔧 HarmonyOS: Using simple secure token (no crypto libs)");

        let token = create_harmonyos_token(&payload.id, &payload.name, &payload.tenant_id)?;
        let jwt_exp_seconds = 86400; // 24小时
        let exp = iat + ChronoDuration::seconds(jwt_exp_seconds);

        return Ok(AuthBody::new(token, exp.timestamp(), jwt_exp_seconds));
    }

    // 标准 JWT 实现（非 HarmonyOS）
    let token_id = uuid::Uuid::new_v4().to_string();

    let jwt_exp_seconds = 60 * 60 * 24;
    let exp = iat + ChronoDuration::seconds(jwt_exp_seconds);

    let custom_claims = Claims {
        user_id: payload.id.to_owned(),
        token_id: token_id.clone(),
        username: payload.name.clone(),
        tenant_id: payload.tenant_id.clone(),
        workspace_id: String::new(),
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

    let token = key.authenticate(jwt_claims).map_err(|e| format!("Token creation error: {}", e))?;

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
        format!("JWT verification error: {}", e)
    })?;

    tracing::debug!("JWT token validated successfully");

    // 从 jwt-simple 的 JWTClaims 中提取过期时间
    let exp = jwt_claims.expires_at.map(|d| d.as_secs() as i64);

    Ok(Claims {
        user_id: jwt_claims.custom.user_id,
        token_id: jwt_claims.custom.token_id,
        username: jwt_claims.custom.username,
        tenant_id: jwt_claims.custom.tenant_id,
        workspace_id: jwt_claims.custom.workspace_id,
        exp,
    })
}

// 检查 token 是否在黑名单中（使用阻塞 DB 调用）
pub fn is_token_blacklisted_sync(
    db: &crate::shared::persistence::database::Database,
    token: &str,
) -> bool {
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));

    // Use try_with to check if we're in a Tokio runtime
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.block_on(async {
            let row = sqlx::query("SELECT 1 FROM token_blacklist WHERE token_hash = ? LIMIT 1")
                .bind(&token_hash)
                .fetch_optional(db.pool())
                .await;
            row.map(|r| r.is_some()).unwrap_or(false)
        })
    } else {
        false
    }
}

// 生成 JWT token 的便捷函数
pub fn generate_token(user_id: &str, username: &str, tenant_id: &str) -> Result<String, String> {
    let payload = AuthPayload {
        id: user_id.to_string(),
        name: username.to_string(),
        tenant_id: tenant_id.to_string(),
    };

    let auth_body = create_jwt(payload)?;
    Ok(auth_body.token)
}

/// 为 Cloud Claims 实现 FromRequestParts，使其可以直接在 handler 中作为 extractor 使用
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = WebAuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Try Authorization header first
        if let Some(auth_header) = parts.headers.typed_get::<Authorization<Bearer>>() {
            let token = auth_header.token();
            return validate_jwt(token).map_err(|e| WebAuthError::InvalidToken(e));
        }

        // Fallback: query string ?token=xxx (needed for EventSource which can't set headers)
        if let Some(query) = parts.uri.query() {
            for pair in query.split('&') {
                let mut kv = pair.splitn(2, '=');
                if kv.next() == Some("token") {
                    if let Some(token) = kv.next() {
                        return validate_jwt(token).map_err(|e| WebAuthError::InvalidToken(e));
                    }
                }
            }
        }

        Err(WebAuthError::MissingToken)
    }
}

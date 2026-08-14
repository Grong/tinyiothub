//! JWT 机制：HS256 签发/校验 + HarmonyOS HMAC 变体。
//! 构造注入（JwtService::new），零全局态（G2，替代原 OnceLock 设计）。
//! G4：axum extractor 与黑名单查询迁出 —— extractor 住 crates/web，业务查询住 apps/cloud。

use chrono::{Duration as ChronoDuration, Local};
use hmac::{Hmac, Mac};
use jwt_simple::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// 认证响应体（G4：自 crates/web 迁入，机制产物归属机制 crate）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthBody {
    pub token: String,
    pub token_type: String,
    pub exp: i64,
    pub expired: i64,
}

impl AuthBody {
    pub fn new(access_token: String, exp: i64, exp_in: i64) -> Self {
        Self {
            token: access_token,
            token_type: "Bearer".to_string(),
            exp,
            expired: exp_in,
        }
    }
}

/// JWT settings — constructor-injected, no globals (G2).
#[derive(Debug, Clone)]
pub struct JwtSettings {
    pub secret: String,
    pub harmonyos_enabled: bool,
}

/// JWT 机制服务：签发/校验。构造注入，零全局态。
#[derive(Debug, Clone)]
pub struct JwtService {
    settings: JwtSettings,
}

impl JwtService {
    /// Create from startup settings.
    pub fn new(settings: JwtSettings) -> Self {
        Self { settings }
    }

    fn jwt_settings(&self) -> &JwtSettings {
        &self.settings
    }
}

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

#[derive(Debug, Deserialize, Clone)]
pub struct AuthPayload {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub workspace_id: String,
}

impl JwtService {
    // 获取 JWT 密钥的辅助函数 - 从启动时注册的 JWT 设置读取
    fn get_jwt_key(&self) -> Result<HS256Key, String> {
        let secret = self.jwt_settings().secret.clone();

        // 验证密钥长度
        if secret.len() < 32 {
            return Err(format!(
                "JWT secret is too short! Minimum 32 characters required, got {}",
                secret.len()
            ));
        }

        Ok(HS256Key::from_bytes(secret.as_bytes()))
    }

    // 检查是否在 HarmonyOS 环境
    fn is_harmonyos(&self) -> bool {
        self.jwt_settings().harmonyos_enabled
    }

    // ============================================================================
    // HarmonyOS 专用：使用 HMAC-SHA256 的安全 token 实现
    // ============================================================================

    // 使用 HMAC-SHA256 计算消息认证码
    fn hmac_sha256(message: &str, key: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let result = mac.finalize();
        // 返回十六进制编码的 HMAC
        result.into_bytes().iter().map(|b| format!("{b:02x}")).collect()
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

    // HarmonyOS 专用：创建安全 token（使用 HMAC-SHA256）
    fn create_harmonyos_token(
        &self,
        user_id: &str,
        username: &str,
        tenant_id: &str,
        workspace_id: &str,
    ) -> Result<String, String> {
        let secret = self.jwt_settings().secret.clone();
        let timestamp = Local::now().timestamp();
        let random_suffix = timestamp % 1000000; // 使用时间戳作为随机数

        // 构建数据部分：user_id:username:tenant_id:workspace_id:timestamp:random
        let data = format!(
            "{}:{}:{}:{}:{}:{}",
            user_id, username, tenant_id, workspace_id, timestamp, random_suffix
        );

        // 计算 HMAC-SHA256 签名
        let signature = Self::hmac_sha256(&data, &secret);

        // 组合 token：data:signature (hex encoded)
        let token_data = format!("{}:{}", data, signature);
        let token = Self::encode_simple(&token_data);

        Ok(token)
    }

    // HarmonyOS 专用：验证安全 token（使用 HMAC-SHA256）
    fn verify_harmonyos_token(&self, token: &str) -> Result<Claims, String> {
        let secret = self.jwt_settings().secret.clone();

        // 解码
        let token_data = Self::decode_simple(token)?;

        // 分割数据：
        //   新格式(7部分): user_id:username:tenant_id:workspace_id:timestamp:random:signature
        //   旧格式(6部分): user_id:username:tenant_id:timestamp:random:signature
        let parts: Vec<&str> = token_data.split(':').collect();
        let (user_id, username, tenant_id, workspace_id, timestamp_str, random_suffix, signature) = match parts.len() {
            7 => (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5], parts[6]),
            6 => (parts[0], parts[1], parts[2], "", parts[3], parts[4], parts[5]),
            _ => return Err("Invalid token format".to_string()),
        };

        let timestamp: i64 = timestamp_str.parse().map_err(|_| "Invalid timestamp".to_string())?;

        // 验证 HMAC-SHA256 签名
        let data = if workspace_id.is_empty() {
            format!("{}:{}:{}:{}:{}", user_id, username, tenant_id, timestamp, random_suffix)
        } else {
            format!(
                "{}:{}:{}:{}:{}:{}",
                user_id, username, tenant_id, workspace_id, timestamp, random_suffix
            )
        };
        let expected_signature = Self::hmac_sha256(&data, &secret);

        if signature != expected_signature {
            return Err("Invalid token signature".to_string());
        }

        // 检查过期（24小时）
        let now = Local::now().timestamp();
        if now - timestamp > 86400 {
            return Err("Token expired".to_string());
        }

        Ok(Claims {
            user_id: user_id.to_string(),
            token_id: timestamp.to_string(),
            username: username.to_string(),
            tenant_id: tenant_id.to_string(),
            workspace_id: workspace_id.to_string(),
            exp: Some(timestamp + 86400),
        })
    }

    // 使用 jwt-simple 创建 JWT
    pub fn create_jwt(&self, payload: AuthPayload) -> Result<AuthBody, String> {
        let iat = Local::now();

        // HarmonyOS: 使用不依赖加密库的安全 token
        if self.is_harmonyos() {
            let token =
                self.create_harmonyos_token(&payload.id, &payload.name, &payload.tenant_id, &payload.workspace_id)?;
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
            workspace_id: payload.workspace_id.clone(),
            exp: None, // 不设置，让 jwt-simple 自动管理
        };

        // 获取 JWT 密钥
        let key = self.get_jwt_key()?;

        // 使用 jwt-simple 创建 token（exp 由 jwt-simple 自动添加）
        let jwt_claims =
            jwt_simple::claims::Claims::with_custom_claims(custom_claims, Duration::from_secs(jwt_exp_seconds as u64));

        let token = key
            .authenticate(jwt_claims)
            .map_err(|e| format!("Token creation error: {}", e))?;

        Ok(AuthBody::new(token, exp.timestamp(), jwt_exp_seconds))
    }

    // 使用 jwt-simple 验证 JWT
    pub fn validate_jwt(&self, token: &str) -> Result<Claims, String> {
        // HarmonyOS: 验证 HMAC-SHA256 token
        if self.is_harmonyos() {
            return self.verify_harmonyos_token(token);
        }

        // 标准 JWT 验证（非 HarmonyOS）
        // 获取 JWT 密钥
        let key = self.get_jwt_key()?;

        let jwt_claims = key
            .verify_token::<Claims>(token, None)
            .map_err(|e| format!("JWT verification error: {}", e))?;

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

    // 生成 JWT token 的便捷函数
    pub fn generate_token(
        &self,
        user_id: &str,
        username: &str,
        tenant_id: &str,
        workspace_id: &str,
    ) -> Result<String, String> {
        let payload = AuthPayload {
            id: user_id.to_string(),
            name: username.to_string(),
            tenant_id: tenant_id.to_string(),
            workspace_id: workspace_id.to_string(),
        };

        let auth_body = self.create_jwt(payload)?;
        Ok(auth_body.token)
    }
}

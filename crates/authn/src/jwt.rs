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
        // CEO review T7：奇数长度输入下 `&s[i..i+2]` 越界 panic——
        // 畸形 token 绝不能在认证路径上炸掉请求任务。
        if s.len() % 2 != 0 {
            return Err("Invalid encoding".to_string());
        }
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

#[cfg(test)]
mod tests {
    //! CEO review T7：jwt.rs 此前零测试——decode_simple 手写 hex 切分、
    //! HarmonyOS 自定义 token 格式均为安全敏感路径，必须有 round-trip/篡改/过期覆盖。

    use super::*;

    const SECRET: &str = "test-secret-key-at-least-32-chars-long";

    fn service(harmonyos: bool) -> JwtService {
        JwtService::new(JwtSettings {
            secret: SECRET.to_string(),
            harmonyos_enabled: harmonyos,
        })
    }

    fn payload() -> AuthPayload {
        AuthPayload {
            id: "user-1".to_string(),
            name: "alice".to_string(),
            tenant_id: "tenant-1".to_string(),
            workspace_id: "ws-1".to_string(),
        }
    }

    // ---- decode_simple / encode_simple ----

    #[test]
    fn simple_codec_round_trip_including_multibyte() {
        for s in ["", "hello", "用户:工作区:1", "a:b:c:123:456"] {
            let encoded = JwtService::encode_simple(s);
            let decoded = JwtService::decode_simple(&encoded).expect("round trip");
            assert_eq!(decoded, s);
        }
    }

    #[test]
    fn decode_simple_rejects_odd_length_without_panic() {
        // 奇数长度曾是越界 panic（认证路径拒绝服务面）。
        assert!(JwtService::decode_simple("abc").is_err());
        assert!(JwtService::decode_simple("0").is_err());
    }

    #[test]
    fn decode_simple_rejects_non_hex_and_invalid_utf8() {
        assert!(JwtService::decode_simple("zz").is_err());
        // 0xFF 不是合法 UTF-8 起始字节。
        assert!(JwtService::decode_simple("ff").is_err());
    }

    // ---- 标准 JWT（jwt-simple）----

    #[test]
    fn standard_jwt_round_trip() {
        let svc = service(false);
        let body = svc.create_jwt(payload()).expect("create");
        let claims = svc.validate_jwt(&body.token).expect("validate");
        assert_eq!(claims.user_id, "user-1");
        assert_eq!(claims.username, "alice");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.workspace_id, "ws-1");
        assert!(claims.exp.is_some());
    }

    #[test]
    fn standard_jwt_rejects_tampered_token() {
        let svc = service(false);
        let mut token = svc
            .generate_token("user-1", "alice", "tenant-1", "ws-1")
            .expect("create");
        // 翻转签名区一个字符。
        let pos = token.len() - 3;
        let replacement = if token.as_bytes()[pos] == b'a' { 'b' } else { 'a' };
        token.replace_range(pos..pos + 1, &replacement.to_string());
        assert!(svc.validate_jwt(&token).is_err());
    }

    #[test]
    fn standard_jwt_rejects_wrong_secret() {
        let svc = service(false);
        let token = svc
            .generate_token("user-1", "alice", "tenant-1", "ws-1")
            .expect("create");
        let other = JwtService::new(JwtSettings {
            secret: "another-secret-key-at-least-32-chars".to_string(),
            harmonyos_enabled: false,
        });
        assert!(other.validate_jwt(&token).is_err());
    }

    #[test]
    fn short_secret_is_rejected() {
        let svc = JwtService::new(JwtSettings {
            secret: "too-short".to_string(),
            harmonyos_enabled: false,
        });
        assert!(svc.create_jwt(payload()).is_err());
    }

    // ---- HarmonyOS token ----

    #[test]
    fn harmonyos_token_round_trip() {
        let svc = service(true);
        let token = svc
            .generate_token("user-1", "alice", "tenant-1", "ws-1")
            .expect("create");
        let claims = svc.validate_jwt(&token).expect("validate");
        assert_eq!(claims.user_id, "user-1");
        assert_eq!(claims.username, "alice");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.workspace_id, "ws-1");
    }

    #[test]
    fn harmonyos_token_rejects_tampering() {
        let svc = service(true);
        let token = svc
            .generate_token("user-1", "alice", "tenant-1", "ws-1")
            .expect("create");
        // 篡改数据区前几个 hex 字符（user_id 字段）。
        let tampered = format!("{}{}", "00", &token[2..]);
        assert!(svc.validate_jwt(&tampered).is_err());
    }

    #[test]
    fn harmonyos_token_rejects_wrong_secret() {
        let svc = service(true);
        let token = svc
            .generate_token("user-1", "alice", "tenant-1", "ws-1")
            .expect("create");
        let other = JwtService::new(JwtSettings {
            secret: "another-secret-key-at-least-32-chars".to_string(),
            harmonyos_enabled: true,
        });
        assert!(other.validate_jwt(&token).is_err());
    }

    #[test]
    fn harmonyos_token_rejects_expired() {
        let svc = service(true);
        // 手工构造一个 25 小时前的 token（新 7 段格式）。
        let old_ts = Local::now().timestamp() - 90_000;
        let data = format!("user-1:alice:tenant-1:ws-1:{}:12345", old_ts);
        let signature = JwtService::hmac_sha256(&data, SECRET);
        let token = JwtService::encode_simple(&format!("{}:{}", data, signature));
        let err = svc.validate_jwt(&token).expect_err("expired token must fail");
        assert!(err.contains("expired"), "expected expiry error, got: {err}");
    }

    #[test]
    fn harmonyos_token_accepts_legacy_6_part_format() {
        let svc = service(true);
        // 旧格式（无 workspace_id）：user:tenant:timestamp:random:signature。
        let ts = Local::now().timestamp();
        let data = format!("user-1:alice:tenant-1:{}:12345", ts);
        let signature = JwtService::hmac_sha256(&data, SECRET);
        let token = JwtService::encode_simple(&format!("{}:{}", data, signature));
        let claims = svc.validate_jwt(&token).expect("legacy format must still validate");
        assert_eq!(claims.user_id, "user-1");
        assert_eq!(claims.workspace_id, "");
    }

    #[test]
    fn harmonyos_token_rejects_malformed_without_panic() {
        let svc = service(true);
        assert!(svc.validate_jwt("not-a-token").is_err());
        assert!(svc.validate_jwt("abc").is_err()); // 奇数长度 hex
        assert!(svc.validate_jwt("").is_err());
    }
}

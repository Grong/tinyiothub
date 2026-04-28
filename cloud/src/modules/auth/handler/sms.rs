#![allow(dead_code)]
// 短信验证码认证模块
// 支持手机验证码登录/注册

use tinyiothub_web::response::ApiResponseBuilder;
use axum::{
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    shared::app_state::AppState,
    shared::api_response::ApiResponse,
    shared::{config::get as get_config, redis::RedisClient},
    shared::security::jwt,
};

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
    pub captcha_ticket: Option<String>,   // 腾讯防水墙票据
    pub captcha_randstr: Option<String>,  // 腾讯防水墙随机串
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
    pub workspace_id: Option<String>,
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

// ============== 频率限制 ==============

enum RateLimitResult {
    Allowed,
    NeedsWait(i64),
    DailyLimitExceeded,
    NeedsCaptcha,
}

/// 检查发送频率限制
async fn check_rate_limit(
    redis: &Option<RedisClient>,
    phone: &str,
    ip: Option<&str>,
) -> Result<RateLimitResult, StatusCode> {
    let config = get_config();
    let rate_limit = config.sms.rate_limit.as_ref();

    let interval_secs = rate_limit.and_then(|r| r.interval_secs).unwrap_or(90) as i64;
    let daily_limit = rate_limit.and_then(|r| r.daily_limit).unwrap_or(5) as i64;

    let redis = match redis {
        Some(r) => r,
        None => return Ok(RateLimitResult::Allowed), // 无 Redis 时跳过检查（仅用于测试）
    };

    // 检查同手机号发送间隔
    let interval_key = format!("sms:interval:{}", phone);
    if let Ok(Some(_)) = redis.get(&interval_key).await {
        return Ok(RateLimitResult::NeedsWait(interval_secs));
    }

    // 检查同手机号当日发送次数
    let daily_key = format!("sms:count:daily:{}", phone);
    if let Ok(count) = redis.get(&daily_key).await
        && let Ok(c) = count.unwrap_or_default().parse::<i64>()
            && c >= daily_limit {
                return Ok(RateLimitResult::DailyLimitExceeded);
            }

    // 检查同 IP 5分钟内发送次数
    if let Some(ip_addr) = ip {
        let ip_key = format!("sms:count:ip:{}", ip_addr);
        if let Ok(count) = redis.get(&ip_key).await
            && let Ok(c) = count.unwrap_or_default().parse::<i64>()
                && c >= 3 {
                    return Ok(RateLimitResult::NeedsCaptcha);
                }
    }

    Ok(RateLimitResult::Allowed)
}

// ============== CAPTCHA 验证 ==============

/// 验证腾讯防水墙票据
async fn verify_captcha(_ticket: &str, _randstr: &str, _ip: &str) -> Result<bool, StatusCode> {
    let config = get_config();
    let captcha_config = match config.sms.captcha.as_ref() {
        Some(c) if c.enabled => c,
        None | Some(_) => return Ok(true), // 未配置或未启用时跳过
    };

    // TODO: 腾讯防水墙 CAPTCHA 验证需要 app_id 和 app_secret
    // 当前仅当 enabled=true 时验证通过（后续完善）
    tracing::warn!("[CAPTCHA] Tencent CAPTCHA verification not fully implemented");
    Ok(captcha_config.enabled)
}

// ============== 阿里云 SMS ==============

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// 调用阿里云 SMS API 发送短信
async fn send_aliyun_sms(
    phone: &str,
    code: &str,
    config: &tinyiothub_config::AliyunSmsConfig,
) -> Result<(), String> {
    let endpoint = "https://dysmsapi.aliyuncs.com/";
    let action = "SendSms";
    let version = "2017-05-25";
    let region_id = "cn-hangzhou";

    // 生成签名随机数和时间戳
    let signature_nonce = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let template_param = format!("{{\"code\":\"{}\"}}", code);

    // 构建请求参数（按字母顺序排序）
    let mut params: Vec<(&str, &str)> = vec![
        ("AccessKeyId", config.access_key_id.as_str()),
        ("Action", action),
        ("Format", "JSON"),
        ("PhoneNumbers", phone),
        ("RegionId", region_id),
        ("SignName", config.sign_name.as_str()),
        ("SignatureMethod", "HMAC-SHA1"),
        ("SignatureNonce", signature_nonce.as_str()),
        ("SignatureVersion", "1.0"),
        ("TemplateCode", config.template_code.as_str()),
        ("TemplateParam", template_param.as_str()),
        ("Timestamp", timestamp.as_str()),
        ("Version", version),
    ];

    // 按 key 排序
    params.sort_by(|a, b| a.0.cmp(b.0));

    // 构建规范的查询字符串
    let canonical_query_string: String = params
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                percent_encode(k),
                percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    // 构建待签名字符串
    let string_to_sign = format!(
        "GET&{}&{}",
        percent_encode("/"),
        percent_encode(canonical_query_string.as_str())
    );

    // 计算 HMAC-SHA1 签名
    let mut mac = HmacSha1::new_from_slice(format!("{}&", config.access_key_secret).as_bytes())
        .map_err(|e| format!("HMAC error: {}", e))?;
    mac.update(string_to_sign.as_bytes());
    let signature = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, mac.finalize().into_bytes());

    // 构建完整 URL
    let url = format!(
        "{}?Signature={}&{}",
        endpoint,
        percent_encode(signature.as_str()),
        canonical_query_string
    );

    tracing::debug!("[SMS] Sending request to Aliyun: {}", url.replace(&config.access_key_secret, "***"));

    // 发送请求
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    #[derive(serde::Deserialize)]
    struct AliyunResponse {
        pub code: String,
        pub message: String,
    }

    let result: AliyunResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if result.code == "OK" {
        tracing::info!(
            "[SMS] Successfully sent code to {} via Aliyun",
            phone
        );
        Ok(())
    } else {
        tracing::error!(
            "[SMS] Aliyun API error: {} - {}",
            result.code,
            result.message
        );
        Err(format!("{}: {}", result.code, result.message))
    }
}

/// URL 百分号编码（Aliyun 风格）
fn percent_encode(s: &str) -> String {
    let encoded: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '~' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect();
    encoded
}

// ============== 路由处理函数 ==============

/// 发送验证码
async fn send_code(
    State(state): State<AppState>,
    ConnectInfo(ip_addr): ConnectInfo<SocketAddr>,
    Json(request): Json<SendCodeRequest>,
) -> Json<ApiResponse<SendCodeResponse>> {
    // 检查 SMS 是否启用
    let config = get_config();
    if !config.sms.enabled {
        return ApiResponseBuilder::error("短信服务未启用".to_string());
    }

    let phone = request.phone.trim();

    // 验证手机号格式
    if !validate_phone(phone) {
        return ApiResponseBuilder::error("手机号格式不正确".to_string());
    }

    let purpose = request.purpose.unwrap_or_else(|| "login".to_string());
    let ip_str = ip_addr.to_string();

    // 频率限制检查
    match check_rate_limit(&state.redis, phone, Some(&ip_str)).await {
        Ok(RateLimitResult::NeedsWait(secs)) => {
            return ApiResponseBuilder::error(format!("操作太频繁，请 {} 秒后重试", secs));
        }
        Ok(RateLimitResult::DailyLimitExceeded) => {
            return ApiResponseBuilder::error("今日发送次数已用完，请明天再试".to_string());
        }
        Ok(RateLimitResult::NeedsCaptcha) => {
            return ApiResponseBuilder::error_with_code(1001, "请先完成验证".to_string());
        }
        Ok(RateLimitResult::Allowed) => {}
        Err(_) => return ApiResponseBuilder::error("系统错误".to_string()),
    }

    // CAPTCHA 验证（如果频率异常，需要验证）
    if let Some(ticket) = &request.captcha_ticket {
        let randstr = request.captcha_randstr.as_deref().unwrap_or("");
        match verify_captcha(ticket, randstr, &ip_str).await {
            Ok(true) => {}
            Ok(false) => {
                return ApiResponseBuilder::error("验证失败，请重试".to_string());
            }
            Err(_) => {
                return ApiResponseBuilder::error("验证服务异常".to_string());
            }
        }
    }

    // 生成验证码
    let code = generate_code();

    // 使用 Redis 存储验证码（优先）
    let redis = state.redis.as_ref();

    if let Some(r) = redis {
        // 存储验证码到 Redis（5分钟过期）
        let code_key = format!("sms:code:{}", phone);
        if let Err(e) = r.set_ex(&code_key, &code, CODE_EXPIRE_SECONDS).await {
            tracing::error!("Failed to store SMS code in Redis: {}", e);
            // 不阻止流程，继续使用数据库存储
        }

        // 设置发送间隔（90秒）
        let interval_key = format!("sms:interval:{}", phone);
        let interval_secs = config
            .sms
            .rate_limit
            .as_ref()
            .and_then(|r| r.interval_secs)
            .unwrap_or(90);
        if let Err(e) = r.set_ex(&interval_key, "1", interval_secs).await {
            tracing::error!("Failed to set rate limit interval: {}", e);
        }

        // 增加当日计数（incr 原子创建/递增，expire 仅设置 TTL 不覆盖值）
        let daily_key = format!("sms:count:daily:{}", phone);
        if let Ok(_count) = r.incr(&daily_key).await {
            if let Err(e) = r.expire(&daily_key, 86400).await {
                tracing::error!("Failed to set daily counter expiry: {}", e);
            }
        }

        // 增加 IP 计数
        let ip_key = format!("sms:count:ip:{}", ip_str);
        match r.incr(&ip_key).await {
            Ok(_count) => {
                if let Err(e) = r.expire(&ip_key, 300).await {
                    tracing::error!("Failed to set IP counter expiry: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to increment IP counter: {}", e);
            }
        }
    } else {
        // 无 Redis 时降级到数据库存储
        let db = state.database();
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::seconds(CODE_EXPIRE_SECONDS as i64);

        let id = uuid::Uuid::new_v4().to_string();
        if let Err(e) = sqlx::query(
            r#"INSERT INTO sms_codes (id, phone, code, purpose, expires_at)
                VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(phone)
        .bind(&code)
        .bind(&purpose)
        .bind(expires_at.to_rfc3339())
        .execute(db.pool())
        .await
        {
            tracing::error!("Failed to save SMS code: {}", e);
            return ApiResponseBuilder::error("发送失败，请稍后重试".to_string());
        }
    }

    // 返回成功响应
    // 在 debug 模式或未配置阿里云 SMS 时，将验证码记录到日志（开发/测试用）
    #[cfg(debug_assertions)]
    {
        tracing::info!("[TEST] SMS code for {}: {}", phone, code);
        return ApiResponseBuilder::success(SendCodeResponse {
            expires_in: CODE_EXPIRE_SECONDS,
            message: format!("验证码已发送（测试模式: {}）", code),
        });
    }

    #[cfg(not(debug_assertions))]
    {
        if let Some(aliyun_config) = &config.sms.aliyun {
            match send_aliyun_sms(phone, &code, aliyun_config).await {
                Ok(_) => ApiResponseBuilder::success(SendCodeResponse {
                    expires_in: CODE_EXPIRE_SECONDS,
                    message: "验证码已发送".to_string(),
                }),
                Err(e) => {
                    tracing::error!("Failed to send SMS: {}", e);
                    ApiResponseBuilder::error("发送失败，请稍后重试".to_string())
                }
            }
        } else {
            // 未配置阿里云 SMS：记录验证码到日志用于调试，提示用户配置 SMS
            tracing::warn!("[SMS] Aliyun SMS not configured — code for {}: {}", phone, code);
            ApiResponseBuilder::error("短信服务未配置，请联系管理员".to_string())
        }
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
        return ApiResponseBuilder::error("手机号格式不正确".to_string());
    }

    let redis = state.redis.as_ref();

    // 从 Redis 获取验证码
    let stored_code = match redis {
        Some(r) => {
            let code_key = format!("sms:code:{}", phone);
            r.get(&code_key).await.ok().flatten()
        }
        None => {
            // Fallback to DB（仅用于开发/测试）
            get_code_from_db(&state, phone).await
        }
    };

    let stored_code = match stored_code {
        Some(c) => c,
        None => return ApiResponseBuilder::error("验证码已过期，请重新获取".to_string()),
    };

    // 验证码比较
    use subtle::ConstantTimeEq;
    if !bool::from(stored_code.as_bytes().ct_eq(code.as_bytes())) {
        // 增加错误计数（原子 incr 避免并发问题）
        if let Some(r) = redis {
            let fail_key = format!("sms:verify:fail:{}", phone);
            let fail_count = match r.incr(&fail_key).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to increment fail count: {}", e);
                    1
                }
            };
            // 首次 incr 时设置过期时间
            if fail_count == 1 {
                if let Err(e) = r.expire(&fail_key, 300).await {
                    tracing::error!("Failed to set fail count expiry: {}", e);
                }
            }

            if fail_count >= 3 {
                // 错误次数过多，删除验证码
                let code_key = format!("sms:code:{}", phone);
                if let Err(e) = r.del(&code_key).await {
                    tracing::error!("Failed to delete code after too many failures: {}", e);
                }
                return ApiResponseBuilder::error("验证码错误次数过多，请重新获取".to_string());
            }
        }
        return ApiResponseBuilder::error("验证码错误".to_string());
    }

    // 验证成功，删除验证码
    if let Some(r) = redis {
        let code_key = format!("sms:code:{}", phone);
        if let Err(e) = r.del(&code_key).await {
            tracing::error!("Failed to delete code after successful verification: {}", e);
        }
    }

    // 查找或创建用户（复用现有逻辑）
    let db = state.database();
    let user = match find_or_create_user_by_phone(db, phone).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to find or create user: {}", e);
            return ApiResponseBuilder::error("登录失败，请稍后重试".to_string());
        }
    };

    // 查找该用户关联的租户和默认 workspace
    let tenant_id: String = sqlx::query_scalar(
        "SELECT tenant_id FROM tenant_users WHERE user_id = ? LIMIT 1"
    )
    .bind(&user.id)
    .fetch_optional(db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "default".to_string());

    let workspace_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE tenant_id = ? LIMIT 1"
    )
    .bind(&tenant_id)
    .fetch_optional(db.pool())
    .await
    .unwrap_or(None);

    let workspace_id_for_token = workspace_id.clone().unwrap_or_default();
    let token = match jwt::generate_token(&user.id, &user.username, &tenant_id, &workspace_id_for_token) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to generate token: {}", e);
            return ApiResponseBuilder::error("登录失败，请稍后重试".to_string());
        }
    };

    ApiResponseBuilder::success(LoginWithCodeResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: 86400,
        user_info: UserInfo {
            id: user.id,
            phone: user.phone.unwrap_or_default(),
            username: Some(user.username),
            display_name: user.display_name,
        },
        workspace_id,
    })
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
        return ApiResponseBuilder::error("手机号格式不正确".to_string());
    }

    if code.is_empty() {
        return ApiResponseBuilder::error("验证码不能为空".to_string());
    }

    // 先从 Redis 获取验证码（与 send_code 保持一致）
    let stored_code = if let Some(r) = &state.redis {
        let code_key = format!("sms:code:{}", phone);
        r.get(&code_key).await.ok().flatten()
    } else {
        None
    };

    // Redis 未命中时 fallback 到数据库
    let stored_code = match stored_code {
        Some(c) => c,
        None => match get_code_from_db(&state, &phone).await {
            Some(c) => c,
            None => {
                return ApiResponseBuilder::success(VerifyCodeResponse {
                    valid: false,
                    message: "验证码不存在或已失效".to_string(),
                });
            }
        },
    };

    // 比较验证码 — 使用常量时间比较防止时序攻击
    use subtle::ConstantTimeEq;
    let is_valid = bool::from(stored_code.as_bytes().ct_eq(code.as_bytes()));

    ApiResponseBuilder::success(VerifyCodeResponse {
        valid: is_valid,
        message: if is_valid {
            "验证码验证成功".to_string()
        } else {
            "验证码错误".to_string()
        },
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

/// 从数据库获取验证码（Redis 不可用时的 fallback）
async fn get_code_from_db(
    state: &AppState,
    phone: &str,
) -> Option<String> {
    let db = state.database();

    let rows = match sqlx::query(
        r#"SELECT code, expires_at FROM sms_codes
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
            tracing::error!("Database error when fetching code: {}", e);
            return None;
        }
    };

    if rows.is_empty() {
        return None;
    }

    let row = &rows[0];

    // 获取存储的验证码
    let stored_code: String = match row.try_get("code") {
        Ok(c) => c,
        Err(_) => return None,
    };

    // 获取过期时间
    let expires_at: String = match row.try_get("expires_at") {
        Ok(e) => e,
        Err(_) => return None,
    };

    // 检查验证码是否过期
    if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&expires_at)
        && exp < chrono::Utc::now() {
            return None;
        }

    Some(stored_code)
}

/// 验证手机号格式（中国大陆手机号）
fn validate_phone(phone: &str) -> bool {
    // 简单验证：11位数字，以1开头
    phone.len() == 11 && phone.starts_with('1') && phone.chars().all(|c| c.is_ascii_digit())
}

/// 生成随机验证码
fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

/// 根据手机号查找或创建用户（原子操作，防止并发重复创建）
async fn find_or_create_user_by_phone(
    db: &crate::shared::persistence::Database,
    phone: &str,
) -> Result<crate::modules::user::User, Box<dyn std::error::Error + Send + Sync>> {
    // 最多重试 3 次，处理并发创建导致的唯一约束冲突
    for attempt in 0..3 {
        // 查找现有用户
        let rows = sqlx::query("SELECT * FROM users WHERE phone = ? LIMIT 1")
            .bind(phone)
            .fetch_all(db.pool())
            .await?;

        if let Some(row) = rows.into_iter().next() {
            return Ok(crate::modules::user::User {
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

        let insert_result = sqlx::query(
            r#"INSERT INTO users (id, username, phone, is_enabled, created_at, updated_at)
                SELECT ?, ?, ?, 1, ?, ?
                WHERE NOT EXISTS (SELECT 1 FROM users WHERE phone = ?)"#,
        )
        .bind(&user_id)
        .bind(phone)
        .bind(phone)
        .bind(&now)
        .bind(&now)
        .bind(phone)
        .execute(db.pool())
        .await;

        match insert_result {
            Ok(result) => {
                if result.rows_affected() > 0 {
                    return Ok(crate::modules::user::User {
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
                    });
                }
                // rows_affected == 0 说明并发请求已经创建了该用户，重试 SELECT
                if attempt < 2 {
                    tracing::warn!(
                        "[SMS] User already created concurrently for phone={}, retrying SELECT (attempt {})",
                        phone,
                        attempt + 2
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to insert user for phone={}: {}", phone, e);
                return Err(Box::new(e));
            }
        }
    }

    // 最后一次重试：直接 SELECT
    let rows = sqlx::query("SELECT * FROM users WHERE phone = ? LIMIT 1")
        .bind(phone)
        .fetch_all(db.pool())
        .await?;

    if let Some(row) = rows.into_iter().next() {
        Ok(crate::modules::user::User {
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
        })
    } else {
        Err("Failed to create or find user after retries".into())
    }
}

// ============== 单元测试 ==============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_phone_valid() {
        // 有效手机号
        assert!(validate_phone("13812345678"));
        assert!(validate_phone("15912345678"));
        assert!(validate_phone("19912345678"));
        assert!(validate_phone("16612345678"));
    }

    #[test]
    fn test_validate_phone_invalid() {
        // 无效手机号 - 长度不对
        assert!(!validate_phone("1381234567"));   // 10位
        assert!(!validate_phone("138123456789")); // 12位
        assert!(!validate_phone(""));              // 空

        // 无效手机号 - 不是1开头
        assert!(!validate_phone("23812345678"));
        assert!(!validate_phone("13812345678".replace("1", "a").as_str()));

        // 无效手机号 - 包含非数字
        assert!(!validate_phone("1381234567a"));
        assert!(!validate_phone("138123456!"));
        assert!(!validate_phone("138 1234 5678")); // 有空格
    }

    #[test]
    fn test_generate_code_format() {
        for _ in 0..100 {
            let code = generate_code();
            // 验证码应该是6位数字
            assert_eq!(code.len(), 6, "Code {} should be 6 digits", code);
            assert!(code.chars().all(|c| c.is_ascii_digit()), "Code {} should be all digits", code);
        }
    }

    #[test]
    fn test_generate_code_range() {
        for _ in 0..100 {
            let code = generate_code();
            let num: u32 = code.parse().unwrap();
            assert!(num < 1_000_000, "Code {} should be < 1000000", num);
        }
    }

    #[test]
    fn test_percent_encode_normal_chars() {
        // 普通字符不编码
        assert_eq!(percent_encode("abc123"), "abc123");
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("test-value.test"), "test-value.test");
    }

    #[test]
    fn test_percent_encode_special_chars() {
        // 特殊字符需要编码
        assert_eq!(percent_encode(" "), "%20");
        assert_eq!(percent_encode("&"), "%26");
        assert_eq!(percent_encode("="), "%3D");
        assert_eq!(percent_encode("%"), "%25");
        assert_eq!(percent_encode("/"), "%2F");
    }

    #[test]
    fn test_percent_encode_chinese() {
        // 中文需要编码
        let result = percent_encode("测试");
        assert!(result.starts_with("%"));
    }

    // RateLimitResult variants tested implicitly via integration tests
}

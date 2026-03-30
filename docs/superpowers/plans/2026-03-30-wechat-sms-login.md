# 微信登录与手机验证码登录实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现微信 Web 扫码登录和手机验证码登录，集成阿里云 SMS 和腾讯防水墙

**Architecture:**
- 后端：Rust/Axum，在现有 `api/src/api/auth/sms.rs` 和 `social.rs` 基础上扩展
- 前端：Next.js 15/React Query，扩展 `web/service/auth.ts` 和登录页面
- 缓存：Redis（用于短信验证码存储、频率限制、微信 state 参数）
- 第三方：阿里云 SMS（短信）、腾讯防水墙（人机验证）、微信开放平台 OAuth

**Tech Stack:** Rust (Tokio, SQLx, Redis), Next.js 15 (React Query, TailwindCSS), Redis, 阿里云 SMS SDK

---

## 一、文件结构

```
api/src/
├── api/auth/
│   ├── sms.rs              # 改造：添加 Redis 频率限制、CAPTCHA 验证、阿里云 SMS 集成
│   ├── social.rs           # 改造：完成微信 code 换取 openid、state CSRF 保护
│   └── mod.rs              # 无需改动
├── infrastructure/
│   ├── config/
│   │   └── settings.rs     # 添加阿里云 SMS、腾讯防水墙配置结构
│   └── mod.rs
└── shared/
    └── error.rs            # 可能需要添加新的错误类型

web/
├── service/
│   └── auth.ts             # 改造：添加微信登录、短信登录 API 封装
├── hooks/
│   ├── use-wechat-login.ts  # 新增：微信扫码登录 hook
│   └── use-sms-login.ts    # 新增：短信验证码登录 hook
└── app/
    └── tenant/login/page.tsx  # 改造：增加微信/短信登录入口
    └── auth/wechat/callback/page.tsx  # 可选：独立的微信回调页面（备用）

migrations/
└── (已有) 20260314000001_create_auth_social_tables.sql  # 复用现有表结构
    - social_bindings 表（对应 spec 中的 user_auth_links）
    - sms_codes 表
    - social_configs 表
└── 新增迁移: 添加 users.phone_number 字段
```

---

## 二、后端任务

### Task 0: 数据库迁移

**Files:**
- Create: `api/migrations/20260330000001_add_phone_to_users.sql`

- [ ] **Step 1: 创建迁移文件**

```sql
-- 添加 phone_number 字段到 users 表
ALTER TABLE users ADD COLUMN phone_number VARCHAR(20) UNIQUE;
```

- [ ] **Step 2: 运行迁移**

```bash
cd api
sqlite3 tinyiothub.db < migrations/20260330000001_add_phone_to_users.sql
# 或使用 sqlx migrate run
```

- [ ] **Step 3: Commit**

```bash
git add api/migrations/20260330000001_add_phone_to_users.sql
git commit -m "feat(auth): add phone_number field to users table"
```

---

### Task 1: 配置结构扩展

**Files:**
- Modify: `api/src/infrastructure/config/settings.rs`

- [ ] **Step 1: 添加阿里云 SMS 配置结构**

在 `settings.rs` 中找到 `SmsConfig` 结构，扩展为：

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SmsConfig {
    pub enabled: bool,
    pub rate_limit: Option<SmsRateLimit>,
    // 阿里云 SMS 新增
    pub aliyun: Option<AliyunSmsConfig>,
    // 腾讯防水墙新增
    pub captcha: Option<CaptchaConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AliyunSmsConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub sign_name: String,        // 短信签名
    pub template_code: String,    // 短信模板 code
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CaptchaConfig {
    pub enabled: bool,
    pub app_id: String,           // 腾讯防水墙 AppID
    pub app_secret: String,       // 腾讯防水墙 AppSecret
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SmsRateLimit {
    pub code_expire_secs: Option<u64>,  // 验证码有效期，默认 300
    pub max_per_minute: Option<u64>,    // 每分钟最大发送次数
    pub daily_limit: Option<u64>,       // 每天最大发送次数，默认 5
    pub interval_secs: Option<u64>,     // 发送间隔，默认 90
}
```

- [ ] **Step 2: 添加微信配置结构**

在 `SocialConfig` 附近添加：

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WechatConfig {
    pub enabled: bool,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub redirect_uri: Option<String>,
}
```

- [ ] **Step 3: 更新配置加载逻辑**

在 `load_from_yaml` 或类似函数中添加新配置的解析（参考现有模式）

- [ ] **Step 4: Commit**

```bash
git add api/src/infrastructure/config/settings.rs
git commit -m "feat(auth): add SMS and WeChat config structures"
```

---

### Task 2: Redis 集成（短信频率限制）

**Files:**
- Create: `api/src/infrastructure/redis/mod.rs` (如果不存在)
- Modify: `api/src/api/auth/sms.rs`

- [ ] **Step 1: 检查现有 Redis 使用**

```bash
grep -r "redis" api/src/ --include="*.rs" | head -20
```

- [ ] **Step 2: 如果没有 Redis 模块，创建基础 Redis 客户端封装**

```rust
// api/src/infrastructure/redis/mod.rs
use redis::{Client, AsyncCommands};

pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    pub fn new(url: &str) -> Result<Self, redis::RedisError> {
        Ok(Self { client: Client::open(url)? })
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.get(key).await
    }

    pub async fn set_ex(&self, key: &str, value: &str, secs: u64) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.set_ex(key, value, secs).await?;
        Ok(())
    }

    pub async fn incr(&self, key: &str) -> Result<i64, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.incr(key, 1).await
    }

    pub async fn ttl(&self, key: &str) -> Result<i64, redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.ttl(key).await
    }

    pub async fn del(&self, key: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.del(key).await?;
        Ok(())
    }
}
```

- [ ] **Step 3: 在 AppState 中添加 Redis 客户端引用**

查找 `api/src/api/mod.rs` 中的 `AppState` 定义，添加：

```rust
pub struct AppState {
    // ... existing fields
    pub redis: Option<RedisClient>,
}
```

- [ ] **Step 4: Commit**

```bash
git add api/src/infrastructure/redis/mod.rs api/src/api/mod.rs
git commit -m "feat(auth): add Redis client for rate limiting"
```

---

### Task 3: 短信验证码发送（改造 sms.rs）

**Files:**
- Modify: `api/src/api/auth/sms.rs`

- [ ] **Step 1: 添加频率限制检查函数**

在 `sms.rs` 中添加：

> 注意：`find_or_create_user_by_phone` 已在 sms.rs 中存在（复用），`generate_jwt_token` 也在 `jwt::generate_token` 中存在

```rust
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
    if let Ok(count) = redis.get(&daily_key).await {
        if let Ok(c) = count.unwrap_or_default().parse::<i64>() {
            if c >= daily_limit {
                return Ok(RateLimitResult::DailyLimitExceeded);
            }
        }
    }

    // 检查同 IP 5分钟内发送次数
    if let Some(ip_addr) = ip {
        let ip_key = format!("sms:count:ip:{}", ip_addr);
        if let Ok(count) = redis.get(&ip_key).await {
            if let Ok(c) = count.unwrap_or_default().parse::<i64>() {
                if c >= 3 {
                    return Ok(RateLimitResult::NeedsCaptcha);
                }
            }
        }
    }

    Ok(RateLimitResult::Allowed)
}

enum RateLimitResult {
    Allowed,
    NeedsWait(i64),
    DailyLimitExceeded,
    NeedsCaptcha,
}
```

- [ ] **Step 2: 添加 CAPTCHA 验证函数**

```rust
/// 验证腾讯防水墙票据
async fn verify_captcha(ticket: &str, randstr: &str, ip: &str) -> Result<bool, StatusCode> {
    let config = get_config();
    let captcha_config = match &config.sms.captcha {
        Some(c) if c.enabled => c,
        None => return Ok(true), // 未配置时跳过
    };

    let url = "https://ssl.captcha.qq.com/ticket/verify";
    let params = [
        ("aid", &captcha_config.app_id),
        ("AppSecretKey", &captcha_config.app_secret),
        ("Ticket", ticket),
        ("Randstr", randstr),
        ("UserIP", ip),
    ];

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .query(&params)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    #[derive(Deserialize)]
    struct CaptchaResponse {
        response: i32,
        err_msg: String,
    }

    let result: CaptchaResponse = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok(result.response == 1)
}
```

- [ ] **Step 3: 改造 `send_code` 函数**

修改 `send_code` 函数签名和逻辑：

```rust
async fn send_code(
    State(state): State<AppState>,
    IpAddress(ip): IpAddress,  // 需要引入 axum::extract::IpAddress
    Json(request): Json<SendCodeRequest>,
) -> Json<ApiResponse<SendCodeResponse>> {
    // ... 现有配置检查 ...

    let ip_str = ip.to_string();

    // 频率限制检查
    match check_rate_limit(&state.redis, phone, Some(&ip_str)).await {
        Ok(RateLimitResult::NeedsWait(secs)) => {
            return ApiResponse::error(format!("操作太频繁，请 {} 秒后重试", secs));
        }
        Ok(RateLimitResult::DailyLimitExceeded) => {
            return ApiResponse::error("今日发送次数已用完，请明天再试".to_string());
        }
        Ok(RateLimitResult::NeedsCaptcha) => {
            return ApiResponse::error_with_code(1001, "请先完成验证".to_string());
        }
        Ok(RateLimitResult::Allowed) => {}
        Err(e) => return e,
    }

    // CAPTCHA 验证（如果频率异常）
    if request.captcha_ticket.is_some() {
        let ticket = request.captcha_ticket.as_ref().unwrap();
        let randstr = request.captcha_randstr.as_ref().unwrap_or(&"".to_string());
        if !verify_captcha(ticket, randstr, &ip_str).await? {
            return ApiResponse::error("验证失败，请重试".to_string());
        }
    }

    // ... 生成验证码并存储到 Redis ...
    let code = generate_code();
    let redis = state.redis.as_ref();

    if let Some(r) = redis {
        // 存储验证码到 Redis（5分钟过期）
        let code_key = format!("sms:code:{}", phone);
        r.set_ex(&code_key, &code, 300).await?;

        // 设置发送间隔（90秒）
        let interval_key = format!("sms:interval:{}", phone);
        r.set_ex(&interval_key, "1", 90).await?;

        // 增加当日计数
        let daily_key = format!("sms:count:daily:{}", phone);
        r.incr(&daily_key).await?;
        // 设置每日计数器在次日凌晨过期（简化处理：直接设置 24 小时）
        r.set_ex(&daily_key, "1", 86400).await?;

        // 增加 IP 计数
        let ip_key = format!("sms:count:ip:{}", ip_str);
        r.incr(&ip_key).await?;
        r.set_ex(&ip_key, "1", 300).await?;
    }

    // 调用阿里云 SMS（或者在测试模式下直接返回）
    #[cfg(debug_assertions)]
    {
        tracing::info!("[TEST] SMS code for {}: {}", phone, code);
        ApiResponse::success(SendCodeResponse {
            expires_in: CODE_EXPIRE_SECONDS,
            message: format!("验证码已发送（测试模式: {}）", code),
        })
    }

    #[cfg(not(debug_assertions))]
    {
        // 调用阿里云 SMS API
        match send_aliyun_sms(phone, &code, &config.sms.aliyun.unwrap()).await {
            Ok(_) => ApiResponse::success(SendCodeResponse {
                expires_in: CODE_EXPIRE_SECONDS,
                message: "验证码已发送".to_string(),
            }),
            Err(e) => {
                tracing::error!("Failed to send SMS: {}", e);
                ApiResponse::error("发送失败，请稍后重试".to_string())
            }
        }
    }
}
```

- [ ] **Step 4: 更新请求结构添加 CAPTCHA 字段**

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendCodeRequest {
    pub phone: String,
    pub purpose: Option<String>,
    pub captcha_ticket: Option<String>,   // 腾讯防水墙票据
    pub captcha_randstr: Option<String>,  // 腾讯防水墙随机串
}
```

- [ ] **Step 5: 添加阿里云 SMS 发送函数**

```rust
/// 调用阿里云 SMS API 发送短信
async fn send_aliyun_sms(
    phone: &str,
    code: &str,
    config: &AliyunSmsConfig,
) -> Result<(), String> {
    use dysmsapi20170525::Client;
    use std::sync::Arc;

    let client = Client::new(
        Arc::new(dysmsapi20170525::Config::new()
            .with_access_key_id(&config.access_key_id)
            .with_access_key_secret(&config.access_key_secret)),
    );

    client.send_sms_request()
        .with_phone_numbers(phone)
        .with_sign_name(&config.sign_name)
        .with_template_code(&config.template_code)
        .with_template_param_json(format!(r#"{{"code":"{}"}}"#, code))
        .send()
        .await
        .map_err(|e| format!("SMS API error: {}", e))?;

    Ok(())
}
```

- [ ] **Step 6: Commit**

```bash
git add api/src/api/auth/sms.rs
git commit -m "feat(auth): add Redis rate limiting and CAPTCHA verification to SMS"
```

---

### Task 4: 短信登录验证（改造 sms.rs）

**Files:**
- Modify: `api/src/api/auth/sms.rs`

- [ ] **Step 1: 改造 `login_with_code` 函数使用 Redis**

```rust
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
        None => return ApiResponse::error("验证码已过期，请重新获取".to_string()),
    };

    // 验证码比较
    use subtle::ConstantTimeEq;
    if !stored_code.as_bytes().ct_eq(code.as_bytes()).into() {
        // 增加错误计数
        if let Some(r) = redis {
            let fail_key = format!("sms:verify:fail:{}", phone);
            let fail_count: i64 = r.get(&fail_key).await.ok().flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0) + 1;
            r.set_ex(&fail_key, fail_count.to_string(), 300).await?;

            if fail_count >= 3 {
                // 错误次数过多，删除验证码
                let code_key = format!("sms:code:{}", phone);
                r.del(&code_key).await?;
                return ApiResponse::error("验证码错误次数过多，请重新获取".to_string());
            }
        }
        return ApiResponse::error("验证码错误".to_string());
    }

    // 验证成功，删除验证码
    if let Some(r) = redis {
        let code_key = format!("sms:code:{}", phone);
        r.del(&code_key).await?;
    }

    // 查找或创建用户（复用现有逻辑）
    let db = state.database();
    let user = find_or_create_user_by_phone(db, phone).await?;

    // 生成 JWT token（复用 jwt::generate_token）
    let token = jwt::generate_token(&user.id, user.get_display_name())?;

    ApiResponse::success(LoginWithCodeResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: 7200,
        user_info: UserInfo {
            id: user.id,
            phone: user.phone.unwrap_or_default(),
            username: Some(user.username),
            display_name: user.display_name,
        },
    })
}
```

- [ ] **Step 2: Commit**

```bash
git add api/src/api/auth/sms.rs
git commit -m "feat(auth): integrate Redis for SMS code verification"
```

---

### Task 5: 微信 OAuth 集成（改造 social.rs）

**Files:**
- Modify: `api/src/api/auth/social.rs`

- [ ] **Step 1: 添加 state CSRF 保护函数**

```rust
/// 生成并存储 OAuth state 参数到 Redis
async fn generate_oauth_state(
    redis: &Option<RedisClient>,
    state: &str,
) -> Result<(), StatusCode> {
    let redis = redis.as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let key = format!("wechat:state:{}", state);
    redis.set_ex(&key, "1", 300).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

/// 验证并删除 OAuth state 参数
async fn verify_oauth_state(
    redis: &Option<RedisClient>,
    state: &str,
) -> Result<bool, StatusCode> {
    let redis = redis.as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let key = format!("wechat:state:{}", state);
    let exists: Option<String> = redis.get(&key).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if exists.is_some() {
        // 删除 state（一次性使用）
        redis.del(&key).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
```

- [ ] **Step 2: 改造 `get_wechat_qrcode` 函数，添加 state 存储到 Redis**

> 现有 `get_wechat_qrcode` 函数（lines 128-184）需要改造，在返回前将 state 存储到 Redis

```rust
// 在现有 get_wechat_qrcode 函数末尾，生成 state 后添加：

// 将 state 存储到 Redis（5分钟有效期）
if let Some(redis) = &state.redis {
    let state_key = format!("wechat:state:{}", state);
    if let Err(e) = redis.set_ex(&state_key, "1", 300).await {
        tracing::warn!("Failed to store WeChat state in Redis: {}", e);
        // 不阻止流程，仅记录警告
    }
}
```

- [ ] **Step 3: 添加微信 API 调用函数**

```rust
/// 调用微信 API 换取 access_token 和 openid
async fn exchange_wechat_code(
    code: &str,
    config: &WechatOAuthConfig,
) -> Result<WechatTokenResponse, String> {
    let url = format!(
        "https://api.weixin.qq.com/sns/oauth2/access_token?appid={}&secret={}&code={}&grant_type=authorization_code",
        config.app_id, config.app_secret, code
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    #[derive(Deserialize)]
    struct WechatErrorResponse {
        errcode: i32,
        errmsg: String,
    }

    // 检查微信返回错误
    if let Ok(err_resp) = resp.json::<WechatErrorResponse>().await {
        if err_resp.errcode != 0 {
            return Err(format!("WeChat API error: {} - {}", err_resp.errcode, err_resp.errmsg));
        }
    }

    resp.json::<WechatTokenResponse>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct WechatTokenResponse {
    access_token: String,
    expires_in: i32,
    refresh_token: String,
    openid: String,
    scope: String,
}
```

- [ ] **Step 4: 改造 `wechat_callback` 函数**

> 注意：此函数需要改为返回 HTML 页面（内嵌 JavaScript），由该页面通过 postMessage 将 token 发送给 opener

```rust
use axum::{
    extract::Query,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router, State,
};

async fn wechat_callback(
    State(state): State<AppState>,
    Query(params): Query<WeChatCallbackQuery>,
) -> Response {
    if let Some(error) = params.error_description {
        // 授权失败，返回错误页面
        let html = format!(r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({{type:'wechat_callback',error:'{}'}},window.location.origin);window.close();</script></body></html>"#, error);
        return Html(html).into_response();
    }

    let code = match params.code {
        Some(c) => c,
        None => {
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'授权码不存在'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    let state = match params.state {
        Some(s) => s,
        None => {
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'state参数缺失'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    // 验证 state CSRF 保护
    match verify_oauth_state(&state.redis, &state).await {
        Ok(true) => {}
        Ok(false) => {
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'授权已过期，请重新扫码'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
        Err(e) => {
            tracing::error!("Redis error verifying state: {:?}", e);
        }
    }

    // 换取 access_token 和 openid
    let config = match get_wechat_config(state.database()).await {
        Some(c) => c,
        None => {
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'微信配置错误'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    let wechat_config = WechatOAuthConfig {
        app_id: config.app_id.unwrap_or_default(),
        app_secret: config.app_secret.unwrap_or_default(),
    };

    let token_resp = match exchange_wechat_code(&code, &wechat_config).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to exchange WeChat code: {}", e);
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'获取授权信息失败'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    // 查找或创建用户
    let db = state.database();
    let user = match find_or_create_user_by_wechat(db, &token_resp.openid).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to find/create user: {:?}", e);
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'用户处理失败'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    // 生成 JWT
    let jwt_token = match jwt::generate_token(&user.id, user.get_display_name()) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to generate JWT: {}", e);
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'生成令牌失败'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    // 存储社交绑定（如果不存在）
    if let Err(e) = save_social_binding(state.database(), &user.id, "wechat", &token_resp.openid).await {
        tracing::warn!("Failed to save social binding: {:?}", e);
    }

    // 返回成功页面，通过 postMessage 发送 token
    let html = format!(r#"<!DOCTYPE html><html><body><script>
        window.opener.postMessage({{type:'wechat_callback',code:'{}',access_token:'{}'}},window.location.origin);
        window.close();
    </script></body></html>"#, code, jwt_token);

    Html(html).into_response()
}
```

> 注意：`get_wechat_config` 已存在于 social.rs（复用），`jwt::generate_token` 已在 `crate::shared::security::jwt` 中存在

struct WechatOAuthConfig {
    app_id: String,
    app_secret: String,
}
```

- [ ] **Step 5: 添加微信用户查找/创建函数**

```rust
/// 根据微信 openid 查找或创建用户
async fn find_or_create_user_by_wechat(
    db: &Database,
    openid: &str,
) -> Result<crate::dto::entity::user::User, StatusCode> {
    // 查找 social_bindings
    let rows = sqlx::query(
        "SELECT user_id FROM social_bindings WHERE provider = 'wechat' AND provider_user_id = ? LIMIT 1"
    )
    .bind(openid)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(row) = rows.into_iter().next() {
        let user_id: String = row.try_get("user_id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // 获取用户信息
        let user_rows = sqlx::query("SELECT * FROM users WHERE id = ? LIMIT 1")
            .bind(&user_id)
            .fetch_all(db.pool())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if let Some(user_row) = user_rows.into_iter().next() {
            return Ok(user_from_row(user_row));
        }
    }

    // 创建新用户（仅用于首次微信登录）
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO users (id, username, is_enabled, created_at, updated_at)
           VALUES (?, ?, 1, ?, ?)"#,
    )
    .bind(&user_id)
    .bind(format!("wechat_{}", &openid[..8]))  // 临时用户名
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user_rows = sqlx::query("SELECT * FROM users WHERE id = ? LIMIT 1")
        .bind(&user_id)
        .fetch_all(db.pool())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    user_rows.into_iter().next()
        .map(user_from_row)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn user_from_row(row: sqlx::Row) -> crate::dto::entity::user::User {
    crate::dto::entity::user::User {
        id: row.try_get("id").unwrap_or_default(),
        username: row.try_get("username").unwrap_or_default(),
        password_hash: row.try_get("password_hash").unwrap_or_default(),
        email: row.try_get("email").ok(),
        phone: row.try_get("phone").ok(),
        display_name: row.try_get("display_name").ok(),
        is_enabled: row.try_get::<i32, _>("is_enabled").unwrap_or(1) == 1,
        parent_id: row.try_get("parent_id").ok(),
        created_at: row.try_get("created_at").unwrap_or_default(),
        updated_at: row.try_get("updated_at").unwrap_or_default(),
        last_login_at: row.try_get("last_login_at").ok(),
    }
}
```

- [ ] **Step 6: 创建微信回调前端页面**

> 微信授权成功后会回调到 `/auth/wechat/callback?code=xxx&state=xxx`，此页面需要：
> 1. 接收 code 和 state 参数
> 2. 调用后端 API 完成登录
> 3. 通过 postMessage 将结果发送给 opener 窗口

**Files:**
- Create: `web/app/auth/wechat/callback/page.tsx`

```tsx
'use client'

import { useEffect, useState } from 'react'
import { useSearchParams } from 'next/navigation'
import { apiPost } from '@/lib/api-client'
import { saveTenantToken } from '@/service/tenant'

export default function WechatCallbackPage() {
  const searchParams = useSearchParams()
  const [error, setError] = useState('')

  useEffect(() => {
    const code = searchParams.get('code')
    const state = searchParams.get('state')
    const errorDesc = searchParams.get('error_description')

    if (errorDesc) {
      window.opener?.postMessage({ type: 'wechat_callback', error: errorDesc }, window.location.origin)
      window.close()
      return
    }

    if (!code || !state) {
      setError('授权参数不完整')
      window.opener?.postMessage({ type: 'wechat_callback', error: '授权参数不完整' }, window.location.origin)
      window.close()
      return
    }

    // 调用后端回调接口
    apiPost('/auth/social/wechat/callback', { code, state })
      .then((resp) => {
        if (resp.code === 0 && resp.result?.access_token) {
          saveTenantToken(resp.result.access_token)
          window.opener?.postMessage({
            type: 'wechat_callback',
            code,
            access_token: resp.result.access_token,
          }, window.location.origin)
        } else {
          window.opener?.postMessage({ type: 'wechat_callback', error: resp.msg }, window.location.origin)
        }
      })
      .catch((err) => {
        window.opener?.postMessage({ type: 'wechat_callback', error: err.message }, window.location.origin)
      })
      .finally(() => {
        window.close()
      })
  }, [searchParams])

  return (
    <div className="flex items-center justify-center h-screen">
      {error ? (
        <p className="text-red-600">{error}</p>
      ) : (
        <p>正在处理登录...</p>
      )}
    </div>
  )
}
```

- [ ] **Step 6: Commit**

```bash
git add api/src/api/auth/social.rs
git commit -m "feat(auth): implement WeChat OAuth callback with HTML postMessage"
```

---

### Task 6: 前端 - 短信登录 Hook

**Files:**
- Create: `web/hooks/use-sms-login.ts`
- Modify: `web/service/auth.ts`

- [ ] **Step 1: 创建 `use-sms-login.ts`**

```typescript
// web/hooks/use-sms-login.ts
'use client'

import { useMutation } from '@tanstack/react-query'
import { apiPost } from '@/lib/api-client'
import { useAuthStore } from '@/store/provider'

interface SendSmsCodeRequest {
  phone: string
  captcha_ticket?: string
  captcha_randstr?: string
}

interface SmsLoginRequest {
  phone: string
  code: string
}

interface SmsCodeResponse {
  expires_in: number
  message: string
}

interface LoginResponse {
  access_token: string
  token_type: string
  expires_in: number
  user_info: {
    id: string
    phone: string
    username?: string
    display_name?: string
  }
}

export const useSmsLogin = () => {
  const { login: setAuthState } = useAuthStore()

  const sendCode = useMutation({
    mutationFn: (data: SendSmsCodeRequest) =>
      apiPost<SmsCodeResponse>('auth/sms/send', data),
  })

  const loginWithCode = useMutation({
    mutationFn: (data: SmsLoginRequest) =>
      apiPost<LoginResponse>('auth/sms/login', data),
    onSuccess: async (response) => {
      if (response.code === 0 && response.result) {
        const { access_token, user_info } = response.result
        await setAuthState(user_info.username || user_info.phone, '')
        return response.result
      }
      throw new Error(response.msg)
    },
  })

  return {
    sendCode,
    loginWithCode,
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add web/hooks/use-sms-login.ts
git commit -m "feat(auth): add SMS login hook"
```

---

### Task 7: 前端 - 微信登录 Hook

**Files:**
- Create: `web/hooks/use-wechat-login.ts`

- [ ] **Step 1: 创建 `use-wechat-login.ts`**

```typescript
// web/hooks/use-wechat-login.ts
'use client'

import { useQuery } from '@tanstack/react-query'
import { apiGet } from '@/lib/api-client'
import { useAuthStore } from '@/store/provider'

interface WechatQrcodeResponse {
  qrcode_url: string
  authorize_url: string
  state: string
}

export const useWechatLogin = () => {
  const { login: setAuthState } = useAuthStore()

  // 获取微信二维码
  const getQrcode = useQuery({
    queryKey: ['wechat', 'qrcode'],
    queryFn: () => apiGet<WechatQrcodeResponse>('auth/social/wechat/qrcode'),
    enabled: false,  // 手动触发
  })

  // 完成登录（供 login page 调用）
  const completeLogin = async (accessToken: string) => {
    await setAuthState('wechat_user', '')
    return { access_token: accessToken }
  }

  return {
    getQrcode,
    completeLogin,
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add web/hooks/use-wechat-login.ts
git commit -m "feat(auth): add WeChat login hook"
```

---

### Task 8: 前端 - 登录页面改造

**Files:**
- Modify: `web/app/tenant/login/page.tsx`

- [ ] **Step 1: 改造登录页面，添加微信和短信登录入口**

```tsx
// web/app/tenant/login/page.tsx (仅展示新增部分)

'use client'

import { useState } from 'react'
import { useRouter } from 'next/navigation'
import Link from 'next/link'
import { tenantApi, saveTenantToken, saveTenantData } from '@/service/tenant'
import { useSmsLogin } from '@/hooks/use-sms-login'
import { useWechatLogin } from '@/hooks/use-wechat-login'

type LoginMode = 'password' | 'sms' | 'wechat'

export default function LoginPage() {
  const router = useRouter()
  const [loginMode, setLoginMode] = useState<LoginMode>('password')
  const [formData, setFormData] = useState({ ... })
  const [smsPhone, setSmsPhone] = useState('')
  const [smsCode, setSmsCode] = useState('')
  const [countdown, setCountdown] = useState(0)
  const [error, setError] = useState('')
  const [isLoading, setIsLoading] = useState(false)

  const { sendCode, loginWithCode } = useSmsLogin()
  const { getQrcode } = useWechatLogin()

  // 发送验证码
  const handleSendCode = async () => {
    if (!smsPhone || smsPhone.length !== 11) {
      setError('请输入正确的手机号')
      return
    }
    setError('')
    const result = await sendCode.mutateAsync({ phone: smsPhone })
    if (result.code === 0) {
      setCountdown(90)
      const timer = setInterval(() => {
        setCountdown(c => {
          if (c <= 1) clearInterval(timer)
          return c - 1
        })
      }, 1000)
    } else {
      setError(result.msg)
    }
  }

  // 短信登录
  const handleSmsLogin = async () => {
    if (!smsCode) {
      setError('请输入验证码')
      return
    }
    setError('')
    setIsLoading(true)
    try {
      const result = await loginWithCode.mutateAsync({
        phone: smsPhone,
        code: smsCode,
      })
      if (result.code === 0 && result.result) {
        const { token } = result.result
        // 注意：这里需要适配 tenant API 的响应格式
        saveTenantToken(token)
        router.push('/tenant/dashboard')
      } else {
        throw new Error(result.msg)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '登录失败')
    } finally {
      setIsLoading(false)
    }
  }

  // 微信登录
  const handleWechatLogin = async () => {
    const result = await getQrcode.refetch()
    if (result.data?.result) {
      const { authorize_url, state } = result.data.result

      // 打开微信授权页面
      const popup = window.open(authorize_url, 'wechat_login', 'width=600,height=700')

      // 监听 postMessage 回调
      const handleMessage = async (event: MessageEvent) => {
        // 验证消息来源
        if (event.origin !== window.location.origin) return

        const { type, code, error } = event.data || {}
        if (type !== 'wechat_callback') return

        window.removeEventListener('message', handleMessage)
        popup?.close()

        if (error) {
          setError(error)
          return
        }

        // 调用后端回调接口获取 token
        try {
          const resp = await wechatCallback.mutateAsync(code)
          if (resp.code === 0 && resp.result?.access_token) {
            // 保存 token 并跳转
            saveTenantToken(resp.result.access_token)
            router.push('/tenant/dashboard')
          } else {
            setError(resp.msg || '登录失败')
          }
        } catch (err) {
          setError(err instanceof Error ? err.message : '登录失败')
        }
      }

      window.addEventListener('message', handleMessage)
    }
  }

  return (
    <div className="w-full max-w-md">
      {/* 登录方式切换 */}
      <div className="flex mb-6 border-b border-gray-200">
        <button
          onClick={() => setLoginMode('password')}
          className={`flex-1 pb-2 text-center ${
            loginMode === 'password'
              ? 'border-b-2 border-primary-600 text-primary-600'
              : 'text-gray-500'
          }`}
        >
          密码登录
        </button>
        <button
          onClick={() => setLoginMode('sms')}
          className={`flex-1 pb-2 text-center ${
            loginMode === 'sms'
              ? 'border-b-2 border-primary-600 text-primary-600'
              : 'text-gray-500'
          }`}
        >
          短信登录
        </button>
        <button
          onClick={() => setLoginMode('wechat')}
          className={`flex-1 pb-2 text-center ${
            loginMode === 'wechat'
              ? 'border-b-2 border-primary-600 text-primary-600'
              : 'text-gray-500'
          }`}
        >
          微信登录
        </button>
      </div>

      {/* 密码登录表单 */}
      {loginMode === 'password' && (
        <form onSubmit={handleSubmit} className="space-y-4">
          {/* ... 现有表单内容 ... */}
        </form>
      )}

      {/* 短信登录表单 */}
      {loginMode === 'sms' && (
        <div className="space-y-4">
          <div>
            <input
              type="tel"
              value={smsPhone}
              onChange={e => setSmsPhone(e.target.value)}
              placeholder="请输入手机号"
              className="w-full px-3 py-2 border border-gray-300 rounded-lg"
              maxLength={11}
            />
          </div>
          <div className="flex space-x-2">
            <input
              type="text"
              value={smsCode}
              onChange={e => setSmsCode(e.target.value)}
              placeholder="验证码"
              className="flex-1 px-3 py-2 border border-gray-300 rounded-lg"
              maxLength={6}
            />
            <button
              type="button"
              onClick={handleSendCode}
              disabled={countdown > 0}
              className="px-4 py-2 bg-gray-100 text-gray-700 rounded-lg disabled:opacity-50"
            >
              {countdown > 0 ? `${countdown}s` : '获取验证码'}
            </button>
          </div>
          <button
            onClick={handleSmsLogin}
            disabled={isLoading}
            className="w-full py-2.5 bg-primary-600 text-white rounded-lg"
          >
            {isLoading ? '登录中...' : '登录'}
          </button>
        </div>
      )}

      {/* 微信登录 */}
      {loginMode === 'wechat' && (
        <div className="text-center py-8">
          <button
            onClick={handleWechatLogin}
            className="w-full py-3 bg-green-600 text-white rounded-lg flex items-center justify-center space-x-2"
          >
            <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
              {/* 微信图标 */}
            </svg>
            <span>微信扫码登录</span>
          </button>
          <p className="text-sm text-gray-500 mt-4">
            使用微信扫描上方二维码进行登录
          </p>
        </div>
      )}

      {/* 错误提示 */}
      {error && (
        <div className="mt-4 p-3 bg-red-50 border border-red-200 text-red-600 rounded-lg text-sm">
          {error}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web/app/tenant/login/page.tsx
git commit -m "feat(auth): add SMS and WeChat login to tenant login page"
```

---

### Task 9: 环境配置示例

**Files:**
- Create: `.env.example` 更新（如果需要）

- [ ] **Step 1: 在 `.env.example` 中添加新配置项**

```bash
# 阿里云 SMS
SMS_ALIYUN_ACCESS_KEY_ID=your_access_key_id
SMS_ALIYUN_ACCESS_KEY_SECRET=your_access_key_secret
SMS_ALIYUN_SIGN_NAME=您的签名
SMS_ALIYUN_TEMPLATE_CODE=SMS_xxxxx

# 腾讯防水墙
SMS_CAPTCHA_APP_ID=your_app_id
SMS_CAPTCHA_APP_SECRET=your_app_secret

# 微信开放平台
WECHAT_APP_ID=your_app_id
WECHAT_APP_SECRET=your_app_secret
WECHAT_REDIRECT_URI=https://yourdomain.com/auth/wechat/callback
```

- [ ] **Step 2: Commit**

```bash
git add .env.example
git commit -m "docs: add SMS and WeChat login environment variables"
```

---

## 三、测试验证

### 后端测试

- [ ] 发送短信验证码：验证 Redis 频率限制生效
- [ ] 验证码登录：验证错误计数和过期处理
- [ ] 微信回调：验证 state CSRF 保护和 code 换取 openid
- [ ] 错误流程：测试各种边界情况

### 前端测试

- [ ] 短信登录完整流程
- [ ] 微信登录二维码显示和扫码授权
- [ ] 登录方式切换
- [ ] 错误提示显示

---

## 四、待集成（需申请）

以下配置需在申请完成后填写：
- [ ] 阿里云 SMS：AccessKey ID/Secret、签名、模板 Code
- [ ] 腾讯防水墙：AppID/AppSecret
- [ ] 微信开放平台：AppID/AppSecret、授权回调域


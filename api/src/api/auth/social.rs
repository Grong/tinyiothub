// 第三方登录模块
// 支持微信扫码登录

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    api::AppState,
    dto::{entity::user::User, response::ApiResponse},
    infrastructure::{config::get as get_config, redis::RedisClient},
    shared::security::jwt,
};

pub fn create_router() -> Router<AppState> {
    Router::new()
        // 微信扫码登录
        .route("/wechat/qrcode", get(get_wechat_qrcode))
        .route("/wechat/callback", get(wechat_callback))
        .route("/wechat/login", post(wechat_login))
        .route("/wechat/miniprogram/login", post(wechat_miniprogram_login))
        // 绑定和解绑
        .route("/bind", post(bind_social_account))
        .route("/unbind", post(unbind_social_account))
        // 登录配置
        .route("/config", get(get_social_config))
        .route("/config", post(update_social_config))
}

// ============== 请求/响应结构 ==============

/// 微信扫码请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatQRCodeRequest {
    pub redirect_uri: Option<String>,
    pub state: Option<String>,
}

/// 微信扫码响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatQRCodeResponse {
    pub qrcode_url: String,    // 二维码页面URL
    pub authorize_url: String, // 授权URL
    pub state: String,         // 状态参数
}

/// 微信回调请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error_description: Option<String>,
}

/// 微信登录请求（小程序）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatMiniProgramLoginRequest {
    pub code: String, // 小程序 code
    pub encrypted_data: Option<String>,
    pub iv: Option<String>,
}

/// 微信登录响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_info: SocialUserInfo,
    pub is_new_user: bool,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SocialUserInfo {
    pub id: String,
    pub provider: String,
    pub provider_user_id: String,
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
    pub phone: Option<String>,
}

/// 绑定请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BindSocialRequest {
    pub provider: String,
    pub code: String,
}

/// 解绑请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UnbindSocialRequest {
    pub provider: String,
}

/// 社交登录配置
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SocialConfig {
    pub provider: String,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub is_enabled: bool,
}

/// 更新配置请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateSocialConfigRequest {
    pub provider: String,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub is_enabled: Option<bool>,
}

// ============== 路由处理函数 ==============

/// 获取微信扫码二维码
async fn get_wechat_qrcode(
    State(state): State<AppState>,
    Query(params): Query<WeChatQRCodeRequest>,
) -> Json<ApiResponse<WeChatQRCodeResponse>> {
    // 从配置获取微信设置
    let config = get_config();

    let wechat_config = match &config.social.wechat {
        Some(c) => c,
        None => {
            return ApiResponse::error("微信登录未配置".to_string());
        }
    };

    // 检查是否启用
    if !wechat_config.enabled {
        return ApiResponse::error("微信登录未启用".to_string());
    }

    let app_id = match &wechat_config.app_id {
        Some(id) => id.clone(),
        None => {
            return ApiResponse::error("微信 AppID 未配置".to_string());
        }
    };

    let app_secret = match &wechat_config.app_secret {
        Some(secret) => secret.clone(),
        None => {
            return ApiResponse::error("微信 AppSecret 未配置".to_string());
        }
    };

    let redirect_uri = params
        .redirect_uri
        .clone()
        .or_else(|| wechat_config.redirect_uri.clone())
        .unwrap_or_default();

    // 生成 state
    let oauth_state = params.state.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // 授权回调地址
    let encoded_redirect = urlencoding::encode(&redirect_uri);

    // 微信授权 URL
    let authorize_url = format!(
        "https://open.weixin.qq.com/connect/qrconnect?appid={}&redirect_uri={}&response_type=code&scope=snsapi_login&state={}#wechat_redirect",
        app_id, encoded_redirect, oauth_state
    );

    // 二维码页面 URL
    let qrcode_url = format!("https://login.weixin.qq.com/l/{}", oauth_state);

    // 将 state 存储到 Redis（5分钟有效期）
    if let Some(redis) = &state.redis {
        let state_key = format!("wechat:state:{}", oauth_state);
        if let Err(e) = redis.set_ex(&state_key, "1", 300).await {
            tracing::warn!("Failed to store WeChat state in Redis: {}", e);
            // 不阻止流程，仅记录警告
        }
    }

    ApiResponse::success(WeChatQRCodeResponse { qrcode_url, authorize_url, state: oauth_state })
}

/// 微信回调处理
async fn wechat_callback(
    State(state): State<AppState>,
    Query(params): Query<WeChatCallbackQuery>,
) -> Response {
    if let Some(error) = params.error_description {
        let html = format!(
            r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({{type:'wechat_callback',error:'{}'}},window.location.origin);window.close();</script></body></html>"#,
            error
        );
        return Html(html).into_response();
    }

    let code = match params.code {
        Some(c) => c,
        None => {
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'授权码不存在'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    let state_param = match params.state {
        Some(s) => s,
        None => {
            let html = r#"<!DOCTYPE html><html><body><script>window.opener.postMessage({type:'wechat_callback',error:'state参数缺失'},window.location.origin);window.close();</script></body></html>"#.to_string();
            return Html(html).into_response();
        }
    };

    // 验证 state CSRF 保护
    match verify_oauth_state(&state.redis, &state_param).await {
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
    let tenant_id: String = sqlx::query_scalar(
        "SELECT tenant_id FROM tenant_users WHERE user_id = ? LIMIT 1"
    )
    .bind(&user.id)
    .fetch_optional(db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "default".to_string());

    let jwt_token = match jwt::generate_token(&user.id, &user.username, &tenant_id) {
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

    // 查找该租户的第一个 workspace 作为默认 workspace
    let workspace_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE tenant_id = ? LIMIT 1"
    )
    .bind(&tenant_id)
    .fetch_optional(db.pool())
    .await
    .unwrap_or(None);

    // 返回成功页面，通过 postMessage 发送 token
    let html = format!(
        r#"<!DOCTYPE html><html><body><script>
        window.opener.postMessage({{type:'wechat_callback',code:'{}',access_token:'{}',workspace_id:'{}'}},window.location.origin);
        window.close();
    </script></body></html>"#,
        code, jwt_token, workspace_id.unwrap_or_default()
    );

    Html(html).into_response()
}

/// 微信登录（使用授权码）
async fn wechat_login(
    State(state): State<AppState>,
    Json(request): Json<WeChatLoginCodeRequest>,
) -> Json<ApiResponse<WeChatLoginResponse>> {
    let code = request.code.trim();

    if code.is_empty() {
        return ApiResponse::error("授权码不能为空".to_string());
    }

    let db = state.database();

    // 获取微信配置
    let config = match get_wechat_config(db).await {
        Some(c) => c,
        None => {
            return ApiResponse::error("微信登录未配置".to_string());
        }
    };

    let (app_id, app_secret) = match (config.app_id, config.app_secret) {
        (Some(id), Some(secret)) => (id, secret),
        _ => {
            return ApiResponse::error("微信配置不完整".to_string());
        }
    };

    // TODO: 调用微信 API 换取 access_token
    // let token_url = format!(
    //     "https://api.weixin.qq.com/sns/oauth2/access_token?appid={}&secret={}&code={}&grant_type=authorization_code",
    //     app_id, app_secret, code
    // );

    // 这里返回模拟响应（开发阶段）
    // 查找默认 workspace
    let workspace_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces LIMIT 1"
    )
    .fetch_optional(db.pool())
    .await
    .unwrap_or(None);

    ApiResponse::success(WeChatLoginResponse {
        access_token: "mock_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: 7200,
        user_info: SocialUserInfo {
            id: uuid::Uuid::new_v4().to_string(),
            provider: "wechat".to_string(),
            provider_user_id: "mock_openid".to_string(),
            nickname: Some("微信用户".to_string()),
            avatar_url: None,
            phone: None,
        },
        is_new_user: true,
        workspace_id,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatLoginCodeRequest {
    pub code: String,
}

/// 微信小程序登录
async fn wechat_miniprogram_login(
    State(state): State<AppState>,
    Json(request): Json<WeChatMiniProgramLoginRequest>,
) -> Json<ApiResponse<WeChatLoginResponse>> {
    let code = request.code.trim();

    if code.is_empty() {
        return ApiResponse::error("code 不能为空".to_string());
    }

    let db = state.database();

    // 获取微信配置
    let config = match get_wechat_config(db).await {
        Some(c) => c,
        None => {
            return ApiResponse::error("微信登录未配置".to_string());
        }
    };

    // TODO: 调用微信小程序 API 换取 session_key 和 openid
    // let api_url = "https://api.weixin.qq.com/sns/jscode2session";

    // 查找默认 workspace
    let workspace_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces LIMIT 1"
    )
    .fetch_optional(db.pool())
    .await
    .unwrap_or(None);

    ApiResponse::success(WeChatLoginResponse {
        access_token: "mock_mp_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: 7200,
        user_info: SocialUserInfo {
            id: uuid::Uuid::new_v4().to_string(),
            provider: "wechat_miniprogram".to_string(),
            provider_user_id: "mock_openid".to_string(),
            nickname: None,
            avatar_url: None,
            phone: None,
        },
        is_new_user: true,
        workspace_id,
    })
}

/// 绑定社交账号
async fn bind_social_account(
    State(state): State<AppState>,
    Json(request): Json<BindSocialRequest>,
) -> Json<ApiResponse<String>> {
    // TODO: 实现绑定逻辑

    ApiResponse::success("绑定成功".to_string())
}

/// 解绑社交账号
async fn unbind_social_account(
    State(state): State<AppState>,
    Json(request): Json<UnbindSocialRequest>,
) -> Json<ApiResponse<String>> {
    // TODO: 实现解绑逻辑

    ApiResponse::success("解绑成功".to_string())
}

/// 获取社交登录配置
async fn get_social_config(State(state): State<AppState>) -> Json<ApiResponse<Vec<SocialConfig>>> {
    let db = state.database();

    let sql = "SELECT provider, app_id, app_secret, redirect_uri, is_enabled FROM social_configs";

    let rows = match db
        .query(sql, |row| {
            Ok(SocialConfig {
                provider: row.try_get("provider")?,
                app_id: row.try_get("app_id")?,
                app_secret: row.try_get("app_secret")?,
                redirect_uri: row.try_get("redirect_uri")?,
                is_enabled: row.try_get::<i32, _>("is_enabled")? == 1,
            })
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to get social config: {}", e);
            return ApiResponse::error("获取配置失败".to_string());
        }
    };

    let configs: Vec<SocialConfig> = rows.into_iter().map(|r| r).collect();
    ApiResponse::success(configs)
}

/// 更新社交登录配置
async fn update_social_config(
    State(state): State<AppState>,
    Json(request): Json<UpdateSocialConfigRequest>,
) -> Json<ApiResponse<String>> {
    let db = state.database();

    let result = sqlx::query(
        r#"UPDATE social_configs
            SET app_id = ?, app_secret = ?, redirect_uri = ?, is_enabled = ?, updated_at = CURRENT_TIMESTAMP
            WHERE provider = ?"#,
    )
    .bind(request.app_id.unwrap_or_default())
    .bind(request.app_secret.unwrap_or_default())
    .bind(request.redirect_uri.unwrap_or_default())
    .bind(request.is_enabled.unwrap_or(false) as i32)
    .bind(&request.provider)
    .execute(db.pool())
    .await;

    if let Err(e) = result {
        tracing::error!("Failed to update social config: {}", e);
        return ApiResponse::error("更新配置失败".to_string());
    }

    ApiResponse::success("配置已更新".to_string())
}

// ============== 辅助函数 ==============

async fn get_wechat_config(
    db: &crate::infrastructure::persistence::database::Database,
) -> Option<SocialConfig> {
    let sql = "SELECT * FROM social_configs WHERE provider = 'wechat' LIMIT 1";

    let rows = db
        .query(sql, |row| {
            Ok(SocialConfig {
                provider: row.try_get("provider").unwrap_or_default(),
                app_id: row.try_get("app_id").ok(),
                app_secret: row.try_get("app_secret").ok(),
                redirect_uri: row.try_get("redirect_uri").ok(),
                is_enabled: row.try_get::<i32, _>("is_enabled").unwrap_or(0) == 1,
            })
        })
        .await
        .ok()?;

    rows.into_iter().next()
}

// ============== 微信 OAuth CSRF 保护 ==============

/// 微信 OAuth 配置
struct WechatOAuthConfig {
    app_id: String,
    app_secret: String,
}

/// 生成并存储 OAuth state 参数到 Redis
async fn generate_oauth_state(redis: &Option<RedisClient>, state: &str) -> Result<(), StatusCode> {
    let redis = redis.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let key = format!("wechat:state:{}", state);
    redis
        .set_ex(&key, "1", 300)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

/// 验证并删除 OAuth state 参数
async fn verify_oauth_state(
    redis: &Option<RedisClient>,
    state: &str,
) -> Result<bool, StatusCode> {
    let redis = redis.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let key = format!("wechat:state:{}", state);
    let exists: Option<String> = redis.get(&key).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if exists.is_some() {
        // 删除 state（一次性使用）
        redis.del(&key).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

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
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("Read body error: {}", e))?;

    // 尝试解析错误响应
    if let Ok(err_resp) = serde_json::from_slice::<WechatErrorResponse>(&body) {
        if err_resp.errcode != 0 {
            return Err(format!(
                "WeChat API error: {} - {}",
                err_resp.errcode, err_resp.errmsg
            ));
        }
    }

    serde_json::from_slice::<WechatTokenResponse>(&body)
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

/// 根据微信 openid 查找或创建用户
async fn find_or_create_user_by_wechat(
    db: &crate::infrastructure::persistence::database::Database,
    openid: &str,
) -> Result<User, StatusCode> {
    // 查找 social_bindings
    let rows = sqlx::query(
        "SELECT user_id FROM social_bindings WHERE provider = 'wechat' AND provider_user_id = ? LIMIT 1",
    )
    .bind(openid)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(row) = rows.into_iter().next() {
        let user_id: String = row
            .try_get("user_id")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    .bind(format!("wechat_{}", &openid[..8])) // 临时用户名
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

    user_rows
        .into_iter()
        .next()
        .map(user_from_row)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn user_from_row(row: sqlx::sqlite::SqliteRow) -> User {
    User {
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

/// 存储社交账号绑定
async fn save_social_binding(
    db: &crate::infrastructure::persistence::database::Database,
    user_id: &str,
    provider: &str,
    provider_user_id: &str,
) -> Result<(), StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO social_bindings (id, user_id, provider, provider_user_id, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(provider, provider_user_id) DO NOTHING"#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(provider)
    .bind(provider_user_id)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

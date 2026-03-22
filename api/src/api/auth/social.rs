// 第三方登录模块
// 支持微信扫码登录

use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::api::AppState;
use crate::dto::response::ApiResponse;
use crate::infrastructure::config::get as get_config;

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
    let state = params
        .state
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // 授权回调地址
    let encoded_redirect = urlencoding::encode(&redirect_uri);

    // 微信授权 URL
    let authorize_url = format!(
        "https://open.weixin.qq.com/connect/qrconnect?appid={}&redirect_uri={}&response_type=code&scope=snsapi_login&state={}#wechat_redirect",
        app_id, encoded_redirect, state
    );

    // 二维码页面 URL
    let qrcode_url = format!("https://login.weixin.qq.com/l/{}", state);

    ApiResponse::success(WeChatQRCodeResponse {
        qrcode_url,
        authorize_url,
        state,
    })
}

/// 微信回调处理
async fn wechat_callback(
    State(_state): State<AppState>,
    Query(params): Query<WeChatCallbackQuery>,
) -> Json<ApiResponse<WeChatCallbackResponse>> {
    if let Some(error) = params.error_description {
        return ApiResponse::error(format!("微信授权失败: {}", error));
    }

    let code = match params.code {
        Some(c) => c,
        None => {
            return ApiResponse::error("授权码不存在".to_string());
        }
    };

    // TODO: 使用 code 换取 access_token
    // 这里返回前端，让前端调用 login 接口

    ApiResponse::success(WeChatCallbackResponse {
        code,
        message: "请使用 code 调用登录接口".to_string(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WeChatCallbackResponse {
    pub code: String,
    pub message: String,
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

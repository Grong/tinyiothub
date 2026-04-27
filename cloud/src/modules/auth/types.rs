use serde::{Deserialize, Serialize};

/// 登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginResponse {
    /// JWT 访问令牌
    pub access_token: String,
    /// 令牌类型（通常是 "Bearer"）
    pub token_type: String,
    /// 令牌过期时间（秒）
    pub expires_in: i64,
    /// 用户信息
    pub user_info: UserInfo,
    /// 当前用户的默认 workspace ID（用于 SSE 等场景）
    pub workspace_id: Option<String>,
}

/// 用户信息（登录响应中的用户信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub date_last_logon: Option<String>,
    pub is_disabled: bool,
    pub parent_id: Option<String>,
}

impl From<crate::modules::user::User> for UserInfo {
    fn from(user: crate::modules::user::User) -> Self {
        Self {
            id: user.id.clone(),
            name: user.get_display_name().to_string(),
            phone: user.phone.clone(),
            email: user.email.clone(),
            avatar: None, // 暂时没有头像字段
            date_last_logon: user.last_login_at.clone(),
            is_disabled: !user.is_enabled, // 注意：is_disabled 是 is_enabled 的反值
            parent_id: user.parent_id.clone(),
        }
    }
}

/// 刷新令牌请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// 刷新令牌响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

use serde::{Deserialize, Serialize};

/// 用户实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: bool,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// 用户DTO（用于API响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserDto {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: bool,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// 用户统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserStatisticsNew {
    pub total_users: i64,
    pub enabled_users: i64,
    pub disabled_users: i64,
    pub recent_logins: i64,
}

/// 用户查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserQueryParams {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建用户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
}

/// 更新用户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
}

/// 用户登录请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 修改密码请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

impl User {
    /// 获取用户显示名称
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.username)
    }

    /// 检查用户是否启用
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// 检查用户是否有父用户
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// 将用户列表转换为DTO列表
    pub fn to_dto_list(users: Vec<User>) -> Vec<UserDto> {
        users.into_iter().map(|user| user.to_dto()).collect()
    }

    /// 转换为DTO
    pub fn to_dto(&self) -> UserDto {
        UserDto {
            id: self.id.clone(),
            username: self.username.clone(),
            email: self.email.clone(),
            phone: self.phone.clone(),
            display_name: self.display_name.clone(),
            is_enabled: self.is_enabled,
            parent_id: self.parent_id.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_login_at: self.last_login_at.clone(),
        }
    }
}

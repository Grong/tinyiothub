//! user 写入/请求契约类型（自 db 归位 core — handler 与 repo 共享的值类型）。

use serde::{Deserialize, Serialize};

/// Create user request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    #[serde(alias = "name")]
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
}

/// Update user request
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

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Change password request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

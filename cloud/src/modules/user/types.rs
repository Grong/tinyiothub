use serde::{Deserialize, Serialize};

/// User entity
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

/// UserDTO (for API responses)
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

/// User statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserStatistics {
    pub total_users: i64,
    pub enabled_users: i64,
    pub disabled_users: i64,
    pub recent_logins: i64,
}

/// Backward compatibility alias
pub type UserStatisticsNew = UserStatistics;

/// User query parameters
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

/// Create user request
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

impl User {
    /// Get user display name
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.username)
    }

    /// Check if user is enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Check if user has parent
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Convert user to DTO
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

    /// Convert user list to DTO list
    pub fn to_dto_list(users: Vec<User>) -> Vec<UserDto> {
        users.into_iter().map(|user| user.to_dto()).collect()
    }
}

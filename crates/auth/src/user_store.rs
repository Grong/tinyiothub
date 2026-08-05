//! Identity-store seam — the user lookup/authenticate/create surface the
//! auth handlers need.
//!
//! The user domain lives in `tinyiothub_user` (Task 17a). Cloud implements
//! this trait for the `UserServiceAuthAdapter` newtype in `app_state.rs`
//! (orphan rule: both this trait and `UserService` are foreign to cloud)
//! and maps between `tinyiothub_user::User`/`CreateUserRequest` and the
//! byte-identical mirror types below.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// User entity mirror (byte-identical field set to `modules::user::User`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthUser {
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

impl AuthUser {
    /// Get user display name
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.username)
    }

    /// Check if user is enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }
}

/// Create user request mirror (byte-identical wire format to
/// `modules::user::types::CreateUserRequest` — the register endpoint
/// deserializes this directly).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthCreateUserRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    #[serde(alias = "name")]
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
}

/// Identity store consumed by the auth handlers. Errors are strings so the
/// UNIQUE-constraint message checks in the register flow keep working
/// byte-identically.
#[async_trait]
pub trait AuthUserStore: Send + Sync {
    async fn authenticate(&self, username: &str, password: &str) -> Result<Option<AuthUser>, String>;

    async fn get_user_by_id(&self, id: &str) -> Result<Option<AuthUser>, String>;

    async fn update_last_login(&self, id: &str) -> Result<(), String>;

    async fn exists_by_username(&self, username: &str) -> Result<bool, String>;

    async fn exists_by_phone(&self, phone: &str) -> Result<bool, String>;

    async fn exists_by_email(&self, email: &str) -> Result<bool, String>;

    async fn create_user(&self, request: &AuthCreateUserRequest) -> Result<AuthUser, String>;
}

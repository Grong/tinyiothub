use std::sync::Arc;

use crate::{
    dto::entity::user::{
        CreateUserRequest, UpdateUserRequest, User, UserQueryParams, UserStatisticsNew,
    },
    shared::error::{Error, Result},
};

use super::repository::{UserCriteria, UserRepository, UserSortBy, UserSortOrder};

/// User domain service
pub struct UserService {
    repository: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repository: Arc<dyn UserRepository>) -> Self {
        Self { repository }
    }

    /// List users with filters and pagination
    pub async fn list_users(
        &self,
        enabled: Option<bool>,
        search: Option<String>,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<User>, i64)> {
        let users = self.repository.find_with_filters(enabled, search.clone(), page, page_size).await?;
        let criteria = UserCriteria {
            is_enabled: enabled,
            search_text: search,
            ..Default::default()
        };
        let total = self.repository.count(&criteria).await?;
        Ok((users, total))
    }

    /// Create a new user (hashes password before storing)
    pub async fn create_user(&self, request: &CreateUserRequest) -> Result<User> {
        if request.username.trim().is_empty() {
            return Err(Error::ValidationError("用户名不能为空".to_string()));
        }
        if request.password.len() < 8 {
            return Err(Error::ValidationError("密码长度不能少于8位".to_string()));
        }

        let password_hash = crate::shared::utils::password::hash_password(&request.password)
            .map_err(|e| Error::ValidationError(format!("密码哈希失败: {}", e)))?;

        let hashed_request = CreateUserRequest {
            username: request.username.clone(),
            password: password_hash,
            email: request.email.clone(),
            phone: request.phone.clone(),
            display_name: request.display_name.clone(),
            is_enabled: request.is_enabled,
            parent_id: request.parent_id.clone(),
        };

        self.repository.create(&hashed_request).await
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>> {
        self.repository.find_by_id(id).await
    }

    /// Get user by username
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        self.repository.find_by_username(username).await
    }

    /// Update user
    pub async fn update_user(&self, id: &str, request: &UpdateUserRequest) -> Result<User> {
        self.repository.update(id, request).await
    }

    /// Delete user
    pub async fn delete_user(&self, id: &str) -> Result<u64> {
        self.repository.delete(id).await
    }

    /// Update enabled status
    pub async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<User> {
        self.repository.update_enabled_status(id, enabled).await
    }

    /// Change password (requires old password verification)
    pub async fn change_password(
        &self,
        id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<bool> {
        let user = self.repository.find_by_id(id).await?.ok_or(Error::NotFound)?;

        match user.verify_password(old_password) {
            Ok(true) => {
                let new_hash = crate::shared::utils::password::hash_password(new_password)
                    .map_err(|e| Error::ValidationError(format!("密码哈希失败: {}", e)))?;
                self.repository.update_password(id, &new_hash).await?;
                Ok(true)
            }
            Ok(false) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Update password (admin override)
    pub async fn update_password(&self, id: &str, new_password: &str) -> Result<()> {
        let new_hash = crate::shared::utils::password::hash_password(new_password)
            .map_err(|e| Error::ValidationError(format!("密码哈希失败: {}", e)))?;
        self.repository.update_password(id, &new_hash).await
    }

    /// Authenticate user by username and password
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<Option<User>> {
        if let Some(user) = self.repository.find_by_username(username).await? {
            match user.verify_password(password) {
                Ok(true) if user.is_enabled => return Ok(Some(user)),
                Ok(_) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    /// Update last login time
    pub async fn update_last_login(&self, id: &str) -> Result<()> {
        self.repository.update_last_login(id).await
    }

    /// Get user statistics
    pub async fn get_user_statistics(&self) -> Result<UserStatisticsNew> {
        self.repository.get_user_statistics().await
    }

    /// Find all users with query params
    pub async fn find_all(&self, params: &UserQueryParams) -> Result<Vec<User>> {
        let criteria = params_to_criteria(params);
        self.repository.find_all(&criteria).await
    }

    /// Check if username exists
    pub async fn exists_by_username(&self, username: &str) -> Result<bool> {
        self.repository.exists_by_username(username).await
    }

    /// Check if email exists
    pub async fn exists_by_email(&self, email: &str) -> Result<bool> {
        self.repository.exists_by_email(email).await
    }
}

/// Convert UserQueryParams to UserCriteria
fn params_to_criteria(params: &UserQueryParams) -> UserCriteria {
    UserCriteria {
        username: params.username.clone(),
        email: params.email.clone(),
        display_name: params.display_name.clone(),
        is_enabled: params.is_enabled,
        parent_id: params.parent_id.clone(),
        search_text: None,
        sort_by: UserSortBy::CreatedAt,
        sort_order: UserSortOrder::Descending,
        limit: params.page_size,
        offset: params.page.map(|p| p.saturating_sub(1) * params.page_size.unwrap_or(0)),
    }
}

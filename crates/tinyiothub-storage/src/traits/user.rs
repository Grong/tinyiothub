use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use tinyiothub_core::models::user::{CreateUserRequest, UpdateUserRequest, User, UserStatisticsNew};
use tinyiothub_core::error::Result;

/// Repository interface for user persistence (defined in domain layer)
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Find a user by its ID
    async fn find_by_id(&self, id: &str) -> Result<Option<User>>;

    /// Find a user by its username
    async fn find_by_username(&self, username: &str) -> Result<Option<User>>;

    /// Find a user by its email
    async fn find_by_email(&self, email: &str) -> Result<Option<User>>;

    /// Find users matching the given criteria
    async fn find_all(&self, criteria: &UserCriteria) -> Result<Vec<User>>;

    /// Count users matching the given criteria
    async fn count(&self, criteria: &UserCriteria) -> Result<i64>;

    /// Create a new user
    async fn create(&self, request: &CreateUserRequest) -> Result<User>;

    /// Update an existing user
    async fn update(&self, id: &str, request: &UpdateUserRequest) -> Result<User>;

    /// Delete a user by its ID
    async fn delete(&self, id: &str) -> Result<u64>;

    /// Find users with enabled/search filters and pagination
    async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<String>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<User>>;

    /// Check if a user with the given username exists
    async fn exists_by_username(&self, username: &str) -> Result<bool>;

    /// Check if a user with the given email exists
    async fn exists_by_email(&self, email: &str) -> Result<bool>;

    /// Update the enabled status of a user
    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<User>;

    /// Update the password of a user (already hashed)
    async fn update_password(&self, id: &str, hashed_password: &str) -> Result<()>;

    /// Update the last login time of a user
    async fn update_last_login(&self, id: &str) -> Result<()>;

    /// Get user statistics
    async fn get_user_statistics(&self) -> Result<UserStatisticsNew>;
}

/// Criteria for querying users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCriteria {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
    pub search_text: Option<String>,
    pub sort_by: UserSortBy,
    pub sort_order: UserSortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for users
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UserSortBy {
    CreatedAt,
    Username,
}

/// Sort order for users
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UserSortOrder {
    Ascending,
    Descending,
}

impl Default for UserSortBy {
    fn default() -> Self {
        Self::CreatedAt
    }
}

impl Default for UserSortOrder {
    fn default() -> Self {
        Self::Descending
    }
}

impl Default for UserCriteria {
    fn default() -> Self {
        Self {
            username: None,
            email: None,
            display_name: None,
            is_enabled: None,
            parent_id: None,
            search_text: None,
            sort_by: UserSortBy::default(),
            sort_order: UserSortOrder::default(),
            limit: None,
            offset: None,
        }
    }
}

impl UserCriteria {
    /// Create a new criteria builder
    pub fn builder() -> UserCriteriaBuilder {
        UserCriteriaBuilder::new()
    }

    /// Filter by username
    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }

    /// Filter by email
    pub fn with_email(mut self, email: String) -> Self {
        self.email = Some(email);
        self
    }

    /// Filter by display name
    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }

    /// Filter by enabled status
    pub fn with_is_enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = Some(is_enabled);
        self
    }

    /// Filter by parent ID
    pub fn with_parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Filter by search text
    pub fn with_search_text(mut self, text: String) -> Self {
        self.search_text = Some(text);
        self
    }

    /// Set sorting
    pub fn with_sort(mut self, sort_by: UserSortBy, sort_order: UserSortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    /// Set pagination
    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for UserCriteria
pub struct UserCriteriaBuilder {
    criteria: UserCriteria,
}

impl UserCriteriaBuilder {
    pub fn new() -> Self {
        Self {
            criteria: UserCriteria::default(),
        }
    }

    pub fn username(mut self, username: String) -> Self {
        self.criteria.username = Some(username);
        self
    }

    pub fn email(mut self, email: String) -> Self {
        self.criteria.email = Some(email);
        self
    }

    pub fn display_name(mut self, display_name: String) -> Self {
        self.criteria.display_name = Some(display_name);
        self
    }

    pub fn is_enabled(mut self, is_enabled: bool) -> Self {
        self.criteria.is_enabled = Some(is_enabled);
        self
    }

    pub fn parent_id(mut self, parent_id: String) -> Self {
        self.criteria.parent_id = Some(parent_id);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn sort_by(mut self, sort_by: UserSortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: UserSortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> UserCriteria {
        self.criteria
    }
}

impl Default for UserCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criteria_builder() {
        let criteria = UserCriteria::builder()
            .username("admin".to_string())
            .email("admin@example.com".to_string())
            .is_enabled(true)
            .sort_by(UserSortBy::Username)
            .sort_order(UserSortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.username, Some("admin".to_string()));
        assert_eq!(criteria.email, Some("admin@example.com".to_string()));
        assert_eq!(criteria.is_enabled, Some(true));
        assert!(matches!(criteria.sort_by, UserSortBy::Username));
        assert!(matches!(criteria.sort_order, UserSortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let criteria = UserCriteria::default()
            .with_username("user-01".to_string())
            .with_is_enabled(false)
            .with_sort(UserSortBy::CreatedAt, UserSortOrder::Descending)
            .with_pagination(50, 10);

        assert_eq!(criteria.username, Some("user-01".to_string()));
        assert_eq!(criteria.is_enabled, Some(false));
        assert!(matches!(criteria.sort_by, UserSortBy::CreatedAt));
        assert!(matches!(criteria.sort_order, UserSortOrder::Descending));
        assert_eq!(criteria.limit, Some(50));
        assert_eq!(criteria.offset, Some(10));
    }
}

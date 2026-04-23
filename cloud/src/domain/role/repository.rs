use async_trait::async_trait;

use crate::dto::entity::role::{CreateRoleRequest, Role, RoleQueryParams, RoleStats, UpdateRoleRequest};
use tinyiothub_core::error::Result;

#[async_trait]
pub trait RoleRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Role>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Role>>;
    async fn create(&self, request: &CreateRoleRequest) -> Result<Role>;
    async fn update(&self, id: &str, request: &UpdateRoleRequest) -> Result<Role>;
    async fn delete(&self, id: &str) -> Result<u64>;
    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64>;
    async fn find_all(&self, params: &RoleQueryParams) -> Result<Vec<Role>>;
    async fn count(&self, params: &RoleQueryParams) -> Result<i64>;
    async fn get_stats(&self) -> Result<RoleStats>;
    async fn find_admin_roles(&self) -> Result<Vec<Role>>;
    async fn find_user_roles(&self) -> Result<Vec<Role>>;
    async fn exists_by_name(&self, name: &str) -> Result<bool>;
    async fn exists_by_name_exclude_id(&self, name: &str, exclude_id: &str) -> Result<bool>;
    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Role>>;
    async fn is_administrator_role(&self, id: &str) -> Result<bool>;
    async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>>;
    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool>;
}

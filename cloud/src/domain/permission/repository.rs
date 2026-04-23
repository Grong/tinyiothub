use async_trait::async_trait;

use crate::dto::entity::permission::{
    CreatePermissionGroupRequest, CreatePermissionRequest, Permission, PermissionGroup,
    PermissionQuery, UpdatePermissionRequest,
};
use tinyiothub_core::error::Result;

#[async_trait]
pub trait PermissionRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Permission>>;
    async fn find_by_code(&self, code: &str) -> Result<Option<Permission>>;
    async fn create(&self, request: &CreatePermissionRequest) -> Result<Permission>;
    async fn update(&self, id: &str, request: &UpdatePermissionRequest) -> Result<Permission>;
    async fn delete(&self, id: &str) -> Result<u64>;
    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64>;
    async fn find_all(&self, params: &PermissionQuery) -> Result<Vec<Permission>>;
    async fn count(&self, params: &PermissionQuery) -> Result<i64>;
    async fn find_by_resource_type(&self, resource_type: &str) -> Result<Vec<Permission>>;
    async fn find_by_action_type(&self, action_type: &str) -> Result<Vec<Permission>>;
    async fn find_system_permissions(&self) -> Result<Vec<Permission>>;
    async fn find_root_permissions(&self) -> Result<Vec<Permission>>;
    async fn find_by_parent_id(&self, parent_id: &str) -> Result<Vec<Permission>>;
    async fn exists_by_code(&self, code: &str) -> Result<bool>;
    async fn exists_by_code_exclude_id(&self, code: &str, exclude_id: &str) -> Result<bool>;
    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Permission>>;
}

#[async_trait]
pub trait PermissionGroupRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<PermissionGroup>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<PermissionGroup>>;
    async fn create(&self, request: &CreatePermissionGroupRequest) -> Result<PermissionGroup>;
    async fn delete(&self, id: &str) -> Result<u64>;
    async fn find_all(&self) -> Result<Vec<PermissionGroup>>;
}

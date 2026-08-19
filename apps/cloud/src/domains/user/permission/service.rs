use std::sync::Arc;

use tinyiothub_core::error::Result;

use tinyiothub_core::models::permission::{
    CreatePermissionGroupRequest, CreatePermissionRequest, UpdatePermissionRequest,
};
use tinyiothub_storage::Db;
use tinyiothub_storage::permission::{Permission, PermissionGroup, PermissionQuery};

pub struct PermissionService {
    db: Arc<Db>,
}

impl PermissionService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn find_permission_by_id(&self, id: &str) -> Result<Option<Permission>> {
        self.db.find_permission_by_id(id).await
    }

    pub async fn find_permission_by_code(&self, code: &str) -> Result<Option<Permission>> {
        self.db.find_permission_by_code(code).await
    }

    pub async fn create_permission(&self, request: &CreatePermissionRequest) -> Result<Permission> {
        self.db.create_permission(request).await
    }

    pub async fn update_permission(&self, id: &str, request: &UpdatePermissionRequest) -> Result<Permission> {
        self.db.update_permission(id, request).await
    }

    pub async fn delete_permission(&self, id: &str) -> Result<u64> {
        self.db.delete_permission(id).await
    }

    pub async fn delete_permissions_by_ids(&self, ids: &[String]) -> Result<u64> {
        self.db.delete_permissions_by_ids(ids).await
    }

    pub async fn find_all_permissions(&self, params: &PermissionQuery) -> Result<Vec<Permission>> {
        self.db.find_permissions(params).await
    }

    pub async fn count_permissions(&self, params: &PermissionQuery) -> Result<i64> {
        self.db.count_permissions(params).await
    }

    pub async fn find_permissions_by_resource_type(&self, resource_type: &str) -> Result<Vec<Permission>> {
        self.db.find_permissions_by_resource_type(resource_type).await
    }

    pub async fn find_permissions_by_action_type(&self, action_type: &str) -> Result<Vec<Permission>> {
        self.db.find_permissions_by_action_type(action_type).await
    }

    pub async fn find_system_permissions(&self) -> Result<Vec<Permission>> {
        self.db.find_system_permissions().await
    }

    pub async fn find_root_permissions(&self) -> Result<Vec<Permission>> {
        self.db.find_root_permissions().await
    }

    pub async fn find_permissions_by_parent_id(&self, parent_id: &str) -> Result<Vec<Permission>> {
        self.db.find_permissions_by_parent_id(parent_id).await
    }

    pub async fn permission_exists_by_code(&self, code: &str) -> Result<bool> {
        self.db.permission_exists_by_code(code).await
    }

    pub async fn permission_exists_by_code_exclude_id(&self, code: &str, exclude_id: &str) -> Result<bool> {
        self.db.permission_exists_by_code_exclude_id(code, exclude_id).await
    }

    pub async fn find_permissions_by_ids(&self, ids: &[String]) -> Result<Vec<Permission>> {
        self.db.find_permissions_by_ids(ids).await
    }

    pub async fn find_group_by_id(&self, id: &str) -> Result<Option<PermissionGroup>> {
        self.db.find_permission_group_by_id(id).await
    }

    pub async fn find_group_by_name(&self, name: &str) -> Result<Option<PermissionGroup>> {
        self.db.find_permission_group_by_name(name).await
    }

    pub async fn create_group(&self, request: &CreatePermissionGroupRequest) -> Result<PermissionGroup> {
        self.db.create_permission_group(request).await
    }

    pub async fn delete_group(&self, id: &str) -> Result<u64> {
        self.db.delete_permission_group(id).await
    }

    pub async fn find_all_groups(&self) -> Result<Vec<PermissionGroup>> {
        self.db.find_all_permission_groups().await
    }
}

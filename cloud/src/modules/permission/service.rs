use std::sync::Arc;

use tinyiothub_core::error::Result;

use super::{
    repo::{PermissionGroupRepository, PermissionRepository},
    types::{
        CreatePermissionGroupRequest, CreatePermissionRequest, Permission, PermissionGroup,
        PermissionQuery, UpdatePermissionRequest,
    },
};

pub struct PermissionService {
    permission_repository: Arc<dyn PermissionRepository>,
    permission_group_repository: Arc<dyn PermissionGroupRepository>,
}

impl PermissionService {
    pub fn new(
        permission_repository: Arc<dyn PermissionRepository>,
        permission_group_repository: Arc<dyn PermissionGroupRepository>,
    ) -> Self {
        Self { permission_repository, permission_group_repository }
    }

    pub async fn find_permission_by_id(&self, id: &str) -> Result<Option<Permission>> {
        self.permission_repository.find_by_id(id).await
    }

    pub async fn find_permission_by_code(&self, code: &str) -> Result<Option<Permission>> {
        self.permission_repository.find_by_code(code).await
    }

    pub async fn create_permission(&self, request: &CreatePermissionRequest) -> Result<Permission> {
        self.permission_repository.create(request).await
    }

    pub async fn update_permission(
        &self,
        id: &str,
        request: &UpdatePermissionRequest,
    ) -> Result<Permission> {
        self.permission_repository.update(id, request).await
    }

    pub async fn delete_permission(&self, id: &str) -> Result<u64> {
        self.permission_repository.delete(id).await
    }

    pub async fn delete_permissions_by_ids(&self, ids: &[String]) -> Result<u64> {
        self.permission_repository.delete_by_ids(ids).await
    }

    pub async fn find_all_permissions(&self, params: &PermissionQuery) -> Result<Vec<Permission>> {
        self.permission_repository.find_all(params).await
    }

    pub async fn count_permissions(&self, params: &PermissionQuery) -> Result<i64> {
        self.permission_repository.count(params).await
    }

    pub async fn find_permissions_by_resource_type(
        &self,
        resource_type: &str,
    ) -> Result<Vec<Permission>> {
        self.permission_repository.find_by_resource_type(resource_type).await
    }

    pub async fn find_permissions_by_action_type(
        &self,
        action_type: &str,
    ) -> Result<Vec<Permission>> {
        self.permission_repository.find_by_action_type(action_type).await
    }

    pub async fn find_system_permissions(&self) -> Result<Vec<Permission>> {
        self.permission_repository.find_system_permissions().await
    }

    pub async fn find_root_permissions(&self) -> Result<Vec<Permission>> {
        self.permission_repository.find_root_permissions().await
    }

    pub async fn find_permissions_by_parent_id(&self, parent_id: &str) -> Result<Vec<Permission>> {
        self.permission_repository.find_by_parent_id(parent_id).await
    }

    pub async fn permission_exists_by_code(&self, code: &str) -> Result<bool> {
        self.permission_repository.exists_by_code(code).await
    }

    pub async fn permission_exists_by_code_exclude_id(
        &self,
        code: &str,
        exclude_id: &str,
    ) -> Result<bool> {
        self.permission_repository.exists_by_code_exclude_id(code, exclude_id).await
    }

    pub async fn find_permissions_by_ids(&self, ids: &[String]) -> Result<Vec<Permission>> {
        self.permission_repository.find_by_ids(ids).await
    }

    pub async fn find_group_by_id(&self, id: &str) -> Result<Option<PermissionGroup>> {
        self.permission_group_repository.find_by_id(id).await
    }

    pub async fn find_group_by_name(&self, name: &str) -> Result<Option<PermissionGroup>> {
        self.permission_group_repository.find_by_name(name).await
    }

    pub async fn create_group(
        &self,
        request: &CreatePermissionGroupRequest,
    ) -> Result<PermissionGroup> {
        self.permission_group_repository.create(request).await
    }

    pub async fn delete_group(&self, id: &str) -> Result<u64> {
        self.permission_group_repository.delete(id).await
    }

    pub async fn find_all_groups(&self) -> Result<Vec<PermissionGroup>> {
        self.permission_group_repository.find_all().await
    }
}

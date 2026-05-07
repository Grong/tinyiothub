use std::sync::Arc;

use tinyiothub_core::error::Result;

use super::{
    repo::RoleRepository,
    types::{CreateRoleRequest, Role, RoleQueryParams, RoleStats, UpdateRoleRequest},
};

pub struct RoleService {
    role_repository: Arc<dyn RoleRepository>,
}

impl RoleService {
    pub fn new(role_repository: Arc<dyn RoleRepository>) -> Self {
        Self { role_repository }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Role>> {
        self.role_repository.find_by_id(id).await
    }

    pub async fn find_by_name(
        &self,
        name: &str,
        workspace_id: Option<&str>,
    ) -> Result<Option<Role>> {
        self.role_repository.find_by_name(name, workspace_id).await
    }

    pub async fn create(&self, request: &CreateRoleRequest) -> Result<Role> {
        self.role_repository.create(request).await
    }

    pub async fn update(&self, id: &str, request: &UpdateRoleRequest) -> Result<Role> {
        self.role_repository.update(id, request).await
    }

    pub async fn delete(&self, id: &str) -> Result<u64> {
        self.role_repository.delete(id).await
    }

    pub async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        self.role_repository.delete_by_ids(ids).await
    }

    pub async fn find_all(&self, params: &RoleQueryParams) -> Result<Vec<Role>> {
        self.role_repository.find_all(params).await
    }

    pub async fn count(&self, params: &RoleQueryParams) -> Result<i64> {
        self.role_repository.count(params).await
    }

    pub async fn get_stats(&self, workspace_id: Option<&str>) -> Result<RoleStats> {
        self.role_repository.get_stats(workspace_id).await
    }

    pub async fn find_admin_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        self.role_repository.find_admin_roles(workspace_id).await
    }

    pub async fn find_user_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        self.role_repository.find_user_roles(workspace_id).await
    }

    pub async fn exists_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<bool> {
        self.role_repository.exists_by_name(name, workspace_id).await
    }

    pub async fn exists_by_name_exclude_id(
        &self,
        name: &str,
        exclude_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<bool> {
        self.role_repository.exists_by_name_exclude_id(name, exclude_id, workspace_id).await
    }

    pub async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Role>> {
        self.role_repository.find_by_ids(ids).await
    }

    pub async fn find_roles_by_user_id(&self, user_id: &str) -> Result<Vec<Role>> {
        self.role_repository.find_roles_by_user_id(user_id).await
    }

    pub async fn is_administrator_role(&self, id: &str) -> Result<bool> {
        self.role_repository.is_administrator_role(id).await
    }

    pub async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        workspace_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>> {
        self.role_repository.find_with_filters(enabled, search, workspace_id, page, page_size).await
    }

    pub async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool> {
        self.role_repository.update_enabled_status(id, enabled).await
    }

    pub async fn get_permissions(&self, role_id: &str) -> Result<Vec<String>> {
        self.role_repository.get_permissions(role_id).await
    }

    pub async fn update_permissions(&self, role_id: &str, permission_ids: &[String]) -> Result<()> {
        self.role_repository.update_permissions(role_id, permission_ids).await
    }
}

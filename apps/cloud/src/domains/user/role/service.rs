use std::sync::Arc;

use tinyiothub_core::error::Result;

use tinyiothub_core::models::role::{CreateRoleRequest, UpdateRoleRequest};
use tinyiothub_storage::Db;
use tinyiothub_storage::role::{Role, RoleQueryParams, RoleStats};

pub struct RoleService {
    db: Arc<Db>,
}

impl RoleService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Role>> {
        self.db.find_role_by_id(id).await
    }

    pub async fn find_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<Option<Role>> {
        self.db.find_role_by_name(name, workspace_id).await
    }

    pub async fn create(&self, request: &CreateRoleRequest) -> Result<Role> {
        self.db.create_role(request).await
    }

    pub async fn update(&self, id: &str, request: &UpdateRoleRequest) -> Result<Role> {
        self.db.update_role(id, request).await
    }

    pub async fn delete(&self, id: &str) -> Result<u64> {
        self.db.delete_role(id).await
    }

    pub async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        self.db.delete_roles_by_ids(ids).await
    }

    pub async fn find_all(&self, params: &RoleQueryParams) -> Result<Vec<Role>> {
        self.db.find_roles(params).await
    }

    pub async fn count(&self, params: &RoleQueryParams) -> Result<i64> {
        self.db.count_roles(params).await
    }

    pub async fn get_stats(&self, workspace_id: Option<&str>) -> Result<RoleStats> {
        self.db.get_role_stats(workspace_id).await
    }

    pub async fn find_admin_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        self.db.find_admin_roles(workspace_id).await
    }

    pub async fn find_user_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        self.db.find_user_roles(workspace_id).await
    }

    pub async fn exists_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<bool> {
        self.db.role_exists_by_name(name, workspace_id).await
    }

    pub async fn exists_by_name_exclude_id(
        &self,
        name: &str,
        exclude_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<bool> {
        self.db
            .role_exists_by_name_exclude_id(name, exclude_id, workspace_id)
            .await
    }

    pub async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Role>> {
        self.db.find_roles_by_ids(ids).await
    }

    pub async fn find_roles_by_user_id(&self, user_id: &str) -> Result<Vec<Role>> {
        self.db.find_roles_by_user_id(user_id).await
    }

    pub async fn is_administrator_role(&self, id: &str) -> Result<bool> {
        self.db.is_administrator_role(id).await
    }

    pub async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        workspace_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>> {
        self.db
            .find_roles_with_filters(enabled, search, workspace_id, page, page_size)
            .await
    }

    pub async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool> {
        self.db.update_role_enabled_status(id, enabled).await
    }

    pub async fn get_permissions(&self, role_id: &str) -> Result<Vec<String>> {
        self.db.get_role_permissions(role_id).await
    }

    pub async fn update_permissions(&self, role_id: &str, permission_ids: &[String]) -> Result<()> {
        self.db.update_role_permissions(role_id, permission_ids).await
    }
}

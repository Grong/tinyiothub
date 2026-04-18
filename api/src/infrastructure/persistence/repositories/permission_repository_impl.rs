use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

use crate::domain::permission::repository::{PermissionGroupRepository, PermissionRepository};
use crate::dto::entity::permission::{
    CreatePermissionGroupRequest, CreatePermissionRequest, Permission, PermissionGroup,
    PermissionQuery, UpdatePermissionRequest,
};
use crate::infrastructure::persistence::database::Database;
use crate::shared::error::Result;

pub struct SqlitePermissionRepository {
    database: Database,
}

impl SqlitePermissionRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl PermissionRepository for SqlitePermissionRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Permission>> {
        let permission = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(permission)
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<Permission>> {
        let permission = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE code = ?"
        )
        .bind(code)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(permission)
    }

    async fn create(&self, request: &CreatePermissionRequest) -> Result<Permission> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let is_system = request.is_system.unwrap_or(false);

        sqlx::query(
            r#"
            INSERT INTO permissions (id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.code)
        .bind(&request.description)
        .bind(&request.resource_type)
        .bind(&request.action_type)
        .bind(is_system)
        .bind(&request.parent_id)
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn update(&self, id: &str, request: &UpdatePermissionRequest) -> Result<Permission> {
        let mut query = QueryBuilder::new("UPDATE permissions SET ");
        let mut has_updates = false;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if let Some(name) = &request.name {
            if has_updates {
                query.push(", ");
            }
            query.push("name = ").push_bind(name);
            has_updates = true;
        }

        if let Some(description) = &request.description {
            if has_updates {
                query.push(", ");
            }
            query.push("description = ").push_bind(description);
            has_updates = true;
        }

        if let Some(resource_type) = &request.resource_type {
            if has_updates {
                query.push(", ");
            }
            query.push("resource_type = ").push_bind(resource_type);
            has_updates = true;
        }

        if let Some(action_type) = &request.action_type {
            if has_updates {
                query.push(", ");
            }
            query.push("action_type = ").push_bind(action_type);
            has_updates = true;
        }

        if let Some(parent_id) = &request.parent_id {
            if has_updates {
                query.push(", ");
            }
            query.push("parent_id = ").push_bind(parent_id);
            has_updates = true;
        }

        if has_updates {
            query.push(", updated_at = ").push_bind(&now);
        } else {
            return self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(crate::shared::error::Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        if let Some(permission) = self.find_by_id(id).await? {
            if permission.is_system {
                return Err(crate::shared::error::Error::NotFound);
            }
        }

        let result =
            sqlx::query("DELETE FROM permissions WHERE id = ?").bind(id).execute(self.database.pool()).await?;

        Ok(result.rows_affected())
    }

    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM permissions WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(") AND is_system = 0");

        let result = query.build().execute(self.database.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn find_all(&self, params: &PermissionQuery) -> Result<Vec<Permission>> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE 1=1"
        );

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(code) = &params.code {
            query.push(" AND code LIKE ").push_bind(format!("%{}%", code));
        }

        if let Some(resource_type) = &params.resource_type {
            query.push(" AND resource_type = ").push_bind(resource_type);
        }

        if let Some(action_type) = &params.action_type {
            query.push(" AND action_type = ").push_bind(action_type);
        }

        if let Some(is_system) = params.is_system {
            query.push(" AND is_system = ").push_bind(is_system);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        query.push(" ORDER BY resource_type, action_type, name");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let permissions = query.build_query_as::<Permission>().fetch_all(self.database.pool()).await?;

        Ok(permissions)
    }

    async fn count(&self, params: &PermissionQuery) -> Result<i64> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM permissions WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(code) = &params.code {
            query.push(" AND code LIKE ").push_bind(format!("%{}%", code));
        }

        if let Some(resource_type) = &params.resource_type {
            query.push(" AND resource_type = ").push_bind(resource_type);
        }

        if let Some(action_type) = &params.action_type {
            query.push(" AND action_type = ").push_bind(action_type);
        }

        if let Some(is_system) = params.is_system {
            query.push(" AND is_system = ").push_bind(is_system);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    async fn find_by_resource_type(&self, resource_type: &str) -> Result<Vec<Permission>> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE resource_type = ? ORDER BY action_type, name"
        )
        .bind(resource_type)
        .fetch_all(self.database.pool())
        .await?;

        Ok(permissions)
    }

    async fn find_by_action_type(&self, action_type: &str) -> Result<Vec<Permission>> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE action_type = ? ORDER BY resource_type, name"
        )
        .bind(action_type)
        .fetch_all(self.database.pool())
        .await?;

        Ok(permissions)
    }

    async fn find_system_permissions(&self) -> Result<Vec<Permission>> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE is_system = 1 ORDER BY resource_type, action_type"
        )
        .fetch_all(self.database.pool())
        .await?;

        Ok(permissions)
    }

    async fn find_root_permissions(&self) -> Result<Vec<Permission>> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE parent_id IS NULL ORDER BY resource_type, action_type"
        )
        .fetch_all(self.database.pool())
        .await?;

        Ok(permissions)
    }

    async fn find_by_parent_id(&self, parent_id: &str) -> Result<Vec<Permission>> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE parent_id = ? ORDER BY action_type, name"
        )
        .bind(parent_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(permissions)
    }

    async fn exists_by_code(&self, code: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE code = ?")
            .bind(code)
            .fetch_one(self.database.pool())
            .await?;

        Ok(count > 0)
    }

    async fn exists_by_code_exclude_id(&self, code: &str, exclude_id: &str) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE code = ? AND id != ?")
                .bind(code)
                .bind(exclude_id)
                .fetch_one(self.database.pool())
                .await?;

        Ok(count > 0)
    }

    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Permission>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query = QueryBuilder::new(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE id IN ("
        );

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let permissions = query.build_query_as::<Permission>().fetch_all(self.database.pool()).await?;

        Ok(permissions)
    }
}

pub struct SqlitePermissionGroupRepository {
    database: Database,
}

impl SqlitePermissionGroupRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl PermissionGroupRepository for SqlitePermissionGroupRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<PermissionGroup>> {
        let group = sqlx::query_as::<_, PermissionGroup>(
            "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(group)
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<PermissionGroup>> {
        let group = sqlx::query_as::<_, PermissionGroup>(
            "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(group)
    }

    async fn create(&self, request: &CreatePermissionGroupRequest) -> Result<PermissionGroup> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let permissions_json =
            serde_json::to_string(&request.permission_ids).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO permission_groups (id, name, description, permissions, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&permissions_json)
        .bind(&now)
        .bind(&now)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(crate::shared::error::Error::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM permission_groups WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    async fn find_all(&self) -> Result<Vec<PermissionGroup>> {
        let groups = sqlx::query_as::<_, PermissionGroup>(
            "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups ORDER BY name"
        )
        .fetch_all(self.database.pool())
        .await?;

        Ok(groups)
    }
}

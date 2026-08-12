//! Role 持久化：角色（P-集中化 E4，自 user crate 迁入）。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};
use tinyiothub_core::error::Result;
use tinyiothub_core::models::role::{CreateRoleRequest, UpdateRoleRequest};

use crate::database::Database;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 仓储契约）— 自领域 crate 迁入
// ──────────────────────────────────────────────

/// Role entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: i32,
    pub workspace_id: Option<String>,
}

/// Role query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RoleQueryParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub workspace_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Role statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RoleStats {
    pub total_roles: i64,
    pub admin_roles: i64,
    pub user_roles: i64,
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            description: None,
            is_administrator: 0,
            workspace_id: None,
        }
    }
}

/// Backward compatibility alias
pub type RoleDto = Role;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_default() {
        let role = Role::default();
        assert!(!role.id.is_empty());
        assert!(role.name.is_empty());
        assert_eq!(role.description, None);
        assert_eq!(role.is_administrator, 0);
    }
}

// ──────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────

// ── Row type (internal) ─────────────────────────────────

#[derive(Debug, Clone, FromRow)]
struct RoleRow {
    id: String,
    name: String,
    description: Option<String>,
    is_administrator: i32,
    workspace_id: Option<String>,
}

impl From<RoleRow> for Role {
    fn from(row: RoleRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            is_administrator: row.is_administrator,
            workspace_id: row.workspace_id,
        }
    }
}

// ── SQLite implementation ───────────────────────────────

pub struct RoleRepository {
    database: crate::database::Database,
}

impl RoleRepository {
    pub fn new(database: crate::database::Database) -> Self {
        Self { database }
    }
}

impl RoleRepository {
    pub async fn find_by_id(&self, id: &str) -> Result<Option<Role>> {
        let row = sqlx::query_as::<_, RoleRow>(
            "SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn find_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<Option<Role>> {
        let mut query =
            QueryBuilder::new("SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE name = ");
        query.push_bind(name);

        if let Some(ws) = workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(ws)
                .push(" OR workspace_id IS NULL)");
        }

        let row = query
            .build_query_as::<RoleRow>()
            .fetch_optional(self.database.pool())
            .await?;

        Ok(row.map(Into::into))
    }

    pub async fn create(&self, request: &CreateRoleRequest) -> Result<Role> {
        let id = uuid::Uuid::new_v4().to_string();
        let is_admin = request.is_administrator.unwrap_or(0);

        sqlx::query(
            r#"
            INSERT INTO roles (id, name, description, is_administrator, workspace_id)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(is_admin)
        .bind(&request.workspace_id)
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    pub async fn update(&self, id: &str, request: &UpdateRoleRequest) -> Result<Role> {
        let mut query = QueryBuilder::new("UPDATE roles SET ");
        let mut has_updates = false;

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

        if let Some(is_administrator) = request.is_administrator {
            if has_updates {
                query.push(", ");
            }
            query.push("is_administrator = ").push_bind(is_administrator);
            has_updates = true;
        }

        if let Some(workspace_id) = &request.workspace_id {
            if has_updates {
                query.push(", ");
            }
            query.push("workspace_id = ").push_bind(workspace_id);
            has_updates = true;
        }

        if !has_updates {
            return self
                .find_by_id(id)
                .await?
                .ok_or(tinyiothub_core::error::Error::NotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(self.database.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(tinyiothub_core::error::Error::NotFound);
        }

        self.find_by_id(id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound)
    }

    pub async fn delete(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM roles WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete_by_ids(&self, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM roles WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query.build().execute(self.database.pool()).await?;
        Ok(result.rows_affected())
    }

    pub async fn find_all(&self, params: &RoleQueryParams) -> Result<Vec<Role>> {
        let mut query =
            QueryBuilder::new("SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(description) = &params.description {
            query
                .push(" AND description LIKE ")
                .push_bind(format!("%{}%", description));
        }

        if let Some(is_administrator) = params.is_administrator {
            query.push(" AND is_administrator = ").push_bind(is_administrator);
        }

        if let Some(workspace_id) = &params.workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(workspace_id)
                .push(" OR workspace_id IS NULL)");
        }

        query.push(" ORDER BY name");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let rows = query
            .build_query_as::<RoleRow>()
            .fetch_all(self.database.pool())
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn count(&self, params: &RoleQueryParams) -> Result<i64> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM roles WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(description) = &params.description {
            query
                .push(" AND description LIKE ")
                .push_bind(format!("%{}%", description));
        }

        if let Some(is_administrator) = params.is_administrator {
            query.push(" AND is_administrator = ").push_bind(is_administrator);
        }

        if let Some(workspace_id) = &params.workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(workspace_id)
                .push(" OR workspace_id IS NULL)");
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    pub async fn get_stats(&self, workspace_id: Option<&str>) -> Result<RoleStats> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT
                COUNT(*) as total_roles,
                COUNT(CASE WHEN is_administrator = 1 THEN 1 END) as admin_roles,
                COUNT(CASE WHEN is_administrator = 0 THEN 1 END) as user_roles
            FROM roles
            WHERE 1=1
            "#,
        );

        if let Some(ws) = workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(ws)
                .push(" OR workspace_id IS NULL)");
        }

        let row = query.build().fetch_one(self.database.pool()).await?;

        let stats = RoleStats {
            total_roles: row.get("total_roles"),
            admin_roles: row.get("admin_roles"),
            user_roles: row.get("user_roles"),
        };

        Ok(stats)
    }

    pub async fn find_admin_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE is_administrator = 1",
        );

        if let Some(ws) = workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(ws)
                .push(" OR workspace_id IS NULL)");
        }

        query.push(" ORDER BY name");

        let rows = query
            .build_query_as::<RoleRow>()
            .fetch_all(self.database.pool())
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn find_user_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE is_administrator = 0",
        );

        if let Some(ws) = workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(ws)
                .push(" OR workspace_id IS NULL)");
        }

        query.push(" ORDER BY name");

        let rows = query
            .build_query_as::<RoleRow>()
            .fetch_all(self.database.pool())
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn exists_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<bool> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) FROM roles WHERE name = ");
        query.push_bind(name);

        if let Some(ws) = workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(ws)
                .push(" OR workspace_id IS NULL)");
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.try_get::<i64, _>(0)?;

        Ok(count > 0)
    }

    pub async fn exists_by_name_exclude_id(
        &self,
        name: &str,
        exclude_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<bool> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) FROM roles WHERE name = ");
        query.push_bind(name);
        query.push(" AND id != ").push_bind(exclude_id);

        if let Some(ws) = workspace_id {
            query
                .push(" AND (workspace_id = ")
                .push_bind(ws)
                .push(" OR workspace_id IS NULL)");
        }

        let row = query.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.try_get::<i64, _>(0)?;

        Ok(count > 0)
    }

    pub async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Role>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query =
            QueryBuilder::new("SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE id IN (");

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let rows = query
            .build_query_as::<RoleRow>()
            .fetch_all(self.database.pool())
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn find_roles_by_user_id(&self, user_id: &str) -> Result<Vec<Role>> {
        let rows = sqlx::query_as::<_, RoleRow>(
            r#"
            SELECT r.id, r.name, r.description, r.is_administrator, r.workspace_id
            FROM roles r
            INNER JOIN user_roles ur ON r.id = ur.role_id
            WHERE ur.user_id = ?
            ORDER BY r.name
            "#,
        )
        .bind(user_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn is_administrator_role(&self, id: &str) -> Result<bool> {
        let role: Option<i32> = sqlx::query_scalar("SELECT is_administrator FROM roles WHERE id = ?")
            .bind(id)
            .fetch_optional(self.database.pool())
            .await?;

        Ok(role.unwrap_or(0) == 1)
    }

    pub async fn find_with_filters(
        &self,
        _enabled: Option<bool>,
        search: Option<&str>,
        workspace_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>> {
        let params = RoleQueryParams {
            page: Some(page),
            page_size: Some(page_size),
            workspace_id: workspace_id.map(|s| s.to_string()),
            name: search.map(|s| s.to_string()),
            ..Default::default()
        };

        self.find_all(&params).await
    }

    pub async fn update_enabled_status(&self, id: &str, _enabled: bool) -> Result<bool> {
        match self.find_by_id(id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    pub async fn get_permissions(&self, role_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>("SELECT permission_id FROM role_permissions WHERE role_id = ?")
            .bind(role_id)
            .fetch_all(self.database.pool())
            .await?;

        Ok(rows)
    }

    pub async fn update_permissions(&self, role_id: &str, permission_ids: &[String]) -> Result<()> {
        let mut tx = self.database.pool().begin().await?;

        sqlx::query("DELETE FROM role_permissions WHERE role_id = ?")
            .bind(role_id)
            .execute(&mut *tx)
            .await?;

        for permission_id in permission_ids {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO role_permissions (id, role_id, permission_id) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(role_id)
                .bind(permission_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

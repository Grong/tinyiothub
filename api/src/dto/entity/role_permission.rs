use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// Role permission entity - 角色权限关联实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RolePermission {
    pub id: String,
    pub role_id: String,
    pub permission_id: String,
    pub target_id: Option<String>, // Optional target resource ID
    pub permission_type: String,   // "read", "write", "delete", "admin"
    pub created_at: String,
}

/// Query parameters for role permission search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RolePermissionQuery {
    pub role_id: Option<String>,
    pub permission_id: Option<String>,
    pub target_id: Option<String>,
    pub permission_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new role permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRolePermissionRequest {
    pub role_id: String,
    pub permission_id: String,
    pub target_id: Option<String>,
    pub permission_type: String,
}

impl RolePermission {
    /// Create a new role permission
    pub fn new(request: CreateRolePermissionRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role_id: request.role_id,
            permission_id: request.permission_id,
            target_id: request.target_id,
            permission_type: request.permission_type,
            created_at: now,
        }
    }

    /// Find role permission by ID
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<RolePermission>, sqlx::Error> {
        let role_permission = sqlx::query_as::<_, RolePermission>(
            "SELECT id, role_id, permission_id, target_id, permission_type, created_at FROM role_permissions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(role_permission)
    }

    /// Create a new role permission in database
    pub async fn create(
        db: &Database,
        request: &CreateRolePermissionRequest,
    ) -> Result<RolePermission, sqlx::Error> {
        let role_permission = Self::new(request.clone());

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO role_permissions (id, role_id, permission_id, target_id, permission_type, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&role_permission.id)
        .bind(&role_permission.role_id)
        .bind(&role_permission.permission_id)
        .bind(&role_permission.target_id)
        .bind(&role_permission.permission_type)
        .bind(&role_permission.created_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(role_permission)
    }

    /// Find all role permissions with filtering
    pub async fn find_all(
        db: &Database,
        query: &RolePermissionQuery,
    ) -> Result<Vec<RolePermission>, sqlx::Error> {
        let mut sql_query = QueryBuilder::new(
            "SELECT id, role_id, permission_id, target_id, permission_type, created_at FROM role_permissions WHERE 1=1"
        );

        if let Some(role_id) = &query.role_id {
            sql_query.push(" AND role_id = ").push_bind(role_id);
        }

        if let Some(permission_id) = &query.permission_id {
            sql_query.push(" AND permission_id = ").push_bind(permission_id);
        }

        if let Some(target_id) = &query.target_id {
            sql_query.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(permission_type) = &query.permission_type {
            sql_query.push(" AND permission_type = ").push_bind(permission_type);
        }

        sql_query.push(" ORDER BY created_at DESC");

        // Add pagination
        if let Some(page_size) = query.page_size {
            let offset = query.page.unwrap_or(1).saturating_sub(1) * page_size;
            sql_query.push(" LIMIT ").push_bind(page_size as i64);
            sql_query.push(" OFFSET ").push_bind(offset as i64);
        }

        let role_permissions =
            sql_query.build_query_as::<RolePermission>().fetch_all(db.pool()).await?;

        Ok(role_permissions)
    }

    /// Find permissions for a specific role
    pub async fn find_by_role_id(
        db: &Database,
        role_id: &str,
    ) -> Result<Vec<RolePermission>, sqlx::Error> {
        let role_permissions = sqlx::query_as::<_, RolePermission>(
            r#"
            SELECT id, role_id, permission_id, target_id, permission_type, created_at 
            FROM role_permissions 
            WHERE role_id = ?
            ORDER BY permission_type, created_at DESC
            "#,
        )
        .bind(role_id)
        .fetch_all(db.pool())
        .await?;

        Ok(role_permissions)
    }

    /// Find roles with a specific permission
    pub async fn find_by_permission_id(
        db: &Database,
        permission_id: &str,
    ) -> Result<Vec<RolePermission>, sqlx::Error> {
        let role_permissions = sqlx::query_as::<_, RolePermission>(
            r#"
            SELECT id, role_id, permission_id, target_id, permission_type, created_at 
            FROM role_permissions 
            WHERE permission_id = ?
            ORDER BY permission_type, created_at DESC
            "#,
        )
        .bind(permission_id)
        .fetch_all(db.pool())
        .await?;

        Ok(role_permissions)
    }

    /// Delete role permission
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM role_permissions WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Remove all permissions from a role
    pub async fn remove_all_role_permissions(
        db: &Database,
        role_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM role_permissions WHERE role_id = ?")
            .bind(role_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Remove all roles from a permission
    pub async fn remove_all_permission_roles(
        db: &Database,
        permission_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM role_permissions WHERE permission_id = ?")
            .bind(permission_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Assign multiple permissions to a role
    pub async fn assign_permissions_to_role(
        db: &Database,
        role_id: &str,
        permission_assignments: &[(String, Option<String>, String)], // (permission_id, target_id, permission_type)
    ) -> Result<Vec<RolePermission>, sqlx::Error> {
        if permission_assignments.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = db.pool().begin().await?;
        let mut created_permissions = Vec::new();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for (permission_id, target_id, permission_type) in permission_assignments {
            let id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO role_permissions (id, role_id, permission_id, target_id, permission_type, created_at)
                VALUES (?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(&id)
            .bind(role_id)
            .bind(permission_id)
            .bind(target_id)
            .bind(permission_type)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            let role_permission = RolePermission {
                id: id.clone(),
                role_id: role_id.to_string(),
                permission_id: permission_id.clone(),
                target_id: target_id.clone(),
                permission_type: permission_type.clone(),
                created_at: now.clone(),
            };

            created_permissions.push(role_permission);
        }

        tx.commit().await?;
        Ok(created_permissions)
    }

    /// Check if role has specific permission
    pub async fn role_has_permission(
        db: &Database,
        role_id: &str,
        permission_id: &str,
        target_id: Option<&str>,
        permission_type: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut query = QueryBuilder::new(
            "SELECT COUNT(*) as count FROM role_permissions WHERE role_id = ? AND permission_id = ? AND permission_type = ?"
        );

        query.push(" AND (target_id IS NULL");
        if let Some(target) = target_id {
            query.push(" OR target_id = ").push_bind(target);
        }
        query.push(")");

        let row = query
            .build()
            .bind(role_id)
            .bind(permission_id)
            .bind(permission_type)
            .fetch_one(db.pool())
            .await?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    /// Get permission statistics by type
    pub async fn get_permission_stats_by_type(
        db: &Database,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT permission_type, COUNT(*) as count
            FROM role_permissions 
            GROUP BY permission_type 
            ORDER BY count DESC
            "#,
        )
        .fetch_all(db.pool())
        .await?;

        let mut stats = Vec::new();
        for row in rows {
            let permission_type: String = row.get("permission_type");
            let count: i64 = row.get("count");
            stats.push((permission_type, count));
        }

        Ok(stats)
    }

    /// Count role permissions
    pub async fn count(db: &Database, query: &RolePermissionQuery) -> Result<i64, sqlx::Error> {
        let mut sql_query =
            QueryBuilder::new("SELECT COUNT(*) as count FROM role_permissions WHERE 1=1");

        if let Some(role_id) = &query.role_id {
            sql_query.push(" AND role_id = ").push_bind(role_id);
        }

        if let Some(permission_id) = &query.permission_id {
            sql_query.push(" AND permission_id = ").push_bind(permission_id);
        }

        if let Some(target_id) = &query.target_id {
            sql_query.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(permission_type) = &query.permission_type {
            sql_query.push(" AND permission_type = ").push_bind(permission_type);
        }

        let row = sql_query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// Check if this is an admin permission
    pub fn is_admin_permission(&self) -> bool {
        self.permission_type == "admin"
    }

    /// Check if this permission applies to a specific target
    pub fn applies_to_target(&self, target_id: &str) -> bool {
        match &self.target_id {
            Some(id) => id == target_id,
            None => true, // Global permission
        }
    }
}

// Backward compatibility
pub type RolePermissionDto = RolePermission;
pub type RolePermissionQueryParams = RolePermissionQuery;

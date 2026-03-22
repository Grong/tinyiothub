use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// 角色实体 - 使用现代化 SQLx 实现
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: i32,
    // created_at column doesn\'t exist in Roles table
    // pub created_at: Option<String>,
}

/// 角色查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RoleQueryParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建角色请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
}

/// 更新角色请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
}

/// 角色统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub struct RoleStats {
    pub total_roles: i64,
    pub admin_roles: i64,
    pub user_roles: i64,
}

impl Role {
    /// 根据 ID 查找角色
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Role>, sqlx::Error> {
        let role = sqlx::query_as::<_, Role>(
            "SELECT id, name, description, is_administrator FROM roles WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(role)
    }

    /// 根据名称查找角色
    pub async fn find_by_name(db: &Database, name: &str) -> Result<Option<Role>, sqlx::Error> {
        let role = sqlx::query_as::<_, Role>(
            "SELECT id, name, description, is_administrator FROM roles WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        Ok(role)
    }

    /// 创建新角色
    pub async fn create(db: &Database, request: &CreateRoleRequest) -> Result<Role, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let is_admin = request.is_administrator.unwrap_or(0);

        sqlx::query(
            r#"
            INSERT INTO roles (id, name, description, IsAdministrator)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(is_admin)
        .execute(db.pool())
        .await?;

        // 返回创建的角色
        Role::find_by_id(db, &id).await?.ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// 更新角色信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateRoleRequest,
    ) -> Result<Role, sqlx::Error> {
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

        if !has_updates {
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除角色
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM roles WHERE id = ?").bind(id).execute(db.pool()).await?;

        Ok(result.rows_affected())
    }

    /// 批量删除角色
    pub async fn delete_by_ids(db: &Database, ids: &[String]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM roles WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(")");

        let result = query.build().execute(db.pool()).await?;
        Ok(result.rows_affected())
    }

    /// 查询角色列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &RoleQueryParams,
    ) -> Result<Vec<Role>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, description, is_administrator FROM roles WHERE 1=1",
        );

        // 动态添加查询条件
        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(description) = &params.description {
            query.push(" AND description LIKE ").push_bind(format!("%{}%", description));
        }

        if let Some(is_administrator) = params.is_administrator {
            query.push(" AND is_administrator = ").push_bind(is_administrator);
        }

        // 添加排序
        query.push(" ORDER BY name");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let roles = query.build_query_as::<Role>().fetch_all(db.pool()).await?;

        Ok(roles)
    }

    /// 统计角色数量
    pub async fn count(db: &Database, params: &RoleQueryParams) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM roles WHERE 1=1");

        if let Some(name) = &params.name {
            query.push(" AND name LIKE ").push_bind(format!("%{}%", name));
        }

        if let Some(description) = &params.description {
            query.push(" AND description LIKE ").push_bind(format!("%{}%", description));
        }

        if let Some(is_administrator) = params.is_administrator {
            query.push(" AND is_administrator = ").push_bind(is_administrator);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 获取角色统计信息
    pub async fn get_stats(db: &Database) -> Result<RoleStats, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_roles,
                COUNT(CASE WHEN is_administrator = 1 THEN 1 END) as admin_roles,
                COUNT(CASE WHEN is_administrator = 0 THEN 1 END) as user_roles
            FROM roles
            "#,
        )
        .fetch_one(db.pool())
        .await?;

        let stats = RoleStats {
            total_roles: row.get("total_roles"),
            admin_roles: row.get("admin_roles"),
            user_roles: row.get("user_roles"),
        };

        Ok(stats)
    }

    /// 获取管理员角色
    pub async fn find_admin_roles(db: &Database) -> Result<Vec<Role>, sqlx::Error> {
        let roles = sqlx::query_as::<_, Role>(
            "SELECT id, name, description, is_administrator FROM roles WHERE is_administrator = 1 ORDER BY name"
        )
        .fetch_all(db.pool())
        .await?;

        Ok(roles)
    }

    /// 获取普通用户角色
    pub async fn find_user_roles(db: &Database) -> Result<Vec<Role>, sqlx::Error> {
        let roles = sqlx::query_as::<_, Role>(
            "SELECT id, name, description, is_administrator FROM roles WHERE is_administrator = 0 ORDER BY name"
        )
        .fetch_all(db.pool())
        .await?;

        Ok(roles)
    }

    /// 检查角色名称是否存在
    pub async fn exists_by_name(db: &Database, name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(db.pool())
            .await?;

        Ok(count > 0)
    }

    /// 检查角色名称是否存在（排除指定 ID）
    pub async fn exists_by_name_exclude_id(
        db: &Database,
        name: &str,
        exclude_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE name = ? AND id != ?")
                .bind(name)
                .bind(exclude_id)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }

    /// 根据 ID 列表查询角色
    pub async fn find_by_ids(db: &Database, ids: &[String]) -> Result<Vec<Role>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query = QueryBuilder::new(
            "SELECT id, name, description, is_administrator FROM roles WHERE id IN (",
        );

        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let roles = query.build_query_as::<Role>().fetch_all(db.pool()).await?;

        Ok(roles)
    }

    /// 检查是否为管理员角色
    pub async fn is_administrator_role(db: &Database, id: &str) -> Result<bool, sqlx::Error> {
        let role: Option<i32> =
            sqlx::query_scalar("SELECT is_administrator FROM roles WHERE id = ?")
                .bind(id)
                .fetch_optional(db.pool())
                .await?;

        Ok(role.unwrap_or(0) == 1)
    }

    /// Find roles with filters - API compatibility method
    pub async fn find_with_filters(
        db: &Database,
        _enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>, sqlx::Error> {
        let mut params = RoleQueryParams::default();
        params.page = Some(page);
        params.page_size = Some(page_size);

        if let Some(search) = search {
            params.name = Some(search.to_string());
        }

        Self::find_all(db, &params).await
    }

    /// Update enabled status - API compatibility method
    /// Note: Roles don't have enabled status in current schema, so this is a no-op
    pub async fn update_enabled_status(
        db: &Database,
        id: &str,
        _enabled: bool,
    ) -> Result<bool, sqlx::Error> {
        // Roles don't have enabled status in current schema
        // Check if role exists
        match Self::find_by_id(db, id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            description: None,
            is_administrator: 0,
        }
    }
}

// 为了向后兼容，保留旧的 DTO 别名
pub type RoleDto = Role;

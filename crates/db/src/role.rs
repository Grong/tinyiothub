//! Role 持久化：角色（P-集中化 E4，自 user crate 迁入）。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};
use tinyiothub_core::error::Result;
use tinyiothub_core::models::role::{CreateRoleRequest, UpdateRoleRequest};

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
// Persistence (free functions + Db facade)
// ──────────────────────────────────────────────

use sqlx::SqlitePool;

use crate::database::Db;

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

// ── Role free functions ─────────────────────────────────

pub(crate) async fn find_role_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Role>> {
    let row = sqlx::query_as::<_, RoleRow>(
        "SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn find_role_by_name(
    pool: &SqlitePool,
    name: &str,
    workspace_id: Option<&str>,
) -> Result<Option<Role>> {
    let mut query =
        QueryBuilder::new("SELECT id, name, description, is_administrator, workspace_id FROM roles WHERE name = ");
    query.push_bind(name);

    if let Some(ws) = workspace_id {
        query
            .push(" AND (workspace_id = ")
            .push_bind(ws)
            .push(" OR workspace_id IS NULL)");
    }

    let row = query.build_query_as::<RoleRow>().fetch_optional(pool).await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn create_role(pool: &SqlitePool, request: &CreateRoleRequest) -> Result<Role> {
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
    .execute(pool)
    .await?;

    find_role_by_id(pool, &id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn update_role(pool: &SqlitePool, id: &str, request: &UpdateRoleRequest) -> Result<Role> {
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
        return find_role_by_id(pool, id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound);
    }

    query.push(" WHERE id = ").push_bind(id);

    let result = query.build().execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(tinyiothub_core::error::Error::NotFound);
    }

    find_role_by_id(pool, id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn delete_role(pool: &SqlitePool, id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn delete_roles_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    let mut query = QueryBuilder::new("DELETE FROM roles WHERE id IN (");
    let mut separated = query.separated(", ");

    for id in ids {
        separated.push_bind(id);
    }

    separated.push_unseparated(")");

    let result = query.build().execute(pool).await?;
    Ok(result.rows_affected())
}

pub(crate) async fn find_roles(pool: &SqlitePool, params: &RoleQueryParams) -> Result<Vec<Role>> {
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

    let rows = query.build_query_as::<RoleRow>().fetch_all(pool).await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn count_roles(pool: &SqlitePool, params: &RoleQueryParams) -> Result<i64> {
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

    let row = query.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");

    Ok(count)
}

pub(crate) async fn get_role_stats(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<RoleStats> {
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

    let row = query.build().fetch_one(pool).await?;

    let stats = RoleStats {
        total_roles: row.get("total_roles"),
        admin_roles: row.get("admin_roles"),
        user_roles: row.get("user_roles"),
    };

    Ok(stats)
}

pub(crate) async fn find_admin_roles(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<Vec<Role>> {
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

    let rows = query.build_query_as::<RoleRow>().fetch_all(pool).await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_user_roles(pool: &SqlitePool, workspace_id: Option<&str>) -> Result<Vec<Role>> {
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

    let rows = query.build_query_as::<RoleRow>().fetch_all(pool).await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn role_exists_by_name(pool: &SqlitePool, name: &str, workspace_id: Option<&str>) -> Result<bool> {
    let mut query = QueryBuilder::new("SELECT COUNT(*) FROM roles WHERE name = ");
    query.push_bind(name);

    if let Some(ws) = workspace_id {
        query
            .push(" AND (workspace_id = ")
            .push_bind(ws)
            .push(" OR workspace_id IS NULL)");
    }

    let row = query.build().fetch_one(pool).await?;
    let count: i64 = row.try_get::<i64, _>(0)?;

    Ok(count > 0)
}

pub(crate) async fn role_exists_by_name_exclude_id(
    pool: &SqlitePool,
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

    let row = query.build().fetch_one(pool).await?;
    let count: i64 = row.try_get::<i64, _>(0)?;

    Ok(count > 0)
}

pub(crate) async fn find_roles_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<Vec<Role>> {
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

    let rows = query.build_query_as::<RoleRow>().fetch_all(pool).await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_roles_by_user_id(pool: &SqlitePool, user_id: &str) -> Result<Vec<Role>> {
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
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn is_administrator_role(pool: &SqlitePool, id: &str) -> Result<bool> {
    let role: Option<i32> = sqlx::query_scalar("SELECT is_administrator FROM roles WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(role.unwrap_or(0) == 1)
}

pub(crate) async fn find_roles_with_filters(
    pool: &SqlitePool,
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

    find_roles(pool, &params).await
}

pub(crate) async fn update_role_enabled_status(pool: &SqlitePool, id: &str, _enabled: bool) -> Result<bool> {
    match find_role_by_id(pool, id).await? {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

pub(crate) async fn get_role_permissions(pool: &SqlitePool, role_id: &str) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>("SELECT permission_id FROM role_permissions WHERE role_id = ?")
        .bind(role_id)
        .fetch_all(pool)
        .await?;

    Ok(rows)
}

pub(crate) async fn update_role_permissions(pool: &SqlitePool, role_id: &str, permission_ids: &[String]) -> Result<()> {
    let mut tx = pool.begin().await?;

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

impl Db {
    /// 按 ID 查询角色。
    pub async fn find_role_by_id(&self, id: &str) -> Result<Option<Role>> {
        find_role_by_id(self.pool(), id).await
    }

    /// 按名称查询角色（含全局角色）。
    pub async fn find_role_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<Option<Role>> {
        find_role_by_name(self.pool(), name, workspace_id).await
    }

    /// 创建角色。
    pub async fn create_role(&self, request: &CreateRoleRequest) -> Result<Role> {
        create_role(self.pool(), request).await
    }

    /// 更新角色。
    pub async fn update_role(&self, id: &str, request: &UpdateRoleRequest) -> Result<Role> {
        update_role(self.pool(), id, request).await
    }

    /// 删除角色。
    pub async fn delete_role(&self, id: &str) -> Result<u64> {
        delete_role(self.pool(), id).await
    }

    /// 批量删除角色。
    pub async fn delete_roles_by_ids(&self, ids: &[String]) -> Result<u64> {
        delete_roles_by_ids(self.pool(), ids).await
    }

    /// 分页查询角色列表。
    pub async fn find_roles(&self, params: &RoleQueryParams) -> Result<Vec<Role>> {
        find_roles(self.pool(), params).await
    }

    /// 统计角色数。
    pub async fn count_roles(&self, params: &RoleQueryParams) -> Result<i64> {
        count_roles(self.pool(), params).await
    }

    /// 角色统计（管理员/普通角色数）。
    pub async fn get_role_stats(&self, workspace_id: Option<&str>) -> Result<RoleStats> {
        get_role_stats(self.pool(), workspace_id).await
    }

    /// 查询管理员角色。
    pub async fn find_admin_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        find_admin_roles(self.pool(), workspace_id).await
    }

    /// 查询普通角色。
    pub async fn find_user_roles(&self, workspace_id: Option<&str>) -> Result<Vec<Role>> {
        find_user_roles(self.pool(), workspace_id).await
    }

    /// 按名称检查角色是否存在。
    pub async fn role_exists_by_name(&self, name: &str, workspace_id: Option<&str>) -> Result<bool> {
        role_exists_by_name(self.pool(), name, workspace_id).await
    }

    /// 按名称检查角色是否存在（排除指定 ID）。
    pub async fn role_exists_by_name_exclude_id(
        &self,
        name: &str,
        exclude_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<bool> {
        role_exists_by_name_exclude_id(self.pool(), name, exclude_id, workspace_id).await
    }

    /// 按 ID 列表批量查询角色。
    pub async fn find_roles_by_ids(&self, ids: &[String]) -> Result<Vec<Role>> {
        find_roles_by_ids(self.pool(), ids).await
    }

    /// 查询用户拥有的角色。
    pub async fn find_roles_by_user_id(&self, user_id: &str) -> Result<Vec<Role>> {
        find_roles_by_user_id(self.pool(), user_id).await
    }

    /// 检查角色是否为管理员角色。
    pub async fn is_administrator_role(&self, id: &str) -> Result<bool> {
        is_administrator_role(self.pool(), id).await
    }

    /// 按过滤条件分页查询角色。
    pub async fn find_roles_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        workspace_id: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Role>> {
        find_roles_with_filters(self.pool(), enabled, search, workspace_id, page, page_size).await
    }

    /// 更新角色启用状态（roles 表无 enabled 列，仅校验存在性）。
    pub async fn update_role_enabled_status(&self, id: &str, enabled: bool) -> Result<bool> {
        update_role_enabled_status(self.pool(), id, enabled).await
    }

    /// 查询角色关联的权限 ID 列表。
    pub async fn get_role_permissions(&self, role_id: &str) -> Result<Vec<String>> {
        get_role_permissions(self.pool(), role_id).await
    }

    /// 覆盖更新角色关联的权限。
    pub async fn update_role_permissions(&self, role_id: &str, permission_ids: &[String]) -> Result<()> {
        update_role_permissions(self.pool(), role_id, permission_ids).await
    }
}

// ──────────────────────────────────────────────
// Legacy RBAC 查询（自 cloud event/security/access_control.rs 迁入，Task 12）
// 注意：legacy schema 按 role_name/permission 列查询（列语义保持不变）。
// ──────────────────────────────────────────────

/// Legacy：用户的角色名列表（user_roles.role_name）。
pub(crate) async fn list_legacy_user_role_names(
    pool: &SqlitePool,
    user_id: &str,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT role_name FROM user_roles WHERE user_id = ? AND is_active = 1")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

/// Legacy：用户级权限列表（user_permissions.permission）。
pub(crate) async fn list_legacy_user_permissions(
    pool: &SqlitePool,
    user_id: &str,
    resource_type: &str,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT permission FROM user_permissions WHERE user_id = ? AND resource_type = ? AND is_active = 1",
    )
    .bind(user_id)
    .bind(resource_type)
    .fetch_all(pool)
    .await
}

/// Legacy：角色级权限列表（role_permissions.permission）。
pub(crate) async fn list_legacy_role_permissions(
    pool: &SqlitePool,
    role_name: &str,
    resource_type: &str,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT permission FROM role_permissions WHERE role_name = ? AND resource_type = ? AND is_active = 1",
    )
    .bind(role_name)
    .bind(resource_type)
    .fetch_all(pool)
    .await
}

/// 用户持有的 is_administrator 角色数（agent_tasks admin 判定；自 cloud 迁入）。
pub(crate) async fn count_user_admin_roles(pool: &SqlitePool, user_id: &str) -> std::result::Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON ur.role_id = r.id          WHERE ur.user_id = ? AND r.is_administrator = 1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

impl Db {
    /// 用户持有的 is_administrator 角色数。
    pub async fn count_user_admin_roles(&self, user_id: &str) -> std::result::Result<i64, sqlx::Error> {
        count_user_admin_roles(self.pool(), user_id).await
    }

    /// Legacy：用户的角色名列表（user_roles.role_name）。
    pub async fn list_legacy_user_role_names(&self, user_id: &str) -> std::result::Result<Vec<String>, sqlx::Error> {
        list_legacy_user_role_names(self.pool(), user_id).await
    }

    /// Legacy：用户级权限列表（user_permissions.permission）。
    pub async fn list_legacy_user_permissions(
        &self,
        user_id: &str,
        resource_type: &str,
    ) -> std::result::Result<Vec<String>, sqlx::Error> {
        list_legacy_user_permissions(self.pool(), user_id, resource_type).await
    }

    /// Legacy：角色级权限列表（role_permissions.permission）。
    pub async fn list_legacy_role_permissions(
        &self,
        role_name: &str,
        resource_type: &str,
    ) -> std::result::Result<Vec<String>, sqlx::Error> {
        list_legacy_role_permissions(self.pool(), role_name, resource_type).await
    }
}

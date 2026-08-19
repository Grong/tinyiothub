//! Permission 持久化：权限与权限组（P-集中化 E4，自 user crate 迁入）。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};
use tinyiothub_core::error::Result;
use tinyiothub_core::models::permission::{
    CreatePermissionGroupRequest, CreatePermissionRequest, UpdatePermissionRequest,
};

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 仓储契约）— 自领域 crate 迁入
// ──────────────────────────────────────────────

/// Permission entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Permission {
    pub id: String,
    pub name: String,
    pub code: String,
    pub description: Option<String>,
    pub resource_type: String,
    pub action_type: String,
    pub is_system: bool,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Permission group entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PermissionGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub permissions: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Query parameters for permission search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct PermissionQuery {
    pub name: Option<String>,
    pub code: Option<String>,
    pub resource_type: Option<String>,
    pub action_type: Option<String>,
    pub is_system: Option<bool>,
    pub parent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Permission {
    /// Create a new permission
    pub fn new(request: CreatePermissionRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            code: request.code,
            description: request.description,
            resource_type: request.resource_type,
            action_type: request.action_type,
            is_system: request.is_system.unwrap_or(false),
            parent_id: request.parent_id,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Check if this is a system permission
    pub fn is_system_permission(&self) -> bool {
        self.is_system
    }

    /// Check if this is a root permission (no parent)
    pub fn is_root_permission(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Get permission full code
    pub fn get_full_code(&self) -> String {
        format!("{}:{}", self.resource_type, self.action_type)
    }

    /// Check if permission allows action on resource
    pub fn allows_action(&self, resource_type: &str, action_type: &str) -> bool {
        (self.resource_type == resource_type || self.resource_type == "*")
            && (self.action_type == action_type || self.action_type == "*" || self.action_type == "admin")
    }

    /// Get permission priority
    pub fn get_priority(&self) -> u8 {
        match self.action_type.as_str() {
            "admin" => 10,
            "write" => 8,
            "delete" => 7,
            "execute" => 6,
            "read" => 5,
            _ => 1,
        }
    }
}

impl PermissionGroup {
    /// Create a new permission group
    pub fn new(request: CreatePermissionGroupRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let permissions_json = serde_json::to_string(&request.permission_ids).unwrap_or_else(|_| "[]".to_string());

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            description: request.description,
            permissions: permissions_json,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Get permission IDs as vector
    pub fn get_permission_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.permissions).unwrap_or_else(|_| Vec::new())
    }

    /// Add permission to group
    pub fn add_permission(&mut self, permission_id: String) {
        let mut permission_ids = self.get_permission_ids();
        if !permission_ids.contains(&permission_id) {
            permission_ids.push(permission_id);
            self.permissions = serde_json::to_string(&permission_ids).unwrap_or_else(|_| "[]".to_string());
            self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Remove permission from group
    pub fn remove_permission(&mut self, permission_id: &str) {
        let mut permission_ids = self.get_permission_ids();
        if let Some(pos) = permission_ids.iter().position(|x| x == permission_id) {
            permission_ids.remove(pos);
            self.permissions = serde_json::to_string(&permission_ids).unwrap_or_else(|_| "[]".to_string());
            self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Check if group contains permission
    pub fn contains_permission(&self, permission_id: &str) -> bool {
        self.get_permission_ids().contains(&permission_id.to_string())
    }
}

/// Backward compatibility aliases
pub type PermissionDto = Permission;
pub type PermissionQueryParams = PermissionQuery;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_create_request() -> CreatePermissionRequest {
        CreatePermissionRequest {
            name: "Read Devices".to_string(),
            code: "device:read".to_string(),
            description: Some("Can read device data".to_string()),
            resource_type: "device".to_string(),
            action_type: "read".to_string(),
            is_system: Some(true),
            parent_id: Some("parent-1".to_string()),
        }
    }

    #[test]
    fn test_permission_new() {
        let perm = Permission::new(test_create_request());
        assert_eq!(perm.name, "Read Devices");
        assert_eq!(perm.code, "device:read");
        assert_eq!(perm.resource_type, "device");
        assert_eq!(perm.action_type, "read");
        assert!(perm.is_system);
        assert_eq!(perm.parent_id, Some("parent-1".to_string()));
    }

    #[test]
    fn test_permission_defaults() {
        let req = CreatePermissionRequest {
            is_system: None,
            ..test_create_request()
        };
        let perm = Permission::new(req);
        assert!(!perm.is_system);
    }

    #[test]
    fn test_is_system_permission() {
        let mut perm = Permission::new(test_create_request());
        perm.is_system = true;
        assert!(perm.is_system_permission());
        perm.is_system = false;
        assert!(!perm.is_system_permission());
    }

    #[test]
    fn test_is_root_permission() {
        let mut perm = Permission::new(test_create_request());
        assert!(!perm.is_root_permission());
        perm.parent_id = None;
        assert!(perm.is_root_permission());
    }

    #[test]
    fn test_get_full_code() {
        let perm = Permission::new(test_create_request());
        assert_eq!(perm.get_full_code(), "device:read");
    }

    #[test]
    fn test_allows_action() {
        let perm = Permission::new(test_create_request());
        assert!(perm.allows_action("device", "read"));
        assert!(!perm.allows_action("alarm", "read"));
        assert!(!perm.allows_action("device", "write"));
    }

    #[test]
    fn test_allows_action_wildcard_resource() {
        let req = CreatePermissionRequest {
            resource_type: "*".to_string(),
            ..test_create_request()
        };
        let perm = Permission::new(req);
        assert!(perm.allows_action("device", "read"));
        assert!(perm.allows_action("alarm", "read"));
    }

    #[test]
    fn test_allows_action_wildcard_action() {
        let req = CreatePermissionRequest {
            action_type: "*".to_string(),
            ..test_create_request()
        };
        let perm = Permission::new(req);
        assert!(perm.allows_action("device", "read"));
        assert!(perm.allows_action("device", "write"));
    }

    #[test]
    fn test_allows_action_admin() {
        let req = CreatePermissionRequest {
            action_type: "admin".to_string(),
            ..test_create_request()
        };
        let perm = Permission::new(req);
        assert!(perm.allows_action("device", "delete"));
    }

    #[test]
    fn test_get_priority() {
        let mut perm = Permission::new(test_create_request());

        perm.action_type = "admin".to_string();
        assert_eq!(perm.get_priority(), 10);

        perm.action_type = "write".to_string();
        assert_eq!(perm.get_priority(), 8);

        perm.action_type = "delete".to_string();
        assert_eq!(perm.get_priority(), 7);

        perm.action_type = "execute".to_string();
        assert_eq!(perm.get_priority(), 6);

        perm.action_type = "read".to_string();
        assert_eq!(perm.get_priority(), 5);

        perm.action_type = "other".to_string();
        assert_eq!(perm.get_priority(), 1);
    }

    #[test]
    fn test_permission_group_new() {
        let req = CreatePermissionGroupRequest {
            name: "Admins".to_string(),
            description: Some("Admin group".to_string()),
            permission_ids: vec!["perm-1".to_string(), "perm-2".to_string()],
        };
        let group = PermissionGroup::new(req);
        assert_eq!(group.name, "Admins");
        assert_eq!(group.get_permission_ids(), vec!["perm-1", "perm-2"]);
    }

    #[test]
    fn test_permission_group_add_remove() {
        let req = CreatePermissionGroupRequest {
            name: "Test".to_string(),
            description: None,
            permission_ids: vec!["perm-1".to_string()],
        };
        let mut group = PermissionGroup::new(req);

        assert!(group.contains_permission("perm-1"));
        assert!(!group.contains_permission("perm-2"));

        group.add_permission("perm-2".to_string());
        assert!(group.contains_permission("perm-2"));

        group.remove_permission("perm-1");
        assert!(!group.contains_permission("perm-1"));
        assert_eq!(group.get_permission_ids(), vec!["perm-2"]);
    }

    #[test]
    fn test_permission_group_add_duplicate() {
        let req = CreatePermissionGroupRequest {
            name: "Test".to_string(),
            description: None,
            permission_ids: vec!["perm-1".to_string()],
        };
        let mut group = PermissionGroup::new(req);
        group.add_permission("perm-1".to_string());
        assert_eq!(group.get_permission_ids().len(), 1);
    }

    #[test]
    fn test_permission_group_get_permission_ids_invalid_json() {
        let mut group = PermissionGroup::new(CreatePermissionGroupRequest {
            name: "Test".to_string(),
            description: None,
            permission_ids: vec![],
        });
        group.permissions = "not json".to_string();
        assert!(group.get_permission_ids().is_empty());
    }
}

// ──────────────────────────────────────────────
// Persistence (free functions + Db facade)
// ──────────────────────────────────────────────

use sqlx::SqlitePool;

use crate::database::Db;

// ── Row types (internal) ────────────────────────────────

#[derive(Debug, Clone, FromRow)]
struct PermissionRow {
    id: String,
    name: String,
    code: String,
    description: Option<String>,
    resource_type: String,
    action_type: String,
    is_system: bool,
    parent_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<PermissionRow> for Permission {
    fn from(row: PermissionRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            code: row.code,
            description: row.description,
            resource_type: row.resource_type,
            action_type: row.action_type,
            is_system: row.is_system,
            parent_id: row.parent_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct PermissionGroupRow {
    id: String,
    name: String,
    description: Option<String>,
    permissions: String,
    created_at: String,
    updated_at: String,
}

impl From<PermissionGroupRow> for PermissionGroup {
    fn from(row: PermissionGroupRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            permissions: row.permissions,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// ── Permission free functions ───────────────────────────

pub(crate) async fn find_permission_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Permission>> {
    let row = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn find_permission_by_code(pool: &SqlitePool, code: &str) -> Result<Option<Permission>> {
    let row = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE code = ?"
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn create_permission(pool: &SqlitePool, request: &CreatePermissionRequest) -> Result<Permission> {
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
    .execute(pool)
    .await?;

    find_permission_by_id(pool, &id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn update_permission(
    pool: &SqlitePool,
    id: &str,
    request: &UpdatePermissionRequest,
) -> Result<Permission> {
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
        return find_permission_by_id(pool, id)
            .await?
            .ok_or(tinyiothub_core::error::Error::NotFound);
    }

    query.push(" WHERE id = ").push_bind(id);

    let result = query.build().execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(tinyiothub_core::error::Error::NotFound);
    }

    find_permission_by_id(pool, id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn delete_permission(pool: &SqlitePool, id: &str) -> Result<u64> {
    if let Some(permission) = find_permission_by_id(pool, id).await?
        && permission.is_system
    {
        return Err(tinyiothub_core::error::Error::NotFound);
    }

    let result = sqlx::query("DELETE FROM permissions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn delete_permissions_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    let mut query = QueryBuilder::new("DELETE FROM permissions WHERE id IN (");
    let mut separated = query.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(") AND is_system = 0");

    let result = query.build().execute(pool).await?;
    Ok(result.rows_affected())
}

pub(crate) async fn find_permissions(pool: &SqlitePool, params: &PermissionQuery) -> Result<Vec<Permission>> {
    let mut query = QueryBuilder::new(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE 1=1",
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

    let rows = query.build_query_as::<PermissionRow>().fetch_all(pool).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn count_permissions(pool: &SqlitePool, params: &PermissionQuery) -> Result<i64> {
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

    let row = query.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn find_permissions_by_resource_type(
    pool: &SqlitePool,
    resource_type: &str,
) -> Result<Vec<Permission>> {
    let rows = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE resource_type = ? ORDER BY action_type, name"
    )
    .bind(resource_type)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_permissions_by_action_type(pool: &SqlitePool, action_type: &str) -> Result<Vec<Permission>> {
    let rows = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE action_type = ? ORDER BY resource_type, name"
    )
    .bind(action_type)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_system_permissions(pool: &SqlitePool) -> Result<Vec<Permission>> {
    let rows = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE is_system = 1 ORDER BY resource_type, action_type"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_root_permissions(pool: &SqlitePool) -> Result<Vec<Permission>> {
    let rows = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE parent_id IS NULL ORDER BY resource_type, action_type"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_permissions_by_parent_id(pool: &SqlitePool, parent_id: &str) -> Result<Vec<Permission>> {
    let rows = sqlx::query_as::<_, PermissionRow>(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE parent_id = ? ORDER BY action_type, name"
    )
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn permission_exists_by_code(pool: &SqlitePool, code: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE code = ?")
        .bind(code)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

pub(crate) async fn permission_exists_by_code_exclude_id(
    pool: &SqlitePool,
    code: &str,
    exclude_id: &str,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE code = ? AND id != ?")
        .bind(code)
        .bind(exclude_id)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

pub(crate) async fn find_permissions_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<Vec<Permission>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let mut query = QueryBuilder::new(
        "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE id IN (",
    );
    let mut separated = query.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let rows = query.build_query_as::<PermissionRow>().fetch_all(pool).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

// ── PermissionGroup free functions ──────────────────────

pub(crate) async fn find_permission_group_by_id(pool: &SqlitePool, id: &str) -> Result<Option<PermissionGroup>> {
    let row = sqlx::query_as::<_, PermissionGroupRow>(
        "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub(crate) async fn find_permission_group_by_name(pool: &SqlitePool, name: &str) -> Result<Option<PermissionGroup>> {
    let row = sqlx::query_as::<_, PermissionGroupRow>(
        "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub(crate) async fn create_permission_group(
    pool: &SqlitePool,
    request: &CreatePermissionGroupRequest,
) -> Result<PermissionGroup> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let permissions_json = serde_json::to_string(&request.permission_ids).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        r#"
        INSERT INTO permission_groups (id, name, description, permissions, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&request.name)
    .bind(&request.description)
    .bind(&permissions_json)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_permission_group_by_id(pool, &id)
        .await?
        .ok_or(tinyiothub_core::error::Error::NotFound)
}

pub(crate) async fn delete_permission_group(pool: &SqlitePool, id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM permission_groups WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub(crate) async fn find_all_permission_groups(pool: &SqlitePool) -> Result<Vec<PermissionGroup>> {
    let rows = sqlx::query_as::<_, PermissionGroupRow>(
        "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

impl Db {
    /// 按 ID 查询权限。
    pub async fn find_permission_by_id(&self, id: &str) -> Result<Option<Permission>> {
        find_permission_by_id(self.pool(), id).await
    }

    /// 按 code 查询权限。
    pub async fn find_permission_by_code(&self, code: &str) -> Result<Option<Permission>> {
        find_permission_by_code(self.pool(), code).await
    }

    /// 创建权限。
    pub async fn create_permission(&self, request: &CreatePermissionRequest) -> Result<Permission> {
        create_permission(self.pool(), request).await
    }

    /// 更新权限。
    pub async fn update_permission(&self, id: &str, request: &UpdatePermissionRequest) -> Result<Permission> {
        update_permission(self.pool(), id, request).await
    }

    /// 删除权限（系统权限不可删）。
    pub async fn delete_permission(&self, id: &str) -> Result<u64> {
        delete_permission(self.pool(), id).await
    }

    /// 批量删除权限（跳过系统权限）。
    pub async fn delete_permissions_by_ids(&self, ids: &[String]) -> Result<u64> {
        delete_permissions_by_ids(self.pool(), ids).await
    }

    /// 分页查询权限列表。
    pub async fn find_permissions(&self, params: &PermissionQuery) -> Result<Vec<Permission>> {
        find_permissions(self.pool(), params).await
    }

    /// 统计权限数。
    pub async fn count_permissions(&self, params: &PermissionQuery) -> Result<i64> {
        count_permissions(self.pool(), params).await
    }

    /// 按资源类型查询权限。
    pub async fn find_permissions_by_resource_type(&self, resource_type: &str) -> Result<Vec<Permission>> {
        find_permissions_by_resource_type(self.pool(), resource_type).await
    }

    /// 按动作类型查询权限。
    pub async fn find_permissions_by_action_type(&self, action_type: &str) -> Result<Vec<Permission>> {
        find_permissions_by_action_type(self.pool(), action_type).await
    }

    /// 查询系统权限。
    pub async fn find_system_permissions(&self) -> Result<Vec<Permission>> {
        find_system_permissions(self.pool()).await
    }

    /// 查询根权限（无父级）。
    pub async fn find_root_permissions(&self) -> Result<Vec<Permission>> {
        find_root_permissions(self.pool()).await
    }

    /// 按父级 ID 查询权限。
    pub async fn find_permissions_by_parent_id(&self, parent_id: &str) -> Result<Vec<Permission>> {
        find_permissions_by_parent_id(self.pool(), parent_id).await
    }

    /// 按 code 检查权限是否存在。
    pub async fn permission_exists_by_code(&self, code: &str) -> Result<bool> {
        permission_exists_by_code(self.pool(), code).await
    }

    /// 按 code 检查权限是否存在（排除指定 ID）。
    pub async fn permission_exists_by_code_exclude_id(&self, code: &str, exclude_id: &str) -> Result<bool> {
        permission_exists_by_code_exclude_id(self.pool(), code, exclude_id).await
    }

    /// 按 ID 列表批量查询权限。
    pub async fn find_permissions_by_ids(&self, ids: &[String]) -> Result<Vec<Permission>> {
        find_permissions_by_ids(self.pool(), ids).await
    }

    /// 按 ID 查询权限组。
    pub async fn find_permission_group_by_id(&self, id: &str) -> Result<Option<PermissionGroup>> {
        find_permission_group_by_id(self.pool(), id).await
    }

    /// 按名称查询权限组。
    pub async fn find_permission_group_by_name(&self, name: &str) -> Result<Option<PermissionGroup>> {
        find_permission_group_by_name(self.pool(), name).await
    }

    /// 创建权限组。
    pub async fn create_permission_group(&self, request: &CreatePermissionGroupRequest) -> Result<PermissionGroup> {
        create_permission_group(self.pool(), request).await
    }

    /// 删除权限组。
    pub async fn delete_permission_group(&self, id: &str) -> Result<u64> {
        delete_permission_group(self.pool(), id).await
    }

    /// 查询所有权限组。
    pub async fn find_all_permission_groups(&self) -> Result<Vec<PermissionGroup>> {
        find_all_permission_groups(self.pool()).await
    }
}

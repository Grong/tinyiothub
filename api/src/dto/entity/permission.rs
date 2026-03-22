use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// Permission entity - 权限实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Permission {
    pub id: String,
    pub name: String,
    pub code: String, // Unique permission code
    pub description: Option<String>,
    pub resource_type: String, // "device", "user", "role", "system", etc.
    pub action_type: String,   // "read", "write", "delete", "admin", "execute"
    pub is_system: bool,       // System permissions cannot be deleted
    pub parent_id: Option<String>, // For hierarchical permissions
    pub created_at: String,
    pub updated_at: String,
}

/// Permission group entity - 权限组实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PermissionGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub permissions: String, // Permission IDs as JSON string
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

/// Request for creating a new permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePermissionRequest {
    pub name: String,
    pub code: String,
    pub description: Option<String>,
    pub resource_type: String,
    pub action_type: String,
    pub is_system: Option<bool>,
    pub parent_id: Option<String>,
}

/// Request for updating a permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePermissionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub resource_type: Option<String>,
    pub action_type: Option<String>,
    pub parent_id: Option<String>,
}

/// Request for creating a permission group
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePermissionGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub permission_ids: Vec<String>,
}

impl Permission {
    /// 根据 ID 查找权限
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Permission>, sqlx::Error> {
        let permission = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(permission)
    }

    /// 根据权限代码查找权限
    pub async fn find_by_code(
        db: &Database,
        code: &str,
    ) -> Result<Option<Permission>, sqlx::Error> {
        let permission = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE code = ?"
        )
        .bind(code)
        .fetch_optional(db.pool())
        .await?;

        Ok(permission)
    }

    /// 创建新权限
    pub async fn create(
        db: &Database,
        request: &CreatePermissionRequest,
    ) -> Result<Permission, sqlx::Error> {
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
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新权限信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdatePermissionRequest,
    ) -> Result<Permission, sqlx::Error> {
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
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除权限
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        // 检查是否为系统权限
        if let Some(permission) = Self::find_by_id(db, id).await? {
            if permission.is_system {
                return Err(sqlx::Error::RowNotFound); // 系统权限不能删除
            }
        }

        let result =
            sqlx::query("DELETE FROM permissions WHERE id = ?").bind(id).execute(db.pool()).await?;

        Ok(result.rows_affected())
    }

    /// 批量删除权限
    pub async fn delete_by_ids(db: &Database, ids: &[String]) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = QueryBuilder::new("DELETE FROM permissions WHERE id IN (");
        let mut separated = query.separated(", ");

        for id in ids {
            separated.push_bind(id);
        }

        separated.push_unseparated(") AND is_system = 0"); // 只删除非系统权限

        let result = query.build().execute(db.pool()).await?;
        Ok(result.rows_affected())
    }

    /// 查询权限列表（支持分页和筛选）
    pub async fn find_all(
        db: &Database,
        params: &PermissionQuery,
    ) -> Result<Vec<Permission>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE 1=1"
        );

        // 动态添加查询条件
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

        // 添加排序
        query.push(" ORDER BY resource_type, action_type, name");

        // 添加分页
        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let permissions = query.build_query_as::<Permission>().fetch_all(db.pool()).await?;

        Ok(permissions)
    }

    /// 统计权限数量
    pub async fn count(db: &Database, params: &PermissionQuery) -> Result<i64, sqlx::Error> {
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

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 根据资源类型查询权限
    pub async fn find_by_resource_type(
        db: &Database,
        resource_type: &str,
    ) -> Result<Vec<Permission>, sqlx::Error> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE resource_type = ? ORDER BY action_type, name"
        )
        .bind(resource_type)
        .fetch_all(db.pool())
        .await?;

        Ok(permissions)
    }

    /// 根据操作类型查询权限
    pub async fn find_by_action_type(
        db: &Database,
        action_type: &str,
    ) -> Result<Vec<Permission>, sqlx::Error> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE action_type = ? ORDER BY resource_type, name"
        )
        .bind(action_type)
        .fetch_all(db.pool())
        .await?;

        Ok(permissions)
    }

    /// 获取系统权限
    pub async fn find_system_permissions(db: &Database) -> Result<Vec<Permission>, sqlx::Error> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE is_system = 1 ORDER BY resource_type, action_type"
        )
        .fetch_all(db.pool())
        .await?;

        Ok(permissions)
    }

    /// 获取根权限（无父权限）
    pub async fn find_root_permissions(db: &Database) -> Result<Vec<Permission>, sqlx::Error> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE parent_id IS NULL ORDER BY resource_type, action_type"
        )
        .fetch_all(db.pool())
        .await?;

        Ok(permissions)
    }

    /// 根据父权限 ID 查询子权限
    pub async fn find_by_parent_id(
        db: &Database,
        parent_id: &str,
    ) -> Result<Vec<Permission>, sqlx::Error> {
        let permissions = sqlx::query_as::<_, Permission>(
            "SELECT id, name, code, description, resource_type, action_type, is_system, parent_id, created_at, updated_at FROM permissions WHERE parent_id = ? ORDER BY action_type, name"
        )
        .bind(parent_id)
        .fetch_all(db.pool())
        .await?;

        Ok(permissions)
    }

    /// 检查权限代码是否存在
    pub async fn exists_by_code(db: &Database, code: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE code = ?")
            .bind(code)
            .fetch_one(db.pool())
            .await?;

        Ok(count > 0)
    }

    /// 检查权限代码是否存在（排除指定 ID）
    pub async fn exists_by_code_exclude_id(
        db: &Database,
        code: &str,
        exclude_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM permissions WHERE code = ? AND id != ?")
                .bind(code)
                .bind(exclude_id)
                .fetch_one(db.pool())
                .await?;

        Ok(count > 0)
    }

    /// 根据 ID 列表查询权限
    pub async fn find_by_ids(
        db: &Database,
        ids: &[String],
    ) -> Result<Vec<Permission>, sqlx::Error> {
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

        let permissions = query.build_query_as::<Permission>().fetch_all(db.pool()).await?;

        Ok(permissions)
    }

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

    /// Get permission full code (including parent hierarchy)
    pub fn get_full_code(&self) -> String {
        format!("{}:{}", self.resource_type, self.action_type)
    }

    /// Check if permission allows action on resource
    pub fn allows_action(&self, resource_type: &str, action_type: &str) -> bool {
        (self.resource_type == resource_type || self.resource_type == "*")
            && (self.action_type == action_type
                || self.action_type == "*"
                || self.action_type == "admin")
    }

    /// Get permission priority (higher number = higher priority)
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
    /// 根据 ID 查找权限组
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<PermissionGroup>, sqlx::Error> {
        let group = sqlx::query_as::<_, PermissionGroup>(
            "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(group)
    }

    /// 根据名称查找权限组
    pub async fn find_by_name(
        db: &Database,
        name: &str,
    ) -> Result<Option<PermissionGroup>, sqlx::Error> {
        let group = sqlx::query_as::<_, PermissionGroup>(
            "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(db.pool())
        .await?;

        Ok(group)
    }

    /// 创建新权限组
    pub async fn create(
        db: &Database,
        request: &CreatePermissionGroupRequest,
    ) -> Result<PermissionGroup, sqlx::Error> {
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
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除权限组
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM permission_groups WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// 查询所有权限组
    pub async fn find_all(db: &Database) -> Result<Vec<PermissionGroup>, sqlx::Error> {
        let groups = sqlx::query_as::<_, PermissionGroup>(
            "SELECT id, name, description, permissions, created_at, updated_at FROM permission_groups ORDER BY name"
        )
        .fetch_all(db.pool())
        .await?;

        Ok(groups)
    }

    /// Create a new permission group
    pub fn new(request: CreatePermissionGroupRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let permissions_json =
            serde_json::to_string(&request.permission_ids).unwrap_or_else(|_| "[]".to_string());

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
            self.permissions =
                serde_json::to_string(&permission_ids).unwrap_or_else(|_| "[]".to_string());
            self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Remove permission from group
    pub fn remove_permission(&mut self, permission_id: &str) {
        let mut permission_ids = self.get_permission_ids();
        if let Some(pos) = permission_ids.iter().position(|x| x == permission_id) {
            permission_ids.remove(pos);
            self.permissions =
                serde_json::to_string(&permission_ids).unwrap_or_else(|_| "[]".to_string());
            self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Check if group contains permission
    pub fn contains_permission(&self, permission_id: &str) -> bool {
        self.get_permission_ids().contains(&permission_id.to_string())
    }
}

// Backward compatibility
pub type PermissionDto = Permission;
pub type PermissionQueryParams = PermissionQuery;

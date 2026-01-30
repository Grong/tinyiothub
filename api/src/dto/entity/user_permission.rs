use crate::infrastructure::persistence::database::Database;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

/// User permission entity - 用户权限实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserPermission {
    pub id: String,
    pub user_id: String,
    pub permission_id: String,
    pub target_id: Option<String>, // Optional target resource ID
    pub permission_type: String,   // "read", "write", "delete", "admin"
    pub action_entity_set: Option<String>, // Specific actions allowed
    pub created_at: String,
    pub expires_at: Option<String>, // Optional expiration
}

/// Query parameters for user permission search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserPermissionQuery {
    pub user_id: Option<String>,
    pub permission_id: Option<String>,
    pub target_id: Option<String>,
    pub permission_type: Option<String>,
    pub action_entity_set: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new user permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateUserPermissionRequest {
    pub user_id: String,
    pub permission_id: String,
    pub target_id: Option<String>,
    pub permission_type: String,
    pub action_entity_set: Option<String>,
    pub expires_at: Option<String>,
}

impl UserPermission {
    /// Create a new user permission
    pub fn new(request: CreateUserPermissionRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: request.user_id,
            permission_id: request.permission_id,
            target_id: request.target_id,
            permission_type: request.permission_type,
            action_entity_set: request.action_entity_set,
            created_at: now,
            expires_at: request.expires_at,
        }
    }

    /// Find user permission by ID
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<UserPermission>, sqlx::Error> {
        let user_permission = sqlx::query_as::<_, UserPermission>(
            r#"
            SELECT id, user_id, permission_id, target_id, permission_type, 
                   action_entity_set, created_at, expires_at 
            FROM user_permissions WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(user_permission)
    }

    /// Create a new user permission in database
    pub async fn create(
        db: &Database,
        request: &CreateUserPermissionRequest,
    ) -> Result<UserPermission, sqlx::Error> {
        let user_permission = Self::new(request.clone());

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO user_permissions (id, user_id, permission_id, target_id, permission_type, 
                                       action_entity_set, created_at, expires_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&user_permission.id)
        .bind(&user_permission.user_id)
        .bind(&user_permission.permission_id)
        .bind(&user_permission.target_id)
        .bind(&user_permission.permission_type)
        .bind(&user_permission.action_entity_set)
        .bind(&user_permission.created_at)
        .bind(&user_permission.expires_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(user_permission)
    }

    /// Find all user permissions with filtering
    pub async fn find_all(
        db: &Database,
        query: &UserPermissionQuery,
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        let mut sql_query = QueryBuilder::new(
            r#"
            SELECT id, user_id, permission_id, target_id, permission_type, 
                   action_entity_set, created_at, expires_at 
            FROM user_permissions WHERE 1=1
            "#,
        );

        if let Some(user_id) = &query.user_id {
            sql_query.push(" AND user_id = ").push_bind(user_id);
        }

        if let Some(permission_id) = &query.permission_id {
            sql_query
                .push(" AND permission_id = ")
                .push_bind(permission_id);
        }

        if let Some(target_id) = &query.target_id {
            sql_query.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(permission_type) = &query.permission_type {
            sql_query
                .push(" AND permission_type = ")
                .push_bind(permission_type);
        }

        if let Some(action_entity_set) = &query.action_entity_set {
            sql_query
                .push(" AND action_entity_set = ")
                .push_bind(action_entity_set);
        }

        // Filter out expired permissions by default
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        sql_query
            .push(" AND (expires_at IS NULL OR expires_at > ")
            .push_bind(now)
            .push(")");

        sql_query.push(" ORDER BY created_at DESC");

        // Add pagination
        if let Some(page_size) = query.page_size {
            let offset = query.page.unwrap_or(1).saturating_sub(1) * page_size;
            sql_query.push(" LIMIT ").push_bind(page_size as i64);
            sql_query.push(" OFFSET ").push_bind(offset as i64);
        }

        let user_permissions = sql_query
            .build_query_as::<UserPermission>()
            .fetch_all(db.pool())
            .await?;

        Ok(user_permissions)
    }

    /// Find permissions for a specific user
    pub async fn find_by_user_id(
        db: &Database,
        user_id: &str,
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let user_permissions = sqlx::query_as::<_, UserPermission>(
            r#"
            SELECT id, user_id, permission_id, target_id, permission_type, 
                   action_entity_set, created_at, expires_at 
            FROM user_permissions 
            WHERE user_id = ? AND (expires_at IS NULL OR expires_at > ?)
            ORDER BY permission_type, created_at DESC
            "#,
        )
        .bind(user_id)
        .bind(now)
        .fetch_all(db.pool())
        .await?;

        Ok(user_permissions)
    }

    /// Find users with a specific permission
    pub async fn find_by_permission_id(
        db: &Database,
        permission_id: &str,
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let user_permissions = sqlx::query_as::<_, UserPermission>(
            r#"
            SELECT id, user_id, permission_id, target_id, permission_type, 
                   action_entity_set, created_at, expires_at 
            FROM user_permissions 
            WHERE permission_id = ? AND (expires_at IS NULL OR expires_at > ?)
            ORDER BY permission_type, created_at DESC
            "#,
        )
        .bind(permission_id)
        .bind(now)
        .fetch_all(db.pool())
        .await?;

        Ok(user_permissions)
    }

    /// Update user permission expiration
    pub async fn update_expiration(
        db: &Database,
        id: &str,
        expires_at: Option<String>,
    ) -> Result<UserPermission, sqlx::Error> {
        let mut tx = db.pool().begin().await?;

        let result = sqlx::query("UPDATE user_permissions SET expires_at = ? WHERE id = ?")
            .bind(&expires_at)
            .bind(id)
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        tx.commit().await?;

        Self::find_by_id(db, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Delete user permission
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM user_permissions WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Remove all permissions from a user
    pub async fn remove_all_user_permissions(
        db: &Database,
        user_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM user_permissions WHERE user_id = ?")
            .bind(user_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Remove all users from a permission
    pub async fn remove_all_permission_users(
        db: &Database,
        permission_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM user_permissions WHERE permission_id = ?")
            .bind(permission_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Assign multiple permissions to a user
    pub async fn assign_permissions_to_user(
        db: &Database,
        user_id: &str,
        permission_assignments: &[(
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        )], // (permission_id, target_id, permission_type, action_entity_set, expires_at)
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        if permission_assignments.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = db.pool().begin().await?;
        let mut created_permissions = Vec::new();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for (permission_id, target_id, permission_type, action_entity_set, expires_at) in
            permission_assignments
        {
            let id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO user_permissions (id, user_id, permission_id, target_id, permission_type, 
                                           action_entity_set, created_at, expires_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(&id)
            .bind(user_id)
            .bind(permission_id)
            .bind(target_id)
            .bind(permission_type)
            .bind(action_entity_set)
            .bind(&now)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;

            let user_permission = UserPermission {
                id: id.clone(),
                user_id: user_id.to_string(),
                permission_id: permission_id.clone(),
                target_id: target_id.clone(),
                permission_type: permission_type.clone(),
                action_entity_set: action_entity_set.clone(),
                created_at: now.clone(),
                expires_at: expires_at.clone(),
            };

            created_permissions.push(user_permission);
        }

        tx.commit().await?;
        Ok(created_permissions)
    }

    /// Check if user has specific permission
    pub async fn user_has_permission(
        db: &Database,
        user_id: &str,
        permission_id: &str,
        target_id: Option<&str>,
        permission_type: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut query_str = String::from(
            r#"
            SELECT COUNT(*) as count FROM user_permissions 
            WHERE user_id = ? AND permission_id = ? AND permission_type = ?
              AND (expires_at IS NULL OR expires_at > ?)
            "#,
        );

        let mut params: Vec<String> = vec![
            user_id.to_string(),
            permission_id.to_string(),
            permission_type.to_string(),
            now,
        ];

        if let Some(target) = target_id {
            query_str.push_str(" AND (target_id IS NULL OR target_id = ?)");
            params.push(target.to_string());
        } else {
            query_str.push_str(" AND target_id IS NULL");
        }

        let mut query = sqlx::query(&query_str);
        for param in &params {
            query = query.bind(param);
        }

        let row = query.fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    /// Get expiring user permissions (within specified days)
    pub async fn get_expiring_permissions(
        db: &Database,
        days: i32,
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        let future_date = chrono::Utc::now() + chrono::Duration::days(days as i64);
        let future_date_str = future_date.format("%Y-%m-%d %H:%M:%S").to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let user_permissions = sqlx::query_as::<_, UserPermission>(
            r#"
            SELECT id, user_id, permission_id, target_id, permission_type, 
                   action_entity_set, created_at, expires_at 
            FROM user_permissions 
            WHERE expires_at IS NOT NULL 
              AND expires_at > ? 
              AND expires_at <= ?
            ORDER BY expires_at ASC
            "#,
        )
        .bind(now)
        .bind(future_date_str)
        .fetch_all(db.pool())
        .await?;

        Ok(user_permissions)
    }

    /// Get permission statistics by type
    pub async fn get_permission_stats_by_type(
        db: &Database,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let rows = sqlx::query(
            r#"
            SELECT permission_type, COUNT(*) as count
            FROM user_permissions 
            WHERE expires_at IS NULL OR expires_at > ?
            GROUP BY permission_type 
            ORDER BY count DESC
            "#,
        )
        .bind(now)
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

    /// Count user permissions
    pub async fn count(db: &Database, query: &UserPermissionQuery) -> Result<i64, sqlx::Error> {
        let mut sql_query =
            QueryBuilder::new("SELECT COUNT(*) as count FROM user_permissions WHERE 1=1");

        if let Some(user_id) = &query.user_id {
            sql_query.push(" AND user_id = ").push_bind(user_id);
        }

        if let Some(permission_id) = &query.permission_id {
            sql_query
                .push(" AND permission_id = ")
                .push_bind(permission_id);
        }

        if let Some(target_id) = &query.target_id {
            sql_query.push(" AND target_id = ").push_bind(target_id);
        }

        if let Some(permission_type) = &query.permission_type {
            sql_query
                .push(" AND permission_type = ")
                .push_bind(permission_type);
        }

        if let Some(action_entity_set) = &query.action_entity_set {
            sql_query
                .push(" AND action_entity_set = ")
                .push_bind(action_entity_set);
        }

        // Filter out expired permissions by default
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        sql_query
            .push(" AND (expires_at IS NULL OR expires_at > ")
            .push_bind(now)
            .push(")");

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

    /// Check if the permission has expired
    pub fn is_expired(&self) -> bool {
        match &self.expires_at {
            Some(expires) => {
                let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                expires < &now
            }
            None => false, // No expiration
        }
    }

    /// Check if the permission is currently valid
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }

    /// Get days until expiration
    pub fn days_until_expiration(&self) -> Option<i64> {
        if let Some(expires) = &self.expires_at {
            let now = chrono::Utc::now();
            if let Ok(exp_time) =
                chrono::NaiveDateTime::parse_from_str(expires, "%Y-%m-%d %H:%M:%S")
            {
                let exp_time_utc =
                    chrono::DateTime::from_naive_utc_and_offset(exp_time, chrono::Utc);
                let duration = exp_time_utc - now;
                return Some(duration.num_days());
            }
        }
        None
    }

    /// Check if permission is expiring soon (within 7 days)
    pub fn is_expiring_soon(&self) -> bool {
        if let Some(days) = self.days_until_expiration() {
            days > 0 && days <= 7
        } else {
            false
        }
    }
}

// Backward compatibility
pub type UserPermissionDto = UserPermission;
pub type UserPermissionQueryParams = UserPermissionQuery;

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// User-Role association entity - 用户角色关联实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserRole {
    pub id: String,
    pub user_id: String,
    pub role_id: String,
    pub assigned_by: Option<String>,
    pub assigned_at: String,
    pub expires_at: Option<String>, // Optional expiration for temporary role assignments
    pub is_active: bool,
}

/// Query parameters for user-role search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserRoleQuery {
    pub user_id: Option<String>,
    pub role_id: Option<String>,
    pub assigned_by: Option<String>,
    pub is_active: Option<bool>,
    pub include_expired: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new user-role association
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateUserRoleRequest {
    pub user_id: String,
    pub role_id: String,
    pub assigned_by: Option<String>,
    pub expires_at: Option<String>,
}

/// User with roles - 用户及其角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserWithRoles {
    pub user_id: String,
    pub user_name: String,
    pub user_email: Option<String>,
    pub roles: Vec<RoleInfo>,
}

/// Role information - 角色信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleInfo {
    pub role_id: String,
    pub role_name: String,
    pub role_description: Option<String>,
    pub is_administrator: bool,
    pub assigned_at: String,
    pub expires_at: Option<String>,
}

impl UserRole {
    /// Create a new user-role association
    pub fn new(request: CreateUserRoleRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: request.user_id,
            role_id: request.role_id,
            assigned_by: request.assigned_by,
            assigned_at: now,
            expires_at: request.expires_at,
            is_active: true,
        }
    }

    /// Find user-role association by ID
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<UserRole>, sqlx::Error> {
        let user_role = sqlx::query_as::<_, UserRole>(
            "SELECT id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active FROM user_roles WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(user_role)
    }

    /// Create a new user-role association in database
    pub async fn create(
        db: &Database,
        request: &CreateUserRoleRequest,
    ) -> Result<UserRole, sqlx::Error> {
        let user_role = Self::new(request.clone());

        let mut tx = db.pool().begin().await?;

        sqlx::query(
            r#"
            INSERT INTO user_roles (id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&user_role.id)
        .bind(&user_role.user_id)
        .bind(&user_role.role_id)
        .bind(&user_role.assigned_by)
        .bind(&user_role.assigned_at)
        .bind(&user_role.expires_at)
        .bind(user_role.is_active)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(user_role)
    }

    /// Find all user-role associations with filtering
    pub async fn find_all(
        db: &Database,
        query: &UserRoleQuery,
    ) -> Result<Vec<UserRole>, sqlx::Error> {
        let mut sql_query = QueryBuilder::new(
            "SELECT id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active FROM user_roles WHERE 1=1"
        );

        if let Some(user_id) = &query.user_id {
            sql_query.push(" AND user_id = ").push_bind(user_id);
        }

        if let Some(role_id) = &query.role_id {
            sql_query.push(" AND role_id = ").push_bind(role_id);
        }

        if let Some(assigned_by) = &query.assigned_by {
            sql_query.push(" AND assigned_by = ").push_bind(assigned_by);
        }

        if let Some(is_active) = query.is_active {
            sql_query.push(" AND is_active = ").push_bind(is_active);
        }

        // Filter expired roles if not explicitly included
        if !query.include_expired.unwrap_or(false) {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            sql_query.push(" AND (expires_at IS NULL OR expires_at > ").push_bind(now).push(")");
        }

        sql_query.push(" ORDER BY assigned_at DESC");

        // Add pagination
        if let Some(page_size) = query.page_size {
            let offset = query.page.unwrap_or(1).saturating_sub(1) * page_size;
            sql_query.push(" LIMIT ").push_bind(page_size as i64);
            sql_query.push(" OFFSET ").push_bind(offset as i64);
        }

        let user_roles = sql_query.build_query_as::<UserRole>().fetch_all(db.pool()).await?;

        Ok(user_roles)
    }

    /// Find roles for a specific user
    pub async fn find_by_user_id(
        db: &Database,
        user_id: &str,
    ) -> Result<Vec<UserRole>, sqlx::Error> {
        let user_roles = sqlx::query_as::<_, UserRole>(
            r#"
            SELECT id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active 
            FROM user_roles 
            WHERE user_id = ? AND is_active = true
            ORDER BY assigned_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(db.pool())
        .await?;

        Ok(user_roles)
    }

    /// Find users with a specific role
    pub async fn find_by_role_id(
        db: &Database,
        role_id: &str,
    ) -> Result<Vec<UserRole>, sqlx::Error> {
        let user_roles = sqlx::query_as::<_, UserRole>(
            r#"
            SELECT id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active 
            FROM user_roles 
            WHERE role_id = ? AND is_active = true
            ORDER BY assigned_at DESC
            "#,
        )
        .bind(role_id)
        .fetch_all(db.pool())
        .await?;

        Ok(user_roles)
    }

    /// Update user-role association
    pub async fn update(
        db: &Database,
        id: &str,
        is_active: Option<bool>,
        expires_at: Option<String>,
    ) -> Result<UserRole, sqlx::Error> {
        let mut tx = db.pool().begin().await?;

        let mut query = QueryBuilder::new("UPDATE user_roles SET ");
        let mut has_updates = false;

        if let Some(active) = is_active {
            query.push("is_active = ").push_bind(active);
            has_updates = true;
        }

        if let Some(expires) = &expires_at {
            if has_updates {
                query.push(", ");
            }
            query.push("expires_at = ").push_bind(expires);
            has_updates = true;
        }

        if !has_updates {
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(&mut *tx).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        tx.commit().await?;

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// Delete user-role association
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM user_roles WHERE id = ?").bind(id).execute(db.pool()).await?;

        Ok(result.rows_affected())
    }

    /// Remove all roles from a user
    pub async fn remove_all_user_roles(db: &Database, user_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
            .bind(user_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Remove all users from a role
    pub async fn remove_all_role_users(db: &Database, role_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM user_roles WHERE role_id = ?")
            .bind(role_id)
            .execute(db.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Assign multiple roles to a user
    pub async fn assign_roles_to_user(
        db: &Database,
        user_id: &str,
        role_ids: &[String],
        assigned_by: Option<String>,
    ) -> Result<Vec<UserRole>, sqlx::Error> {
        if role_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut tx = db.pool().begin().await?;
        let mut created_roles = Vec::new();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        for role_id in role_ids {
            let id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO user_roles (id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(&id)
            .bind(user_id)
            .bind(role_id)
            .bind(&assigned_by)
            .bind(&now)
            .bind::<Option<String>>(None) // No expiration by default
            .bind(true)
            .execute(&mut *tx)
            .await?;

            let user_role = UserRole {
                id: id.clone(),
                user_id: user_id.to_string(),
                role_id: role_id.clone(),
                assigned_by: assigned_by.clone(),
                assigned_at: now.clone(),
                expires_at: None,
                is_active: true,
            };

            created_roles.push(user_role);
        }

        tx.commit().await?;
        Ok(created_roles)
    }

    /// Get user with roles information
    pub async fn get_user_with_roles(
        db: &Database,
        user_id: &str,
    ) -> Result<Option<UserWithRoles>, sqlx::Error> {
        // First get user information
        let user_row = sqlx::query("SELECT id, name, email FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(db.pool())
            .await?;

        let user_row = match user_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let user_name: String = user_row.get("Name");
        let user_email: Option<String> = user_row.get("Email");

        // Get user roles with role information
        let role_rows = sqlx::query(
            r#"
            SELECT ur.role_id, ur.assigned_at, ur.expires_at, r.name as role_name, 
                   r.description as role_description, r.is_administrator as is_administrator
            FROM user_roles ur
            JOIN Roles r ON ur.role_id = r.id
            WHERE ur.user_id = ? AND ur.is_active = true
            ORDER BY ur.assigned_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(db.pool())
        .await?;

        let mut user_with_roles = UserWithRoles::new(user_id.to_string(), user_name, user_email);

        for row in role_rows {
            let role_info = RoleInfo::new(
                row.get("role_id"),
                row.get("role_name"),
                row.get("role_description"),
                row.get::<i32, _>("is_administrator") != 0,
                row.get("assigned_at"),
                row.get("expires_at"),
            );
            user_with_roles.add_role(role_info);
        }

        Ok(Some(user_with_roles))
    }

    /// Get users with specific role
    pub async fn get_users_with_role(
        db: &Database,
        role_id: &str,
    ) -> Result<Vec<UserWithRoles>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT u.id as user_id, u.name as user_name, u.email as user_email,
                   ur.assigned_at, ur.expires_at, r.name as role_name, 
                   r.description as role_description, r.is_administrator as is_administrator
            FROM user_roles ur
            JOIN Users u ON ur.user_id = u.id
            JOIN Roles r ON ur.role_id = r.id
            WHERE ur.role_id = ? AND ur.is_active = true
            ORDER BY u.name
            "#,
        )
        .bind(role_id)
        .fetch_all(db.pool())
        .await?;

        let mut users_with_roles = Vec::new();

        for row in rows {
            let user_id: String = row.get("user_id");
            let user_name: String = row.get("user_name");
            let user_email: Option<String> = row.get("user_email");

            let role_info = RoleInfo::new(
                role_id.to_string(),
                row.get("role_name"),
                row.get("role_description"),
                row.get::<i32, _>("is_administrator") != 0,
                row.get("assigned_at"),
                row.get("expires_at"),
            );

            let mut user_with_roles = UserWithRoles::new(user_id, user_name, user_email);
            user_with_roles.add_role(role_info);
            users_with_roles.push(user_with_roles);
        }

        Ok(users_with_roles)
    }

    /// Count user-role associations
    pub async fn count(db: &Database, query: &UserRoleQuery) -> Result<i64, sqlx::Error> {
        let mut sql_query = QueryBuilder::new("SELECT COUNT(*) as count FROM user_roles WHERE 1=1");

        if let Some(user_id) = &query.user_id {
            sql_query.push(" AND user_id = ").push_bind(user_id);
        }

        if let Some(role_id) = &query.role_id {
            sql_query.push(" AND role_id = ").push_bind(role_id);
        }

        if let Some(assigned_by) = &query.assigned_by {
            sql_query.push(" AND assigned_by = ").push_bind(assigned_by);
        }

        if let Some(is_active) = query.is_active {
            sql_query.push(" AND is_active = ").push_bind(is_active);
        }

        if !query.include_expired.unwrap_or(false) {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            sql_query.push(" AND (expires_at IS NULL OR expires_at > ").push_bind(now).push(")");
        }

        let row = sql_query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// Get expiring role assignments (within specified days)
    pub async fn get_expiring_assignments(
        db: &Database,
        days: i32,
    ) -> Result<Vec<UserRole>, sqlx::Error> {
        let future_date = chrono::Utc::now() + chrono::Duration::days(days as i64);
        let future_date_str = future_date.format("%Y-%m-%d %H:%M:%S").to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let user_roles = sqlx::query_as::<_, UserRole>(
            r#"
            SELECT id, user_id, role_id, assigned_by, assigned_at, expires_at, is_active 
            FROM user_roles 
            WHERE is_active = true 
              AND expires_at IS NOT NULL 
              AND expires_at > ? 
              AND expires_at <= ?
            ORDER BY expires_at ASC
            "#,
        )
        .bind(now)
        .bind(future_date_str)
        .fetch_all(db.pool())
        .await?;

        Ok(user_roles)
    }

    /// Check if the role assignment has expired
    pub fn is_expired(&self) -> bool {
        match &self.expires_at {
            Some(expires) => {
                let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                expires < &now
            }
            None => false, // No expiration
        }
    }

    /// Check if the role assignment is currently valid
    pub fn is_valid(&self) -> bool {
        self.is_active && !self.is_expired()
    }

    /// Deactivate the role assignment
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    /// Activate the role assignment
    pub fn activate(&mut self) {
        self.is_active = true;
    }

    /// Extend the expiration date
    pub fn extend_expiration(&mut self, new_expiration: String) {
        self.expires_at = Some(new_expiration);
    }

    /// Remove expiration (make permanent)
    pub fn make_permanent(&mut self) {
        self.expires_at = None;
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

    /// Check if role assignment is expiring soon (within 7 days)
    pub fn is_expiring_soon(&self) -> bool {
        if let Some(days) = self.days_until_expiration() {
            days > 0 && days <= 7
        } else {
            false
        }
    }
}

impl UserWithRoles {
    /// Create a new user with roles
    pub fn new(user_id: String, user_name: String, user_email: Option<String>) -> Self {
        Self { user_id, user_name, user_email, roles: Vec::new() }
    }

    /// Add a role to the user
    pub fn add_role(&mut self, role: RoleInfo) {
        self.roles.push(role);
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role_id: &str) -> bool {
        self.roles.iter().any(|r| r.role_id == role_id)
    }

    /// Check if user has administrator role
    pub fn is_administrator(&self) -> bool {
        self.roles.iter().any(|r| r.is_administrator)
    }

    /// Get all role IDs
    pub fn get_role_ids(&self) -> Vec<String> {
        self.roles.iter().map(|r| r.role_id.clone()).collect()
    }

    /// Get active roles (not expired)
    pub fn get_active_roles(&self) -> Vec<&RoleInfo> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.roles
            .iter()
            .filter(|r| match &r.expires_at {
                Some(expires) => expires > &now,
                None => true,
            })
            .collect()
    }

    /// Get count of active roles
    pub fn active_role_count(&self) -> usize {
        self.get_active_roles().len()
    }
}

impl RoleInfo {
    /// Create a new role info
    pub fn new(
        role_id: String,
        role_name: String,
        role_description: Option<String>,
        is_administrator: bool,
        assigned_at: String,
        expires_at: Option<String>,
    ) -> Self {
        Self { role_id, role_name, role_description, is_administrator, assigned_at, expires_at }
    }

    /// Check if role is expired
    pub fn is_expired(&self) -> bool {
        match &self.expires_at {
            Some(expires) => {
                let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                expires < &now
            }
            None => false,
        }
    }

    /// Check if role is active
    pub fn is_active(&self) -> bool {
        !self.is_expired()
    }
}

// Backward compatibility
pub type UserRolesDto = UserRole;
pub type UserRoleQueryParams = UserRoleQuery;

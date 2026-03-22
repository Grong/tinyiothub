use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row};

use crate::infrastructure::persistence::database::Database;

/// 用户实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: bool,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// 用户DTO（用于API响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserDto {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: bool,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// 用户统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserStatisticsNew {
    pub total_users: i64,
    pub enabled_users: i64,
    pub disabled_users: i64,
    pub recent_logins: i64,
}

/// 用户查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserQueryParams {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建用户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
}

/// 更新用户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
}

/// 用户登录请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 修改密码请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

impl User {
    /// 根据 ID 查找用户
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<User>, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, username, password_hash, email, phone, display_name, 
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await?;

        Ok(user)
    }

    /// 根据用户名查找用户
    pub async fn find_by_username(
        db: &Database,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, username, password_hash, email, phone, display_name, 
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE username = ?
            "#,
        )
        .bind(username)
        .fetch_optional(db.pool())
        .await?;

        Ok(user)
    }

    /// 根据邮箱查找用户
    pub async fn find_by_email(db: &Database, email: &str) -> Result<Option<User>, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, username, password_hash, email, phone, display_name, 
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE email = ?
            "#,
        )
        .bind(email)
        .fetch_optional(db.pool())
        .await?;

        Ok(user)
    }

    /// 创建新用户
    pub async fn create(db: &Database, request: &CreateUserRequest) -> Result<User, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用 bcrypt 进行安全密码哈希（必须成功，不允许降级）
        let password_hash =
            crate::utils::password::hash_password(&request.password).map_err(|e| {
                tracing::error!("Failed to hash password during user creation: {}", e);
                sqlx::Error::Protocol(format!("password hashing failed: {}", e))
            })?;

        sqlx::query(
            r#"
            INSERT INTO users (
                id, username, password_hash, email, phone, display_name,
                is_enabled, parent_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&request.username)
        .bind(&password_hash)
        .bind(&request.email)
        .bind(&request.phone)
        .bind(&request.display_name)
        .bind(request.is_enabled.unwrap_or(true))
        .bind(&request.parent_id)
        .bind(&now)
        .bind(&now)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新用户信息
    pub async fn update(
        db: &Database,
        id: &str,
        request: &UpdateUserRequest,
    ) -> Result<User, sqlx::Error> {
        let mut query = QueryBuilder::new("UPDATE users SET ");
        let mut has_updates = false;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if let Some(username) = &request.username {
            if has_updates {
                query.push(", ");
            }
            query.push("username = ").push_bind(username);
            has_updates = true;
        }

        if let Some(email) = &request.email {
            if has_updates {
                query.push(", ");
            }
            query.push("email = ").push_bind(email);
            has_updates = true;
        }

        if let Some(phone) = &request.phone {
            if has_updates {
                query.push(", ");
            }
            query.push("phone = ").push_bind(phone);
            has_updates = true;
        }

        if let Some(display_name) = &request.display_name {
            if has_updates {
                query.push(", ");
            }
            query.push("display_name = ").push_bind(display_name);
            has_updates = true;
        }

        if let Some(is_enabled) = &request.is_enabled {
            if has_updates {
                query.push(", ");
            }
            query.push("is_enabled = ").push_bind(is_enabled);
            has_updates = true;
        }

        if let Some(parent_id) = &request.parent_id {
            if has_updates {
                query.push(", ");
            }
            query.push("parent_id = ").push_bind(parent_id);
            has_updates = true;
        }

        if !has_updates {
            return Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound);
        }

        query.push(", updated_at = ").push_bind(now);
        query.push(" WHERE id = ").push_bind(id);

        let result = query.build().execute(db.pool()).await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 删除用户
    pub async fn delete(db: &Database, id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM users WHERE id = ?").bind(id).execute(db.pool()).await?;

        Ok(result.rows_affected())
    }

    /// 查询用户列表
    pub async fn find_all(
        db: &Database,
        params: &UserQueryParams,
    ) -> Result<Vec<User>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, username, password_hash, email, phone, display_name, 
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE 1=1
            "#,
        );

        if let Some(username) = &params.username {
            query.push(" AND username LIKE ").push_bind(format!("%{}%", username));
        }

        if let Some(email) = &params.email {
            query.push(" AND email LIKE ").push_bind(format!("%{}%", email));
        }

        if let Some(display_name) = &params.display_name {
            query.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
        }

        if let Some(is_enabled) = &params.is_enabled {
            query.push(" AND is_enabled = ").push_bind(is_enabled);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        query.push(" ORDER BY created_at DESC");

        if let Some(page_size) = params.page_size {
            let offset = params.page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let users = query.build_query_as::<User>().fetch_all(db.pool()).await?;

        Ok(users)
    }

    /// 统计用户数量
    pub async fn count(db: &Database, params: &UserQueryParams) -> Result<i64, sqlx::Error> {
        let mut query = QueryBuilder::new("SELECT COUNT(*) as count FROM users WHERE 1=1");

        if let Some(username) = &params.username {
            query.push(" AND username LIKE ").push_bind(format!("%{}%", username));
        }

        if let Some(email) = &params.email {
            query.push(" AND email LIKE ").push_bind(format!("%{}%", email));
        }

        if let Some(display_name) = &params.display_name {
            query.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
        }

        if let Some(is_enabled) = &params.is_enabled {
            query.push(" AND is_enabled = ").push_bind(is_enabled);
        }

        if let Some(parent_id) = &params.parent_id {
            query.push(" AND parent_id = ").push_bind(parent_id);
        }

        let row = query.build().fetch_one(db.pool()).await?;
        let count: i64 = row.get("count");

        Ok(count)
    }

    /// 验证用户登录
    pub async fn verify_login(
        db: &Database,
        username: &str,
        password: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        if let Some(user) = Self::find_by_username(db, username).await? {
            // 使用 bcrypt 验证密码
            use crate::utils::password::verify_password;
            if verify_password(password, &user.password_hash).is_ok() && user.is_enabled {
                return Ok(Some(user));
            }
        }
        Ok(None)
    }

    /// 更新最后登录时间
    pub async fn update_last_login(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(())
    }

    /// 修改密码
    pub async fn change_password(
        db: &Database,
        id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<bool, sqlx::Error> {
        if let Some(user) = Self::find_by_id(db, id).await? {
            // Use bcrypt to verify old password
            use crate::utils::password::{hash_password, verify_password};
            if verify_password(old_password, &user.password_hash).is_ok() {
                let new_hash = hash_password(new_password).map_err(|e| {
                    tracing::error!("Failed to hash new password during change: {}", e);
                    sqlx::Error::Protocol(format!("password hashing failed: {}", e))
                })?;
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

                sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                    .bind(&new_hash)
                    .bind(&now)
                    .bind(id)
                    .execute(db.pool())
                    .await?;

                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 检查用户名是否存在
    pub async fn exists_by_username(db: &Database, username: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = ?")
            .bind(username)
            .fetch_one(db.pool())
            .await?;

        Ok(count > 0)
    }

    /// 检查邮箱是否存在
    pub async fn exists_by_email(db: &Database, email: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?")
            .bind(email)
            .fetch_one(db.pool())
            .await?;

        Ok(count > 0)
    }

    /// 获取用户显示名称
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.username)
    }

    /// 检查用户是否启用
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// 检查用户是否有父用户
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// 用户认证（登录验证）
    pub async fn authenticate(
        db: &Database,
        username: &str,
        password: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        Self::verify_login(db, username, password).await
    }

    /// 更新最后登录时间（别名方法，兼容旧代码）
    pub async fn update_last_logon(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        Self::update_last_login(db, id).await
    }

    /// 检查用户是否启用（别名方法，兼容旧代码）
    pub fn enabled(&self) -> bool {
        self.is_enabled
    }

    /// 带过滤条件查找用户列表
    pub async fn find_with_filters(
        db: &Database,
        enabled: Option<bool>,
        search: Option<String>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<User>, sqlx::Error> {
        let mut query = QueryBuilder::new(
            r#"
            SELECT id, username, password_hash, email, phone, display_name, 
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE 1=1
            "#,
        );

        if let Some(enabled) = enabled {
            query.push(" AND is_enabled = ").push_bind(enabled);
        }

        if let Some(search) = &search {
            query
                .push(" AND (username LIKE ")
                .push_bind(format!("%{}%", search))
                .push(" OR display_name LIKE ")
                .push_bind(format!("%{}%", search))
                .push(" OR email LIKE ")
                .push_bind(format!("%{}%", search))
                .push(")");
        }

        query.push(" ORDER BY created_at DESC");

        if let Some(page_size) = page_size {
            let offset = page.unwrap_or(1).saturating_sub(1) * page_size;
            query.push(" LIMIT ").push_bind(page_size as i64);
            query.push(" OFFSET ").push_bind(offset as i64);
        }

        let users = query.build_query_as::<User>().fetch_all(db.pool()).await?;

        Ok(users)
    }

    /// 将用户列表转换为DTO列表
    pub fn to_dto_list(users: Vec<User>) -> Vec<UserDto> {
        users.into_iter().map(|user| user.to_dto()).collect()
    }

    /// 转换为DTO
    pub fn to_dto(&self) -> UserDto {
        UserDto {
            id: self.id.clone(),
            username: self.username.clone(),
            email: self.email.clone(),
            phone: self.phone.clone(),
            display_name: self.display_name.clone(),
            is_enabled: self.is_enabled,
            parent_id: self.parent_id.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_login_at: self.last_login_at.clone(),
        }
    }

    /// 获取用户统计信息
    pub async fn get_user_statistics(db: &Database) -> Result<UserStatisticsNew, sqlx::Error> {
        let total_users: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(db.pool()).await?;

        let enabled_users: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_enabled = true")
                .fetch_one(db.pool())
                .await?;

        let disabled_users: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_enabled = false")
                .fetch_one(db.pool())
                .await?;

        // 最近7天内登录的用户数
        let recent_logins: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE last_login_at >= datetime('now', '-7 days')",
        )
        .fetch_one(db.pool())
        .await?;

        Ok(UserStatisticsNew { total_users, enabled_users, disabled_users, recent_logins })
    }

    /// 根据名称查找用户（别名方法，兼容旧代码）
    pub async fn find_by_name(db: &Database, name: &str) -> Result<Option<User>, sqlx::Error> {
        Self::find_by_username(db, name).await
    }

    /// 更新用户启用状态
    pub async fn update_enabled_status(
        db: &Database,
        id: &str,
        enabled: bool,
    ) -> Result<User, sqlx::Error> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result = sqlx::query("UPDATE users SET is_enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 验证密码
    pub fn verify_password(&self, password: &str) -> bool {
        crate::utils::password::verify_password(password, &self.password_hash).unwrap_or(false)
    }

    /// 更新密码（别名方法，兼容旧代码）
    pub async fn update_password(
        db: &Database,
        id: &str,
        new_password: &str,
    ) -> Result<(), sqlx::Error> {
        let new_hash = crate::utils::password::hash_password(new_password).map_err(|e| {
            tracing::error!("Failed to hash password for update: {}", e);
            sqlx::Error::Protocol(format!("password hashing failed: {}", e))
        })?;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
            .bind(&new_hash)
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;

        Ok(())
    }
}

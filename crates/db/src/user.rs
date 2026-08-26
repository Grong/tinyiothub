//! User 持久化：用户账户（P-集中化 E4，自 user crate 迁入）。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, SqlitePool};
use tinyiothub_core::error::{Error, Result};
use tinyiothub_core::models::user::{CreateUserRequest, UpdateUserRequest};

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 仓储契约）— 自领域 crate 迁入
// ──────────────────────────────────────────────

/// User entity
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// UserDTO (for API responses)
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

/// User statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserStatistics {
    pub total_users: i64,
    pub enabled_users: i64,
    pub disabled_users: i64,
    pub recent_logins: i64,
}

/// Backward compatibility alias
pub type UserStatisticsNew = UserStatistics;

/// User query parameters
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

impl User {
    /// Get user display name
    pub fn get_display_name(&self) -> &str {
        self.display_name.as_ref().unwrap_or(&self.username)
    }

    /// Check if user is enabled
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Check if user has parent
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Convert user to DTO
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

    /// Convert user list to DTO list
    pub fn to_dto_list(users: Vec<User>) -> Vec<UserDto> {
        users.into_iter().map(|user| user.to_dto()).collect()
    }
}

// ──────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────

/// Criteria for querying users
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserCriteria {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
    pub parent_id: Option<String>,
    pub search_text: Option<String>,
    pub sort_by: UserSortBy,
    pub sort_order: UserSortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for users
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum UserSortBy {
    #[default]
    CreatedAt,
    Username,
}

/// Sort order for users
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum UserSortOrder {
    Ascending,
    #[default]
    Descending,
}

impl UserCriteria {
    /// Create a new criteria builder
    pub fn builder() -> UserCriteriaBuilder {
        UserCriteriaBuilder::new()
    }

    /// Filter by username
    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }

    /// Filter by email
    pub fn with_email(mut self, email: String) -> Self {
        self.email = Some(email);
        self
    }

    /// Filter by display name
    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }

    /// Filter by enabled status
    pub fn with_is_enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = Some(is_enabled);
        self
    }

    /// Filter by parent ID
    pub fn with_parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Filter by search text
    pub fn with_search_text(mut self, text: String) -> Self {
        self.search_text = Some(text);
        self
    }

    /// Set sorting
    pub fn with_sort(mut self, sort_by: UserSortBy, sort_order: UserSortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    /// Set pagination
    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for UserCriteria
pub struct UserCriteriaBuilder {
    criteria: UserCriteria,
}

impl UserCriteriaBuilder {
    pub fn new() -> Self {
        Self {
            criteria: UserCriteria::default(),
        }
    }

    pub fn username(mut self, username: String) -> Self {
        self.criteria.username = Some(username);
        self
    }

    pub fn email(mut self, email: String) -> Self {
        self.criteria.email = Some(email);
        self
    }

    pub fn display_name(mut self, display_name: String) -> Self {
        self.criteria.display_name = Some(display_name);
        self
    }

    pub fn is_enabled(mut self, is_enabled: bool) -> Self {
        self.criteria.is_enabled = Some(is_enabled);
        self
    }

    pub fn parent_id(mut self, parent_id: String) -> Self {
        self.criteria.parent_id = Some(parent_id);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn sort_by(mut self, sort_by: UserSortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: UserSortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> UserCriteria {
        self.criteria
    }
}

impl Default for UserCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// --- SQLite implementation ---

/// Internal row type for sqlx mapping
#[derive(Debug, Clone, FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    email: Option<String>,
    phone: Option<String>,
    display_name: Option<String>,
    is_enabled: bool,
    parent_id: Option<String>,
    created_at: String,
    updated_at: String,
    last_login_at: Option<String>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            username: row.username,
            password_hash: row.password_hash,
            email: row.email,
            phone: row.phone,
            display_name: row.display_name,
            is_enabled: row.is_enabled,
            parent_id: row.parent_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_login_at: row.last_login_at,
        }
    }
}
// ──────────────────────────────────────────────
// 持久化函数（pub(crate) 自由函数 + Db 委托）
// ──────────────────────────────────────────────

pub(crate) async fn find_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, username, password_hash, email, phone, display_name,
               is_enabled, parent_id, created_at, updated_at, last_login_at
        FROM users WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn find_user_by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, username, password_hash, email, phone, display_name,
               is_enabled, parent_id, created_at, updated_at, last_login_at
        FROM users WHERE username = ?
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

pub(crate) async fn find_user_by_email(pool: &SqlitePool, email: &str) -> Result<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, username, password_hash, email, phone, display_name,
               is_enabled, parent_id, created_at, updated_at, last_login_at
        FROM users WHERE email = ?
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

/// 按手机号查询用户 — 自 cloud auth sms 段收编。
pub(crate) async fn find_user_by_phone(pool: &SqlitePool, phone: &str) -> Result<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, username, password_hash, email, phone, display_name,
               is_enabled, parent_id, created_at, updated_at, last_login_at
        FROM users WHERE phone = ? LIMIT 1
        "#,
    )
    .bind(phone)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

/// 按邮箱查询启用用户的登录凭据（id, username, password_hash）
/// — 自 cloud tenant/handler 登录段收编。
pub(crate) async fn find_enabled_user_credentials_by_email(
    pool: &SqlitePool,
    email: &str,
) -> Result<Option<(String, String, String)>> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, username, password_hash FROM users WHERE email = ? AND is_enabled = 1 LIMIT 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub(crate) async fn find_users(pool: &SqlitePool, criteria: &UserCriteria) -> Result<Vec<User>> {
    let mut builder = QueryBuilder::new(
        r#"
        SELECT id, username, password_hash, email, phone, display_name,
               is_enabled, parent_id, created_at, updated_at, last_login_at
        FROM users WHERE 1=1
        "#,
    );

    if let Some(username) = &criteria.username {
        builder.push(" AND username LIKE ").push_bind(format!("%{}%", username));
    }

    if let Some(email) = &criteria.email {
        builder.push(" AND email LIKE ").push_bind(format!("%{}%", email));
    }

    if let Some(display_name) = &criteria.display_name {
        builder
            .push(" AND display_name LIKE ")
            .push_bind(format!("%{}%", display_name));
    }

    if let Some(is_enabled) = &criteria.is_enabled {
        builder.push(" AND is_enabled = ").push_bind(is_enabled);
    }

    if let Some(parent_id) = &criteria.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }

    if let Some(search_text) = &criteria.search_text {
        let pattern = format!("%{}%", search_text);
        builder.push(" AND (username LIKE ").push_bind(&pattern);
        builder.push(" OR display_name LIKE ").push_bind(&pattern);
        builder.push(" OR email LIKE ").push_bind(pattern);
        builder.push(")");
    }

    match criteria.sort_by {
        UserSortBy::CreatedAt => builder.push(" ORDER BY created_at"),
        UserSortBy::Username => builder.push(" ORDER BY username"),
    };

    match criteria.sort_order {
        UserSortOrder::Ascending => builder.push(" ASC"),
        UserSortOrder::Descending => builder.push(" DESC"),
    };

    if let Some(limit) = criteria.limit {
        builder.push(" LIMIT ").push_bind(limit as i64);
    }
    if let Some(offset) = criteria.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let rows = builder.build_query_as::<UserRow>().fetch_all(pool).await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn count_users(pool: &SqlitePool, criteria: &UserCriteria) -> Result<i64> {
    let mut builder = QueryBuilder::new("SELECT COUNT(*) as count FROM users WHERE 1=1");

    if let Some(username) = &criteria.username {
        builder.push(" AND username LIKE ").push_bind(format!("%{}%", username));
    }

    if let Some(email) = &criteria.email {
        builder.push(" AND email LIKE ").push_bind(format!("%{}%", email));
    }

    if let Some(display_name) = &criteria.display_name {
        builder
            .push(" AND display_name LIKE ")
            .push_bind(format!("%{}%", display_name));
    }

    if let Some(is_enabled) = &criteria.is_enabled {
        builder.push(" AND is_enabled = ").push_bind(is_enabled);
    }

    if let Some(parent_id) = &criteria.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }

    if let Some(search_text) = &criteria.search_text {
        let pattern = format!("%{}%", search_text);
        builder.push(" AND (username LIKE ").push_bind(&pattern);
        builder.push(" OR display_name LIKE ").push_bind(&pattern);
        builder.push(" OR email LIKE ").push_bind(pattern);
        builder.push(")");
    }

    let row = builder.build().fetch_one(pool).await?;
    let count: i64 = row.get("count");
    Ok(count)
}

pub(crate) async fn create_user(pool: &SqlitePool, request: &CreateUserRequest) -> Result<User> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

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
    .bind(&request.password)
    .bind(&request.email)
    .bind(&request.phone)
    .bind(&request.display_name)
    .bind(request.is_enabled.unwrap_or(true))
    .bind(&request.parent_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_user_by_id(pool, &id).await?.ok_or(Error::NotFound)
}

pub(crate) async fn update_user(pool: &SqlitePool, id: &str, request: &UpdateUserRequest) -> Result<User> {
    let mut tx = pool.begin().await?;

    let mut builder = QueryBuilder::new("UPDATE users SET ");
    let mut has_updates = false;
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    if let Some(username) = &request.username {
        if has_updates {
            builder.push(", ");
        }
        builder.push("username = ").push_bind(username);
        has_updates = true;
    }

    if let Some(email) = &request.email {
        if has_updates {
            builder.push(", ");
        }
        builder.push("email = ").push_bind(email);
        has_updates = true;
    }

    if let Some(phone) = &request.phone {
        if has_updates {
            builder.push(", ");
        }
        builder.push("phone = ").push_bind(phone);
        has_updates = true;
    }

    if let Some(display_name) = &request.display_name {
        if has_updates {
            builder.push(", ");
        }
        builder.push("display_name = ").push_bind(display_name);
        has_updates = true;
    }

    if let Some(is_enabled) = &request.is_enabled {
        if has_updates {
            builder.push(", ");
        }
        builder.push("is_enabled = ").push_bind(is_enabled);
        has_updates = true;
    }

    if let Some(parent_id) = &request.parent_id {
        if has_updates {
            builder.push(", ");
        }
        builder.push("parent_id = ").push_bind(parent_id);
        has_updates = true;
    }

    if !has_updates {
        return find_user_by_id(pool, id).await?.ok_or(Error::NotFound);
    }

    builder.push(", updated_at = ").push_bind(&now);
    builder.push(" WHERE id = ").push_bind(id);

    let result = builder.build().execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, username, password_hash, email, phone, display_name,
               is_enabled, parent_id, created_at, updated_at, last_login_at
        FROM users WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await;

    match row {
        Ok(row) => {
            tx.commit().await?;
            Ok(row.into())
        }
        Err(_) => Err(Error::NotFound),
    }
}

pub(crate) async fn delete_user(pool: &SqlitePool, id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub(crate) async fn find_users_with_filters(
    pool: &SqlitePool,
    enabled: Option<bool>,
    search: Option<String>,
    page: u32,
    page_size: u32,
) -> Result<Vec<User>> {
    let criteria = UserCriteria {
        is_enabled: enabled,
        search_text: search,
        limit: Some(page_size),
        offset: Some((page.saturating_sub(1)) * page_size),
        ..Default::default()
    };

    find_users(pool, &criteria).await
}

pub(crate) async fn user_exists_by_username(pool: &SqlitePool, username: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

pub(crate) async fn user_exists_by_email(pool: &SqlitePool, email: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

pub(crate) async fn user_exists_by_phone(pool: &SqlitePool, phone: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE phone = ?")
        .bind(phone)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

pub(crate) async fn update_user_enabled_status(pool: &SqlitePool, id: &str, enabled: bool) -> Result<User> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query("UPDATE users SET is_enabled = ?, updated_at = ? WHERE id = ?")
        .bind(enabled)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    find_user_by_id(pool, id).await?.ok_or(Error::NotFound)
}

pub(crate) async fn update_user_password(pool: &SqlitePool, id: &str, hashed_password: &str) -> Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(hashed_password)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    Ok(())
}

pub(crate) async fn update_user_last_login(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

pub(crate) async fn get_user_statistics(pool: &SqlitePool) -> Result<UserStatisticsNew> {
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await?;

    let enabled_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_enabled = true")
        .fetch_one(pool)
        .await?;

    let disabled_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_enabled = false")
        .fetch_one(pool)
        .await?;

    let recent_logins: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE last_login_at >= datetime('now', '-7 days')")
            .fetch_one(pool)
            .await?;

    Ok(UserStatisticsNew {
        total_users,
        enabled_users,
        disabled_users,
        recent_logins,
    })
}

/// 租户注册时插入 owner 用户（username = email）— 自 cloud tenant/handler 注册段收编。
pub(crate) async fn insert_tenant_owner_user(
    pool: &SqlitePool,
    user_id: &str,
    email: &str,
    password_hash: &str,
) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, email, is_enabled, created_at, updated_at)
           VALUES (?, ?, ?, ?, 1, ?, ?)"#,
    )
    .bind(user_id)
    .bind(email)
    .bind(password_hash)
    .bind(email)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// 手机号登录：用户不存在才插入（并发安全），返回受影响行数
/// — 自 cloud auth sms 段收编（SQL 逐字迁移）。
pub(crate) async fn insert_phone_user_if_absent(pool: &SqlitePool, user_id: &str, phone: &str) -> Result<u64> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, phone, is_enabled, created_at, updated_at)
            SELECT ?, ?, '', ?, 1, ?, ?
            WHERE NOT EXISTS (SELECT 1 FROM users WHERE phone = ?)"#,
    )
    .bind(user_id)
    .bind(phone)
    .bind(phone)
    .bind(&now)
    .bind(&now)
    .bind(phone)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// 社交登录首次插入用户（无密码）— 自 cloud auth social 段收编。
pub(crate) async fn insert_social_user(pool: &SqlitePool, user_id: &str, username: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO users (id, username, is_enabled, created_at, updated_at)
           VALUES (?, ?, 1, ?, ?)"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

// ──────────────────────────────────────────────
// Db 委托（User 领域）
// ──────────────────────────────────────────────

impl Db {
    /// 按 ID 查询用户。
    pub async fn find_user_by_id(&self, id: &str) -> Result<Option<User>> {
        find_user_by_id(self.pool(), id).await
    }

    /// 按用户名查询用户。
    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>> {
        find_user_by_username(self.pool(), username).await
    }

    /// 按邮箱查询用户。
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        find_user_by_email(self.pool(), email).await
    }

    /// 按手机号查询用户。
    pub async fn find_user_by_phone(&self, phone: &str) -> Result<Option<User>> {
        find_user_by_phone(self.pool(), phone).await
    }

    /// 按邮箱查询启用用户的登录凭据（id, username, password_hash）。
    pub async fn find_enabled_user_credentials_by_email(
        &self,
        email: &str,
    ) -> Result<Option<(String, String, String)>> {
        find_enabled_user_credentials_by_email(self.pool(), email).await
    }

    /// 按条件查询用户列表。
    pub async fn find_users(&self, criteria: &UserCriteria) -> Result<Vec<User>> {
        find_users(self.pool(), criteria).await
    }

    /// 按条件统计用户数。
    pub async fn count_users(&self, criteria: &UserCriteria) -> Result<i64> {
        count_users(self.pool(), criteria).await
    }

    /// 创建用户（request.password 为已哈希口令）。
    pub async fn create_user(&self, request: &CreateUserRequest) -> Result<User> {
        create_user(self.pool(), request).await
    }

    /// 更新用户（仅更新传入字段；事务内回读）。
    pub async fn update_user(&self, id: &str, request: &UpdateUserRequest) -> Result<User> {
        update_user(self.pool(), id, request).await
    }

    /// 删除用户，返回受影响行数。
    pub async fn delete_user(&self, id: &str) -> Result<u64> {
        delete_user(self.pool(), id).await
    }

    /// 按启用状态/搜索词分页查询用户。
    pub async fn find_users_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<String>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<User>> {
        find_users_with_filters(self.pool(), enabled, search, page, page_size).await
    }

    /// 用户名是否已存在。
    pub async fn user_exists_by_username(&self, username: &str) -> Result<bool> {
        user_exists_by_username(self.pool(), username).await
    }

    /// 邮箱是否已存在。
    pub async fn user_exists_by_email(&self, email: &str) -> Result<bool> {
        user_exists_by_email(self.pool(), email).await
    }

    /// 手机号是否已存在。
    pub async fn user_exists_by_phone(&self, phone: &str) -> Result<bool> {
        user_exists_by_phone(self.pool(), phone).await
    }

    /// 更新用户启用状态。
    pub async fn update_user_enabled_status(&self, id: &str, enabled: bool) -> Result<User> {
        update_user_enabled_status(self.pool(), id, enabled).await
    }

    /// 更新用户口令（已哈希）。
    pub async fn update_user_password(&self, id: &str, hashed_password: &str) -> Result<()> {
        update_user_password(self.pool(), id, hashed_password).await
    }

    /// 更新用户最近登录时间。
    pub async fn update_user_last_login(&self, id: &str) -> Result<()> {
        update_user_last_login(self.pool(), id).await
    }

    /// 用户统计（总数/启用/禁用/近 7 天登录）。
    pub async fn get_user_statistics(&self) -> Result<UserStatisticsNew> {
        get_user_statistics(self.pool()).await
    }

    /// 租户注册时插入 owner 用户（username = email）。
    pub async fn insert_tenant_owner_user(&self, user_id: &str, email: &str, password_hash: &str) -> Result<()> {
        insert_tenant_owner_user(self.pool(), user_id, email, password_hash).await
    }

    /// 手机号登录：用户不存在才插入（并发安全），返回受影响行数。
    pub async fn insert_phone_user_if_absent(&self, user_id: &str, phone: &str) -> Result<u64> {
        insert_phone_user_if_absent(self.pool(), user_id, phone).await
    }

    /// 社交登录首次插入用户（无密码）。
    pub async fn insert_social_user(&self, user_id: &str, username: &str) -> Result<()> {
        insert_social_user(self.pool(), user_id, username).await
    }
}

// ──────────────────────────────────────────────
// 初始化引导查询（自 cloud shared/initialization.rs 迁入，Task 12）
// ──────────────────────────────────────────────

/// 查询用户 display_name。
pub(crate) async fn find_user_display_name(
    pool: &SqlitePool,
    user_id: &str,
) -> std::result::Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> = sqlx::query_as("SELECT display_name FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(name,)| name))
}

impl Db {
    /// 查询用户 display_name。
    pub async fn find_user_display_name(&self, user_id: &str) -> std::result::Result<Option<String>, sqlx::Error> {
        find_user_display_name(self.pool(), user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criteria_builder() {
        let criteria = UserCriteria::builder()
            .username("admin".to_string())
            .email("admin@example.com".to_string())
            .is_enabled(true)
            .sort_by(UserSortBy::Username)
            .sort_order(UserSortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.username, Some("admin".to_string()));
        assert_eq!(criteria.email, Some("admin@example.com".to_string()));
        assert_eq!(criteria.is_enabled, Some(true));
        assert!(matches!(criteria.sort_by, UserSortBy::Username));
        assert!(matches!(criteria.sort_order, UserSortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let criteria = UserCriteria::default()
            .with_username("user-01".to_string())
            .with_is_enabled(false)
            .with_sort(UserSortBy::CreatedAt, UserSortOrder::Descending)
            .with_pagination(50, 10);

        assert_eq!(criteria.username, Some("user-01".to_string()));
        assert_eq!(criteria.is_enabled, Some(false));
        assert!(matches!(criteria.sort_by, UserSortBy::CreatedAt));
        assert!(matches!(criteria.sort_order, UserSortOrder::Descending));
        assert_eq!(criteria.limit, Some(50));
        assert_eq!(criteria.offset, Some(10));
    }
}

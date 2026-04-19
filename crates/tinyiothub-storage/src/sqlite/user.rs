use async_trait::async_trait;
use sqlx::{FromRow, QueryBuilder, Row};

use crate::traits::user::{
        UserCriteria, UserRepository, UserSortBy, UserSortOrder,
    };
use tinyiothub_core::models::user::{CreateUserRequest, UpdateUserRequest, User, UserStatisticsNew};
use crate::sqlite::database::Database;
use tinyiothub_core::error::{Error, Result};

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

/// SQLite implementation of UserRepository
#[derive(Debug, Clone)]
pub struct SqliteUserRepository {
    database: Database,
}

impl SqliteUserRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, password_hash, email, phone, display_name,
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, password_hash, email, phone, display_name,
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE username = ?
            "#,
        )
        .bind(username)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, password_hash, email, phone, display_name,
                   is_enabled, parent_id, created_at, updated_at, last_login_at
            FROM users WHERE email = ?
            "#,
        )
        .bind(email)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn find_all(&self, criteria: &UserCriteria) -> Result<Vec<User>> {
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
            builder.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
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

        let rows = builder.build_query_as::<UserRow>()
            .fetch_all(self.database.pool())
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count(&self, criteria: &UserCriteria) -> Result<i64> {
        let mut builder = QueryBuilder::new("SELECT COUNT(*) as count FROM users WHERE 1=1");

        if let Some(username) = &criteria.username {
            builder.push(" AND username LIKE ").push_bind(format!("%{}%", username));
        }

        if let Some(email) = &criteria.email {
            builder.push(" AND email LIKE ").push_bind(format!("%{}%", email));
        }

        if let Some(display_name) = &criteria.display_name {
            builder.push(" AND display_name LIKE ").push_bind(format!("%{}%", display_name));
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

        let row = builder.build().fetch_one(self.database.pool()).await?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    async fn create(&self, request: &CreateUserRequest) -> Result<User> {
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
        .execute(self.database.pool())
        .await?;

        self.find_by_id(&id).await?.ok_or(Error::NotFound)
    }

    async fn update(&self, id: &str, request: &UpdateUserRequest) -> Result<User> {
        let mut tx = self.database.pool().begin().await?;

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
            return self.find_by_id(id).await?.ok_or(Error::NotFound);
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

    async fn delete(&self, id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;
        Ok(result.rows_affected())
    }

    async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<String>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<User>> {
        let mut criteria = UserCriteria::default();
        criteria.is_enabled = enabled;
        if let Some(search) = search {
            criteria.search_text = Some(search);
        }
        criteria.limit = Some(page_size);
        criteria.offset = Some((page.saturating_sub(1)) * page_size);

        self.find_all(&criteria).await
    }

    async fn exists_by_username(&self, username: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = ?")
            .bind(username)
            .fetch_one(self.database.pool())
            .await?;
        Ok(count > 0)
    }

    async fn exists_by_email(&self, email: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?")
            .bind(email)
            .fetch_one(self.database.pool())
            .await?;
        Ok(count > 0)
    }

    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<User> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result = sqlx::query(
            "UPDATE users SET is_enabled = ?, updated_at = ? WHERE id = ?"
        )
        .bind(enabled)
        .bind(&now)
        .bind(id)
        .execute(self.database.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound);
        }

        self.find_by_id(id).await?.ok_or(Error::NotFound)
    }

    async fn update_password(&self, id: &str, hashed_password: &str) -> Result<()> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result = sqlx::query(
            "UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?"
        )
        .bind(hashed_password)
        .bind(&now)
        .bind(id)
        .execute(self.database.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound);
        }

        Ok(())
    }

    async fn update_last_login(&self, id: &str) -> Result<()> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn get_user_statistics(&self) -> Result<UserStatisticsNew> {
        let total_users: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users")
                .fetch_one(self.database.pool())
                .await?;

        let enabled_users: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_enabled = true")
                .fetch_one(self.database.pool())
                .await?;

        let disabled_users: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_enabled = false")
                .fetch_one(self.database.pool())
                .await?;

        let recent_logins: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE last_login_at >= datetime('now', '-7 days')"
        )
        .fetch_one(self.database.pool())
        .await?;

        Ok(UserStatisticsNew {
            total_users,
            enabled_users,
            disabled_users,
            recent_logins,
        })
    }
}

//! Auth 持久化：认证专有表（Task 7 自 cloud auth handler 裸 SQL 收编）。
//!
//! 表归属：token_blacklist（登出黑名单）、sms_codes（短信验证码，Redis
//! 不可用时的降级存储）、social_bindings / social_configs（第三方登录）。
//! users / tenant_users / workspaces 表的 SQL 归 user/tenant/workspace 领域文件。

use sqlx::SqlitePool;

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化函数（pub(crate) 自由函数 + Db 委托）
// ──────────────────────────────────────────────

/// 登出时把 token 哈希写入黑名单（1 天过期）。
pub(crate) async fn insert_token_blacklist(
    pool: &SqlitePool,
    token_hash: &str,
    expires_at: &str,
) -> std::result::Result<(), sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("INSERT INTO token_blacklist (id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(&now)
        .execute(pool)
        .await?;

    Ok(())
}

/// 存储短信验证码（Redis 不可用时的降级路径）。
pub(crate) async fn insert_sms_code(
    pool: &SqlitePool,
    phone: &str,
    code: &str,
    purpose: &str,
    expires_at: &str,
) -> std::result::Result<(), sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"INSERT INTO sms_codes (id, phone, code, purpose, expires_at)
            VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(phone)
    .bind(code)
    .bind(purpose)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// 取最新一条未验证的短信验证码（code, expires_at）。
pub(crate) async fn find_latest_sms_code(
    pool: &SqlitePool,
    phone: &str,
    purpose: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let row: Option<(String, String)> = sqlx::query_as(
        r#"SELECT code, expires_at FROM sms_codes
            WHERE phone = ? AND purpose = ?
            AND verified_at IS NULL
            ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(phone)
    .bind(purpose)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// 按第三方身份查绑定的 user_id。
pub(crate) async fn find_social_binding_user_id(
    pool: &SqlitePool,
    provider: &str,
    provider_user_id: &str,
) -> std::result::Result<Option<String>, sqlx::Error> {
    let user_id: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM social_bindings WHERE provider = ? AND provider_user_id = ? LIMIT 1",
    )
    .bind(provider)
    .bind(provider_user_id)
    .fetch_optional(pool)
    .await?;

    Ok(user_id)
}

/// 存储社交账号绑定（已存在则忽略）。
pub(crate) async fn insert_social_binding(
    pool: &SqlitePool,
    user_id: &str,
    provider: &str,
    provider_user_id: &str,
) -> std::result::Result<(), sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO social_bindings (id, user_id, provider, provider_user_id, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(provider, provider_user_id) DO NOTHING"#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(provider)
    .bind(provider_user_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// 更新社交登录配置。
pub(crate) async fn update_social_config(
    pool: &SqlitePool,
    provider: &str,
    app_id: &str,
    app_secret: &str,
    redirect_uri: &str,
    is_enabled: bool,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE social_configs
            SET app_id = ?, app_secret = ?, redirect_uri = ?, is_enabled = ?, updated_at = CURRENT_TIMESTAMP
            WHERE provider = ?"#,
    )
    .bind(app_id)
    .bind(app_secret)
    .bind(redirect_uri)
    .bind(is_enabled as i32)
    .bind(provider)
    .execute(pool)
    .await?;

    Ok(())
}

// ──────────────────────────────────────────────
// Db 委托（Auth 领域）
// ──────────────────────────────────────────────

impl Db {
    /// 登出时把 token 哈希写入黑名单。
    pub async fn insert_token_blacklist(&self, token_hash: &str, expires_at: &str) -> std::result::Result<(), sqlx::Error> {
        insert_token_blacklist(self.pool(), token_hash, expires_at).await
    }

    /// 存储短信验证码（Redis 降级路径）。
    pub async fn insert_sms_code(
        &self,
        phone: &str,
        code: &str,
        purpose: &str,
        expires_at: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        insert_sms_code(self.pool(), phone, code, purpose, expires_at).await
    }

    /// 取最新一条未验证的短信验证码（code, expires_at）。
    pub async fn find_latest_sms_code(
        &self,
        phone: &str,
        purpose: &str,
    ) -> Result<Option<(String, String)>, sqlx::Error> {
        find_latest_sms_code(self.pool(), phone, purpose).await
    }

    /// 按第三方身份查绑定的 user_id。
    pub async fn find_social_binding_user_id(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        find_social_binding_user_id(self.pool(), provider, provider_user_id).await
    }

    /// 存储社交账号绑定（已存在则忽略）。
    pub async fn insert_social_binding(
        &self,
        user_id: &str,
        provider: &str,
        provider_user_id: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        insert_social_binding(self.pool(), user_id, provider, provider_user_id).await
    }

    /// 更新社交登录配置。
    pub async fn update_social_config(
        &self,
        provider: &str,
        app_id: &str,
        app_secret: &str,
        redirect_uri: &str,
        is_enabled: bool,
    ) -> std::result::Result<(), sqlx::Error> {
        update_social_config(self.pool(), provider, app_id, app_secret, redirect_uri, is_enabled).await
    }
}

/// 检查 token_hash 是否在黑名单中（自 cloud api/middleware/context.rs 迁入）。
pub(crate) async fn token_blacklist_contains(pool: &SqlitePool, token_hash: &str) -> std::result::Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT 1 FROM token_blacklist WHERE token_hash = ? LIMIT 1")
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

impl Db {
    /// 检查 token_hash 是否在黑名单中。
    pub async fn token_blacklist_contains(&self, token_hash: &str) -> std::result::Result<bool, sqlx::Error> {
        token_blacklist_contains(self.pool(), token_hash).await
    }
}

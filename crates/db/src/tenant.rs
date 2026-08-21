//! Tenant 持久化：租户与 API Key（P-集中化 E4，自 tenant crate 迁入）。
//!
//! 类型随 repo 住 db（方案 B）：Tenant/ApiKey 及请求/查询契约为 DB 行类型，
//! tenant crate 保留 service/handler，经 re-export 兼容。

use tinyiothub_core::models::tenant::{CreateApiKeyRequest, CreateTenantRequest};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tinyiothub_core::error::{Error, Result};

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型（DB 行 + 仓储契约）— 自 tenant/types.rs 迁入
// ──────────────────────────────────────────────

/// Subscription plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SubscriptionPlan {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub device_limit: i32,
    pub api_call_limit: i32,
    pub storage_mb: i32,
    pub user_limit: i32,
    pub price_monthly: f64,
    pub price_yearly: f64,
    pub features: String,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Tenant entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan_id: String,
    pub subscription_status: String,
    pub trial_expires_at: Option<String>,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: String,
    pub locale: String,
    pub custom_logo: Option<String>,
    pub custom_theme: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Tenant query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TenantQueryParams {
    pub status: Option<String>,
    pub plan_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Tenant usage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TenantUsage {
    pub id: String,
    pub tenant_id: String,
    pub device_count: i32,
    pub api_call_count: i32,
    pub api_call_reset_at: Option<String>,
    pub storage_used_bytes: i64,
    pub user_count: i32,
    pub total_api_calls: i64,
    pub total_api_errors: i64,
    pub updated_at: String,
}

/// API Key entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiKey {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub permissions: String,
    pub rate_limit: i32,
    pub is_enabled: bool,
    pub is_revoked: bool,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub request_count: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// API usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiUsageStats {
    pub total_calls: i64,
    pub success_calls: i64,
    pub error_calls: i64,
    pub avg_latency_ms: f64,
    pub period_start: String,
    pub period_end: String,
}

// ──────────────────────────────────────────────
// 持久化函数（pub(crate) 自由函数 + Db 委托）
// ──────────────────────────────────────────────

fn generate_secure_key() -> String {
    let mut bytes = [0u8; 36];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    bytes.iter().map(|b| CHARS[(b % 62) as usize] as char).collect()
}

fn tenant_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Tenant> {
    Ok(Tenant {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        slug: row.try_get("slug")?,
        status: row.try_get("status")?,
        plan_id: row.try_get("plan_id")?,
        subscription_status: row.try_get("subscription_status")?,
        trial_expires_at: row.try_get("trial_expires_at")?,
        billing_email: row.try_get("billing_email")?,
        billing_contact: row.try_get("billing_contact")?,
        timezone: row.try_get("timezone")?,
        locale: row.try_get("locale")?,
        custom_logo: row.try_get("custom_logo")?,
        custom_theme: row.try_get("custom_theme")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn api_key_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ApiKey> {
    Ok(ApiKey {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        name: row.try_get("name")?,
        key_hash: row.try_get("key_hash")?,
        prefix: row.try_get("prefix")?,
        permissions: row.try_get("permissions")?,
        rate_limit: row.try_get("rate_limit")?,
        is_enabled: row.try_get::<i32, _>("is_enabled")? != 0,
        is_revoked: row.try_get::<i32, _>("is_revoked")? != 0,
        last_used_at: row.try_get("last_used_at")?,
        last_used_ip: row.try_get("last_used_ip")?,
        request_count: row.try_get("request_count")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub(crate) async fn list_subscription_plans(pool: &SqlitePool) -> Result<Vec<SubscriptionPlan>> {
    let rows = sqlx::query("SELECT * FROM subscription_plans ORDER BY sort_order ASC")
        .fetch_all(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    let mut plans = Vec::with_capacity(rows.len());
    for row in &rows {
        plans.push(SubscriptionPlan {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            display_name: row.try_get("display_name")?,
            description: row.try_get("description")?,
            device_limit: row.try_get("device_limit")?,
            api_call_limit: row.try_get("api_call_limit")?,
            storage_mb: row.try_get("storage_mb")?,
            user_limit: row.try_get("user_limit")?,
            price_monthly: row.try_get("price_monthly")?,
            price_yearly: row.try_get("price_yearly")?,
            features: row.try_get("features")?,
            sort_order: row.try_get("sort_order")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }

    Ok(plans)
}

pub(crate) async fn find_subscription_plan_by_id(pool: &SqlitePool, id: &str) -> Result<Option<SubscriptionPlan>> {
    let row = sqlx::query("SELECT * FROM subscription_plans WHERE id = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    if let Some(row) = row {
        Ok(Some(SubscriptionPlan {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            display_name: row.try_get("display_name")?,
            description: row.try_get("description")?,
            device_limit: row.try_get("device_limit")?,
            api_call_limit: row.try_get("api_call_limit")?,
            storage_mb: row.try_get("storage_mb")?,
            user_limit: row.try_get("user_limit")?,
            price_monthly: row.try_get("price_monthly")?,
            price_yearly: row.try_get("price_yearly")?,
            features: row.try_get("features")?,
            sort_order: row.try_get("sort_order")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        }))
    } else {
        Ok(None)
    }
}

pub(crate) async fn create_tenant(pool: &SqlitePool, req: &CreateTenantRequest) -> Result<Tenant> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let trial_expires = (chrono::Utc::now() + chrono::Duration::days(14))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, slug, status, plan_id, subscription_status,
            trial_expires_at, billing_email, billing_contact, timezone, locale,
            created_at, updated_at)
        VALUES (?, ?, ?, 'trial', 'plan_free', 'active',
            ?, ?, ?, ?, ?,
            ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.slug)
    .bind(&trial_expires)
    .bind(req.billing_email.as_deref().unwrap_or(""))
    .bind(req.billing_contact.as_deref().unwrap_or(""))
    .bind(req.timezone.as_deref().unwrap_or("Asia/Shanghai"))
    .bind(req.locale.as_deref().unwrap_or("zh-CN"))
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO tenant_usage (id, tenant_id, device_count, api_call_count, storage_used_bytes, user_count, total_api_calls, total_api_errors, updated_at)
        VALUES (?, ?, 0, 0, 0, 1, 0, 0, ?)
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&id)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    find_tenant_by_id(pool, &id).await?.ok_or(Error::NotFound)
}

pub(crate) async fn find_tenant_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Tenant>> {
    let row = sqlx::query("SELECT * FROM tenants WHERE id = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    match row {
        Some(row) => Ok(Some(tenant_from_row(&row)?)),
        None => Ok(None),
    }
}

pub(crate) async fn find_tenant_by_slug(pool: &SqlitePool, slug: &str) -> Result<Option<Tenant>> {
    let row = sqlx::query("SELECT * FROM tenants WHERE slug = ? LIMIT 1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    match row {
        Some(row) => Ok(Some(tenant_from_row(&row)?)),
        None => Ok(None),
    }
}

/// 用户所属租户（tenant_users 关联，取第一个）— 自 cloud tenant/handler 登录段收编。
pub(crate) async fn find_tenant_by_user_id(pool: &SqlitePool, user_id: &str) -> Result<Option<Tenant>> {
    let row = sqlx::query(
        "SELECT t.* FROM tenants t
         INNER JOIN tenant_users tu ON t.id = tu.tenant_id
         WHERE tu.user_id = ? LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    match row {
        Some(row) => Ok(Some(tenant_from_row(&row)?)),
        None => Ok(None),
    }
}

/// 用户所属 tenant_id（tenant_users 关联）— 自 cloud auth 登录上下文收编。
pub(crate) async fn find_tenant_id_by_user_id(pool: &SqlitePool, user_id: &str) -> Result<Option<String>> {
    let tenant_id: Option<String> = sqlx::query_scalar("SELECT tenant_id FROM tenant_users WHERE user_id = ? LIMIT 1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(tenant_id)
}

/// 添加租户成员（tenant_users 行）— 自 cloud tenant/handler 注册段收编。
pub(crate) async fn insert_tenant_user(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: &str,
    role: &str,
    invitation_status: &str,
) -> Result<()> {
    let tenant_user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO tenant_users (id, tenant_id, user_id, role, invitation_status, joined_at, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&tenant_user_id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .bind(invitation_status)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    Ok(())
}

pub(crate) async fn get_tenant_usage(pool: &SqlitePool, tenant_id: &str) -> Result<Option<TenantUsage>> {
    let row = sqlx::query("SELECT * FROM tenant_usage WHERE tenant_id = ? LIMIT 1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    if let Some(row) = row {
        Ok(Some(TenantUsage {
            id: row.try_get("id")?,
            tenant_id: row.try_get("tenant_id")?,
            device_count: row.try_get("device_count")?,
            api_call_count: row.try_get("api_call_count")?,
            api_call_reset_at: row.try_get("api_call_reset_at")?,
            storage_used_bytes: row.try_get("storage_used_bytes")?,
            user_count: row.try_get("user_count")?,
            total_api_calls: row.try_get("total_api_calls")?,
            total_api_errors: row.try_get("total_api_errors")?,
            updated_at: row.try_get("updated_at")?,
        }))
    } else {
        Ok(None)
    }
}

pub(crate) async fn change_tenant_plan(pool: &SqlitePool, tenant_id: &str, plan_id: &str) -> Result<Tenant> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"
        UPDATE tenants SET
            plan_id = ?,
            subscription_status = 'active',
            trial_expires_at = NULL,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(plan_id)
    .bind(&now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    find_tenant_by_id(pool, tenant_id).await?.ok_or(Error::NotFound)
}

pub(crate) async fn suspend_tenant(pool: &SqlitePool, tenant_id: &str) -> Result<Tenant> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("UPDATE tenants SET status = 'suspended', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    find_tenant_by_id(pool, tenant_id).await?.ok_or(Error::NotFound)
}

pub(crate) async fn activate_tenant(pool: &SqlitePool, tenant_id: &str) -> Result<Tenant> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("UPDATE tenants SET status = 'active', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    find_tenant_by_id(pool, tenant_id).await?.ok_or(Error::NotFound)
}

pub(crate) async fn create_api_key(
    pool: &SqlitePool,
    workspace_id: &str,
    req: &CreateApiKeyRequest,
) -> Result<(ApiKey, String)> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let raw_key = format!("tinh_{}", generate_secure_key());
    let prefix = raw_key[..12].to_string();
    let key_hash = format!("{:x}", Sha256::digest(raw_key.as_bytes()));

    let permissions = req
        .permissions
        .as_ref()
        .map(|p| serde_json::to_string(p).unwrap_or_else(|_| "[\"read\"]".to_string()))
        .unwrap_or_else(|| "[\"read\"]".to_string());

    let rate_limit = req.rate_limit.unwrap_or(60);

    let expires_at = req.expires_in_days.map(|days| {
        (chrono::Utc::now() + chrono::Duration::days(days as i64))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    });

    sqlx::query(
        r#"
        INSERT INTO api_keys (id, workspace_id, name, key_hash, prefix, permissions, rate_limit, is_enabled, is_revoked, expires_at, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, 1, 0, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(&req.name)
    .bind(&key_hash)
    .bind(&prefix)
    .bind(&permissions)
    .bind(rate_limit)
    .bind(expires_at.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    let key = find_api_key_by_id(pool, &id).await?.ok_or(Error::NotFound)?;

    Ok((key, raw_key))
}

pub(crate) async fn find_api_key_by_id(pool: &SqlitePool, id: &str) -> Result<Option<ApiKey>> {
    let row = sqlx::query("SELECT * FROM api_keys WHERE id = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    match row {
        Some(row) => Ok(Some(api_key_from_row(&row)?)),
        None => Ok(None),
    }
}

pub(crate) async fn find_api_key_by_prefix(pool: &SqlitePool, prefix: &str) -> Result<Option<ApiKey>> {
    let row = sqlx::query("SELECT * FROM api_keys WHERE prefix = ? AND is_revoked = 0 LIMIT 1")
        .bind(prefix)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    match row {
        Some(row) => Ok(Some(api_key_from_row(&row)?)),
        None => Ok(None),
    }
}

pub(crate) async fn find_api_key_by_hash(pool: &SqlitePool, key_hash: &str) -> Result<Option<ApiKey>> {
    let row = sqlx::query("SELECT * FROM api_keys WHERE key_hash = ? AND is_revoked = 0 LIMIT 1")
        .bind(key_hash)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    match row {
        Some(row) => Ok(Some(api_key_from_row(&row)?)),
        None => Ok(None),
    }
}

pub(crate) async fn find_api_keys_by_workspace(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<ApiKey>> {
    let sql = "SELECT * FROM api_keys WHERE workspace_id = ? AND is_revoked = 0 ORDER BY created_at DESC";

    let mut rows = sqlx::query(sql)
        .bind(workspace_id)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    Ok(rows
        .drain(..)
        .map(|row| ApiKey {
            id: row.try_get("id").unwrap_or_default(),
            workspace_id: row.try_get("workspace_id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            key_hash: row.try_get("key_hash").unwrap_or_default(),
            prefix: row.try_get("prefix").unwrap_or_default(),
            permissions: row.try_get("permissions").unwrap_or_default(),
            rate_limit: row.try_get("rate_limit").unwrap_or_default(),
            is_enabled: row.try_get::<i32, _>("is_enabled").unwrap_or_default() != 0,
            is_revoked: row.try_get::<i32, _>("is_revoked").unwrap_or_default() != 0,
            last_used_at: row.try_get("last_used_at").ok(),
            last_used_ip: row.try_get("last_used_ip").ok(),
            request_count: row.try_get("request_count").unwrap_or_default(),
            expires_at: row.try_get("expires_at").ok(),
            created_at: row.try_get("created_at").unwrap_or_default(),
            updated_at: row.try_get("updated_at").unwrap_or_default(),
        })
        .collect())
}

pub(crate) async fn revoke_api_key(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("UPDATE api_keys SET is_revoked = 1, updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(())
}

pub(crate) async fn enable_api_key(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("UPDATE api_keys SET is_enabled = 1, updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(())
}

pub(crate) async fn disable_api_key(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query("UPDATE api_keys SET is_enabled = 0, updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(())
}

pub(crate) async fn record_api_usage(
    pool: &SqlitePool,
    workspace_id: &str,
    api_key_id: Option<&str>,
    method: &str,
    path: &str,
    status_code: i32,
    latency_ms: i32,
    ip_address: Option<&str>,
) -> Result<()> {
    let tenant_id: Option<String> = sqlx::query_scalar("SELECT tenant_id FROM workspaces WHERE id = ? LIMIT 1")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

    let tenant_id = tenant_id.unwrap_or_default();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"
        INSERT INTO api_usage (id, tenant_id, api_key_id, method, path, status_code, latency_ms, ip_address, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&tenant_id)
    .bind(api_key_id.unwrap_or(""))
    .bind(method)
    .bind(path)
    .bind(status_code)
    .bind(latency_ms)
    .bind(ip_address.unwrap_or(""))
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    if let Some(key_id) = api_key_id {
        sqlx::query(
            r#"
            UPDATE api_keys SET
                last_used_at = ?,
                last_used_ip = ?,
                request_count = request_count + 1
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(ip_address.unwrap_or(""))
        .bind(key_id)
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    }

    let error_count = if status_code >= 400 { 1 } else { 0 };
    sqlx::query(
        r#"
        INSERT INTO tenant_usage (id, tenant_id, api_call_count, total_api_calls, total_api_errors, updated_at)
        VALUES (?, ?, 1, 1, ?, ?)
        ON CONFLICT(tenant_id) DO UPDATE SET
            api_call_count = api_call_count + 1,
            total_api_calls = total_api_calls + 1,
            total_api_errors = total_api_errors + ?,
            updated_at = ?
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&tenant_id)
    .bind(error_count)
    .bind(&now)
    .bind(error_count)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    Ok(())
}

pub(crate) async fn get_api_usage_stats(pool: &SqlitePool, tenant_id: &str, days: i32) -> Result<ApiUsageStats> {
    let cutoff_date = (chrono::Utc::now() - chrono::Duration::days(days as i64))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) as total_calls,
            SUM(CASE WHEN status_code < 400 THEN 1 ELSE 0 END) as success_calls,
            SUM(CASE WHEN status_code >= 400 THEN 1 ELSE 0 END) as error_calls,
            COALESCE(AVG(latency_ms), 0) as avg_latency,
            MIN(created_at) as period_start,
            MAX(created_at) as period_end
        FROM api_usage
        WHERE tenant_id = ?
        AND created_at >= ?
        "#,
    )
    .bind(tenant_id)
    .bind(&cutoff_date)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::DatabaseError(e.to_string()))?;

    if let Some(row) = row {
        Ok(ApiUsageStats {
            total_calls: row.try_get::<i64, _>("total_calls")?,
            success_calls: row.try_get::<i64, _>("success_calls")?,
            error_calls: row.try_get::<i64, _>("error_calls")?,
            avg_latency_ms: row.try_get::<f64, _>("avg_latency")?,
            period_start: row.try_get("period_start")?,
            period_end: row.try_get("period_end")?,
        })
    } else {
        Ok(ApiUsageStats {
            total_calls: 0,
            success_calls: 0,
            error_calls: 0,
            avg_latency_ms: 0.0,
            period_start: String::new(),
            period_end: String::new(),
        })
    }
}

// ──────────────────────────────────────────────
// Db 委托（Tenant 领域）
// ──────────────────────────────────────────────

impl Db {
    /// 列出全部订阅计划（按 sort_order 升序）。
    pub async fn list_subscription_plans(&self) -> Result<Vec<SubscriptionPlan>> {
        list_subscription_plans(self.pool()).await
    }

    /// 按 ID 查询订阅计划。
    pub async fn find_subscription_plan_by_id(&self, id: &str) -> Result<Option<SubscriptionPlan>> {
        find_subscription_plan_by_id(self.pool(), id).await
    }

    /// 创建租户（附带初始化 tenant_usage 行）。
    pub async fn create_tenant(&self, req: &CreateTenantRequest) -> Result<Tenant> {
        create_tenant(self.pool(), req).await
    }

    /// 按 ID 查询租户。
    pub async fn find_tenant_by_id(&self, id: &str) -> Result<Option<Tenant>> {
        find_tenant_by_id(self.pool(), id).await
    }

    /// 按 slug 查询租户。
    pub async fn find_tenant_by_slug(&self, slug: &str) -> Result<Option<Tenant>> {
        find_tenant_by_slug(self.pool(), slug).await
    }

    /// 查询用户所属租户（tenant_users 关联，取第一个）。
    pub async fn find_tenant_by_user_id(&self, user_id: &str) -> Result<Option<Tenant>> {
        find_tenant_by_user_id(self.pool(), user_id).await
    }

    /// 查询用户所属 tenant_id（tenant_users 关联）。
    pub async fn find_tenant_id_by_user_id(&self, user_id: &str) -> Result<Option<String>> {
        find_tenant_id_by_user_id(self.pool(), user_id).await
    }

    /// 添加租户成员（tenant_users 行）。
    pub async fn insert_tenant_user(
        &self,
        tenant_id: &str,
        user_id: &str,
        role: &str,
        invitation_status: &str,
    ) -> Result<()> {
        insert_tenant_user(self.pool(), tenant_id, user_id, role, invitation_status).await
    }

    /// 查询租户用量。
    pub async fn get_tenant_usage(&self, tenant_id: &str) -> Result<Option<TenantUsage>> {
        get_tenant_usage(self.pool(), tenant_id).await
    }

    /// 变更租户订阅计划（subscription_status 置 active、清 trial）。
    pub async fn change_tenant_plan(&self, tenant_id: &str, plan_id: &str) -> Result<Tenant> {
        change_tenant_plan(self.pool(), tenant_id, plan_id).await
    }

    /// 挂起租户。
    pub async fn suspend_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        suspend_tenant(self.pool(), tenant_id).await
    }

    /// 激活租户。
    pub async fn activate_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        activate_tenant(self.pool(), tenant_id).await
    }

    /// 创建 API Key（返回行与明文 key，明文仅此一次可得）。
    pub async fn create_api_key(&self, workspace_id: &str, req: &CreateApiKeyRequest) -> Result<(ApiKey, String)> {
        create_api_key(self.pool(), workspace_id, req).await
    }

    /// 按 ID 查询 API Key。
    pub async fn find_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>> {
        find_api_key_by_id(self.pool(), id).await
    }

    /// 按前缀查询未吊销 API Key。
    pub async fn find_api_key_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>> {
        find_api_key_by_prefix(self.pool(), prefix).await
    }

    /// 按哈希查询未吊销 API Key。
    pub async fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>> {
        find_api_key_by_hash(self.pool(), key_hash).await
    }

    /// 列出工作空间下未吊销 API Key（按创建时间倒序）。
    pub async fn find_api_keys_by_workspace(&self, workspace_id: &str) -> Result<Vec<ApiKey>> {
        find_api_keys_by_workspace(self.pool(), workspace_id).await
    }

    /// 吊销 API Key。
    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        revoke_api_key(self.pool(), id).await
    }

    /// 启用 API Key。
    pub async fn enable_api_key(&self, id: &str) -> Result<()> {
        enable_api_key(self.pool(), id).await
    }

    /// 禁用 API Key。
    pub async fn disable_api_key(&self, id: &str) -> Result<()> {
        disable_api_key(self.pool(), id).await
    }

    /// 记录一次 API 调用（api_usage 行 + key 使用统计 + tenant_usage 累计）。
    pub async fn record_api_usage(
        &self,
        workspace_id: &str,
        api_key_id: Option<&str>,
        method: &str,
        path: &str,
        status_code: i32,
        latency_ms: i32,
        ip_address: Option<&str>,
    ) -> Result<()> {
        record_api_usage(
            self.pool(),
            workspace_id,
            api_key_id,
            method,
            path,
            status_code,
            latency_ms,
            ip_address,
        )
        .await
    }

    /// 统计租户近 `days` 天 API 调用。
    pub async fn get_api_usage_stats(&self, tenant_id: &str, days: i32) -> Result<ApiUsageStats> {
        get_api_usage_stats(self.pool(), tenant_id, days).await
    }
}

// ──────────────────────────────────────────────
// 初始化引导查询（自 cloud shared/initialization.rs 迁入，Task 12）
// ──────────────────────────────────────────────

/// 用户是否已关联任一租户。
pub(crate) async fn tenant_user_exists(pool: &SqlitePool, user_id: &str) -> std::result::Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tenant_users WHERE user_id = ?)")
        .bind(user_id)
        .fetch_one(pool)
        .await
}

/// 默认租户是否存在。
pub(crate) async fn default_tenant_exists(pool: &SqlitePool) -> std::result::Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = 'tenant-default-001')")
        .fetch_one(pool)
        .await
}

/// 创建默认租户（字面量行，幂等由调用方保证）。
pub(crate) async fn insert_default_tenant(pool: &SqlitePool) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO tenants
                   (id, name, slug, status, plan_id, subscription_status,
                    billing_email, timezone, locale, created_at, updated_at)
                   VALUES
                   ('tenant-default-001', 'Default Organization', 'default', 'active',
                    'plan_free', 'active', 'admin@tinyiothub.local', 'UTC', 'zh-CN',
                    datetime('now'), datetime('now'))"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// 关联用户到默认租户（INSERT OR IGNORE）。
pub(crate) async fn insert_default_tenant_user(
    pool: &SqlitePool,
    tenant_user_id: &str,
    user_id: &str,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO tenant_users
               (id, tenant_id, user_id, role, invitation_status, joined_at, created_at, updated_at)
               VALUES (?, 'tenant-default-001', ?, 'owner', 'accepted',
                       datetime('now'), datetime('now'), datetime('now'))"#,
    )
    .bind(tenant_user_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    /// 用户是否已关联任一租户。
    pub async fn tenant_user_exists(&self, user_id: &str) -> std::result::Result<bool, sqlx::Error> {
        tenant_user_exists(self.pool(), user_id).await
    }

    /// 默认租户是否存在。
    pub async fn default_tenant_exists(&self) -> std::result::Result<bool, sqlx::Error> {
        default_tenant_exists(self.pool()).await
    }

    /// 创建默认租户（字面量行）。
    pub async fn insert_default_tenant(&self) -> std::result::Result<(), sqlx::Error> {
        insert_default_tenant(self.pool()).await
    }

    /// 关联用户到默认租户（INSERT OR IGNORE）。
    pub async fn insert_default_tenant_user(
        &self,
        tenant_user_id: &str,
        user_id: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        insert_default_tenant_user(self.pool(), tenant_user_id, user_id).await
    }
}

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::dto::entity::workspace::Workspace;
use crate::infrastructure::persistence::database::Database;

/// 订阅计划
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

impl SubscriptionPlan {
    /// 获取所有计划
    pub async fn find_all(db: &Database) -> Result<Vec<SubscriptionPlan>, sqlx::Error> {
        let sql = "SELECT * FROM subscription_plans ORDER BY sort_order ASC";

        db.query(sql, |row| {
            Ok(SubscriptionPlan {
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
            })
        })
        .await
    }

    /// 根据 ID 获取
    pub async fn find_by_id(
        db: &Database,
        id: &str,
    ) -> Result<Option<SubscriptionPlan>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM subscription_plans WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(db.pool())
            .await?;

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
}

/// 租户
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

/// 租户查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TenantQueryParams {
    pub status: Option<String>,
    pub plan_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建租户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
}

/// 更新租户请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub billing_email: Option<String>,
    pub billing_contact: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub custom_logo: Option<String>,
    pub custom_theme: Option<String>,
}

/// 租户使用量
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

impl Tenant {
    /// 创建租户
    pub async fn create(db: &Database, req: &CreateTenantRequest) -> Result<Tenant, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 试用 14 天
        let trial_expires = (chrono::Utc::now() + chrono::Duration::days(14))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // 使用参数化查询防止 SQL 注入
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
        .execute(db.pool())
        .await?;

        // 初始化使用量记录
        sqlx::query(
            r#"
            INSERT INTO tenant_usage (id, tenant_id, device_count, api_call_count, storage_used_bytes, user_count, total_api_calls, total_api_errors, updated_at)
            VALUES (?, ?, 0, 0, 0, 1, 0, 0, ?)
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&id)
        .bind(&now)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 根据 ID 获取
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Tenant>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM tenants WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(db.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(Tenant {
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
            }))
        } else {
            Ok(None)
        }
    }

    /// 根据 slug 获取
    pub async fn find_by_slug(db: &Database, slug: &str) -> Result<Option<Tenant>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM tenants WHERE slug = ? LIMIT 1")
            .bind(slug)
            .fetch_optional(db.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(Tenant {
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
            }))
        } else {
            Ok(None)
        }
    }

    /// 获取租户使用量
    pub async fn get_usage(
        db: &Database,
        tenant_id: &str,
    ) -> Result<Option<TenantUsage>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM tenant_usage WHERE tenant_id = ? LIMIT 1")
            .bind(tenant_id)
            .fetch_optional(db.pool())
            .await?;

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

    /// 检查配额
    pub async fn check_quota(
        db: &Database,
        tenant_id: &str,
        resource: &str,
    ) -> Result<bool, sqlx::Error> {
        let tenant = Self::find_by_id(db, tenant_id).await?.ok_or(sqlx::Error::RowNotFound)?;
        let plan = SubscriptionPlan::find_by_id(db, &tenant.plan_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let usage = Self::get_usage(db, tenant_id).await?.ok_or(sqlx::Error::RowNotFound)?;

        match resource {
            "device" => {
                if plan.device_limit == 0 {
                    return Ok(true);
                }
                Ok(usage.device_count < plan.device_limit)
            }
            "api_call" => {
                if plan.api_call_limit == 0 {
                    return Ok(true);
                }
                Ok(usage.api_call_count < plan.api_call_limit)
            }
            "user" => {
                if plan.user_limit == 0 {
                    return Ok(true);
                }
                Ok(usage.user_count < plan.user_limit)
            }
            _ => Ok(false),
        }
    }

    /// 更新订阅计划
    pub async fn change_plan(
        db: &Database,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<Tenant, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
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
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, tenant_id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 暂停租户
    pub async fn suspend(db: &Database, tenant_id: &str) -> Result<Tenant, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
        sqlx::query(
            "UPDATE tenants SET status = 'suspended', updated_at = ? WHERE id = ?"
        )
        .bind(&now)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, tenant_id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 激活租户
    pub async fn activate(db: &Database, tenant_id: &str) -> Result<Tenant, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
        sqlx::query(
            "UPDATE tenants SET status = 'active', updated_at = ? WHERE id = ?"
        )
        .bind(&now)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;

        Self::find_by_id(db, tenant_id).await?.ok_or(sqlx::Error::RowNotFound)
    }
}

/// API Key 实体
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

/// 创建 API Key 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateApiKeyRequest {
    pub workspace_id: String,
    pub name: String,
    pub permissions: Option<Vec<String>>,
    pub rate_limit: Option<i32>,
    pub expires_in_days: Option<i32>,
}

/// API 使用统计
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

impl ApiKey {
    /// 创建 API Key（绑定到 workspace）
    pub async fn create(
        db: &Database,
        workspace_id: &str,
        req: &CreateApiKeyRequest,
    ) -> Result<(ApiKey, String), sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 生成随机密钥
        let raw_key = format!("sk_live_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let prefix = format!("sk_live_{}", &raw_key[8..16]);

        // 计算 hash
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };
        let mut hasher = DefaultHasher::new();
        raw_key.hash(&mut hasher);
        let key_hash = format!("{:x}", hasher.finish());

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

        // 使用参数化查询防止 SQL 注入
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
        .execute(db.pool())
        .await?;

        let key = Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)?;

        Ok((key, raw_key))
    }

    /// 根据 ID 获取
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<ApiKey>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(db.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(ApiKey {
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
            }))
        } else {
            Ok(None)
        }
    }

    /// 根据 prefix 获取
    pub async fn find_by_prefix(
        db: &Database,
        prefix: &str,
    ) -> Result<Option<ApiKey>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE prefix = ? AND is_revoked = 0 LIMIT 1")
            .bind(prefix)
            .fetch_optional(db.pool())
            .await?;

        if let Some(row) = row {
            Ok(Some(ApiKey {
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
            }))
        } else {
            Ok(None)
        }
    }

    /// 获取 workspace 的所有 Key
    pub async fn find_by_workspace(
        db: &Database,
        workspace_id: &str,
    ) -> Result<Vec<ApiKey>, sqlx::Error> {
        let sql = "SELECT * FROM api_keys WHERE workspace_id = ? AND is_revoked = 0 ORDER BY created_at DESC";

        let mut rows = sqlx::query(sql)
            .bind(workspace_id)
            .fetch_all(db.pool())
            .await?;

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

    /// 禁用 Key
    pub async fn revoke(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
        sqlx::query("UPDATE api_keys SET is_revoked = 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// 启用 Key
    pub async fn enable(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
        sqlx::query("UPDATE api_keys SET is_enabled = 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// 禁用 Key
    pub async fn disable(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 使用参数化查询防止 SQL 注入
        sqlx::query("UPDATE api_keys SET is_enabled = 0, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(db.pool())
            .await?;
        Ok(())
    }

    /// 记录 API 使用（基于 workspace_id）
    pub async fn record_usage(
        db: &Database,
        workspace_id: &str,
        api_key_id: Option<&str>,
        method: &str,
        path: &str,
        status_code: i32,
        latency_ms: i32,
        ip_address: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // Look up tenant_id from workspace (for analytics tables that still use tenant_id)
        let tenant_id = Workspace::find_by_id(db, workspace_id)
            .await
            .ok()
            .flatten()
            .map(|w| w.tenant_id)
            .unwrap_or_default();

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
        .execute(db.pool())
        .await?;

        // 更新 Key 使用统计
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
            .execute(db.pool())
            .await?;
        }

        // 更新租户使用统计（使用从 workspace 解析的 tenant_id）
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
        .execute(db.pool())
        .await;

        Ok(())
    }

    /// 获取使用统计
    pub async fn get_usage_stats(
        db: &Database,
        tenant_id: &str,
        days: i32,
    ) -> Result<ApiUsageStats, sqlx::Error> {
        // 计算日期范围，避免在 SQL 中直接拼接天数
        let cutoff_date = (chrono::Utc::now() - chrono::Duration::days(days as i64))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // 使用参数化查询防止 SQL 注入
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
        .fetch_optional(db.pool())
        .await?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_plan_fields() {
        let plan = SubscriptionPlan {
            id: "plan_basic".to_string(),
            name: "basic".to_string(),
            display_name: "基础版".to_string(),
            description: Some("适合小型项目".to_string()),
            device_limit: 100,
            api_call_limit: 10000,
            storage_mb: 1024,
            user_limit: 5,
            price_monthly: 99.0,
            price_yearly: 990.0,
            features: r#"{"webhook": true}"#.to_string(),
            sort_order: 2,
            created_at: "2026-03-12 00:00:00".to_string(),
            updated_at: "2026-03-12 00:00:00".to_string(),
        };

        assert_eq!(plan.device_limit, 100);
        assert_eq!(plan.price_monthly, 99.0);
    }

    #[test]
    fn test_tenant_fields() {
        let tenant = Tenant {
            id: "tenant-001".to_string(),
            name: "Test Company".to_string(),
            slug: "test-company".to_string(),
            status: "active".to_string(),
            plan_id: "plan_pro".to_string(),
            subscription_status: "active".to_string(),
            trial_expires_at: Some("2026-03-26 00:00:00".to_string()),
            billing_email: Some("billing@test.com".to_string()),
            billing_contact: Some("John Doe".to_string()),
            timezone: "Asia/Shanghai".to_string(),
            locale: "zh-CN".to_string(),
            custom_logo: None,
            custom_theme: None,
            created_at: "2026-03-12 00:00:00".to_string(),
            updated_at: "2026-03-12 00:00:00".to_string(),
        };

        assert_eq!(tenant.slug, "test-company");
        assert_eq!(tenant.status, "active");
    }

    #[test]
    fn test_create_tenant_request() {
        let req = CreateTenantRequest {
            name: "New Company".to_string(),
            slug: "new-company".to_string(),
            billing_email: Some("admin@newcompany.com".to_string()),
            billing_contact: Some("Jane Doe".to_string()),
            timezone: Some("America/New_York".to_string()),
            locale: Some("en-US".to_string()),
        };

        assert_eq!(req.name, "New Company");
        assert_eq!(req.slug, "new-company");
    }

    #[test]
    fn test_api_key_fields() {
        let key = ApiKey {
            id: "key-001".to_string(),
            workspace_id: "ws-001".to_string(),
            name: "Production API".to_string(),
            key_hash: "abc123".to_string(),
            prefix: "sk_live_abc".to_string(),
            permissions: r#"["read", "write"]"#.to_string(),
            rate_limit: 60,
            is_enabled: true,
            is_revoked: false,
            last_used_at: Some("2026-03-12 10:00:00".to_string()),
            last_used_ip: Some("192.168.1.1".to_string()),
            request_count: 1000,
            expires_at: None,
            created_at: "2026-03-12 00:00:00".to_string(),
            updated_at: "2026-03-12 00:00:00".to_string(),
        };

        assert_eq!(key.rate_limit, 60);
        assert!(key.is_enabled);
    }

    #[test]
    fn test_create_api_key_request() {
        let req = CreateApiKeyRequest {
            workspace_id: "ws-001".to_string(),
            name: "Test Key".to_string(),
            permissions: Some(vec!["read".to_string()]),
            rate_limit: Some(100),
            expires_in_days: Some(90),
        };

        assert_eq!(req.name, "Test Key");
        assert_eq!(req.rate_limit, Some(100));
    }

    #[test]
    fn test_tenant_usage_fields() {
        let usage = TenantUsage {
            id: "usage-001".to_string(),
            tenant_id: "tenant-001".to_string(),
            device_count: 50,
            api_call_count: 5000,
            api_call_reset_at: Some("2026-04-01 00:00:00".to_string()),
            storage_used_bytes: 1024000000,
            user_count: 5,
            total_api_calls: 50000,
            total_api_errors: 100,
            updated_at: "2026-03-12 12:00:00".to_string(),
        };

        assert_eq!(usage.device_count, 50);
        assert_eq!(usage.api_call_count, 5000);
    }

    #[test]
    fn test_api_usage_stats() {
        let stats = ApiUsageStats {
            total_calls: 10000,
            success_calls: 9900,
            error_calls: 100,
            avg_latency_ms: 45.5,
            period_start: "2026-03-01 00:00:00".to_string(),
            period_end: "2026-03-12 23:59:59".to_string(),
        };

        assert_eq!(stats.total_calls, 10000);
        assert!(stats.avg_latency_ms > 0.0);
    }

    #[test]
    fn test_plan_features_json() {
        let features = r#"{
            "webhook": true,
            "sms": true,
            "email": false,
            "api_access": true,
            "custom_brand": false
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(features).unwrap();
        assert!(parsed["webhook"].as_bool().unwrap_or(false));
        assert!(parsed["sms"].as_bool().unwrap_or(false));
    }
}

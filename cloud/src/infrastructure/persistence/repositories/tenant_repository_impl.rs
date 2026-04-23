use async_trait::async_trait;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::domain::tenant::repository::TenantRepository;
use crate::dto::entity::tenant::{
    ApiKey, ApiUsageStats, CreateApiKeyRequest, CreateTenantRequest, SubscriptionPlan, Tenant,
    TenantUsage,
};
use tinyiothub_storage::sqlite::Database;
use tinyiothub_core::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct SqliteTenantRepository {
    database: Database,
}

impl SqliteTenantRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

fn generate_secure_key() -> String {
    let mut bytes = [0u8; 36];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    bytes
        .iter()
        .map(|b| CHARS[(b % 62) as usize] as char)
        .collect()
}

#[async_trait]
impl TenantRepository for SqliteTenantRepository {
    async fn find_all_plans(&self) -> Result<Vec<SubscriptionPlan>> {
        let sql = "SELECT * FROM subscription_plans ORDER BY sort_order ASC";

        let plans = self
            .database
            .query(sql, |row| {
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
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(plans)
    }

    async fn find_plan_by_id(&self, id: &str) -> Result<Option<SubscriptionPlan>> {
        let row = sqlx::query("SELECT * FROM subscription_plans WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(self.database.pool())
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

    async fn create_tenant(&self, req: &CreateTenantRequest) -> Result<Tenant> {
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
        .execute(self.database.pool())
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
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        self.find_tenant_by_id(&id)
            .await?
            .ok_or(Error::NotFound)
    }

    async fn find_tenant_by_id(&self, id: &str) -> Result<Option<Tenant>> {
        let row = sqlx::query("SELECT * FROM tenants WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

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

    async fn find_tenant_by_slug(&self, slug: &str) -> Result<Option<Tenant>> {
        let row = sqlx::query("SELECT * FROM tenants WHERE slug = ? LIMIT 1")
            .bind(slug)
            .fetch_optional(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

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

    async fn get_tenant_usage(&self, tenant_id: &str) -> Result<Option<TenantUsage>> {
        let row = sqlx::query("SELECT * FROM tenant_usage WHERE tenant_id = ? LIMIT 1")
            .bind(tenant_id)
            .fetch_optional(self.database.pool())
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

    async fn change_plan(&self, tenant_id: &str, plan_id: &str) -> Result<Tenant> {
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
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        self.find_tenant_by_id(tenant_id)
            .await?
            .ok_or(Error::NotFound)
    }

    async fn suspend_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "UPDATE tenants SET status = 'suspended', updated_at = ? WHERE id = ?"
        )
        .bind(&now)
        .bind(tenant_id)
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        self.find_tenant_by_id(tenant_id)
            .await?
            .ok_or(Error::NotFound)
    }

    async fn activate_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "UPDATE tenants SET status = 'active', updated_at = ? WHERE id = ?"
        )
        .bind(&now)
        .bind(tenant_id)
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        self.find_tenant_by_id(tenant_id)
            .await?
            .ok_or(Error::NotFound)
    }

    async fn create_api_key(
        &self,
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
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        let key = self
            .find_api_key_by_id(&id)
            .await?
            .ok_or(Error::NotFound)?;

        Ok((key, raw_key))
    }

    async fn find_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

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

    async fn find_api_key_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>> {
        let row = sqlx::query(
            "SELECT * FROM api_keys WHERE prefix = ? AND is_revoked = 0 LIMIT 1"
        )
        .bind(prefix)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

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

    async fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>> {
        let row = sqlx::query(
            "SELECT * FROM api_keys WHERE key_hash = ? AND is_revoked = 0 LIMIT 1"
        )
        .bind(key_hash)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

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

    async fn find_api_keys_by_workspace(&self, workspace_id: &str) -> Result<Vec<ApiKey>> {
        let sql = "SELECT * FROM api_keys WHERE workspace_id = ? AND is_revoked = 0 ORDER BY created_at DESC";

        let mut rows = sqlx::query(sql)
            .bind(workspace_id)
            .fetch_all(self.database.pool())
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

    async fn revoke_api_key(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE api_keys SET is_revoked = 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn enable_api_key(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE api_keys SET is_enabled = 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn disable_api_key(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE api_keys SET is_enabled = 0, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn record_api_usage(
        &self,
        workspace_id: &str,
        api_key_id: Option<&str>,
        method: &str,
        path: &str,
        status_code: i32,
        latency_ms: i32,
        ip_address: Option<&str>,
    ) -> Result<()> {
        // Look up tenant_id from workspace
        let tenant_id: Option<String> = sqlx::query_scalar(
            "SELECT tenant_id FROM workspaces WHERE id = ? LIMIT 1"
        )
        .bind(workspace_id)
        .fetch_optional(self.database.pool())
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
        .execute(self.database.pool())
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
            .execute(self.database.pool())
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
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn get_api_usage_stats(&self, tenant_id: &str, days: i32) -> Result<ApiUsageStats> {
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
        .fetch_optional(self.database.pool())
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
}

//! system_settings 键值存储（自 cloud event/security 迁入，Task 12）。

use sqlx::SqlitePool;

use crate::database::Db;

/// 读取事件安全配置 JSON（system_settings.key = 'event_security_config'）。
pub(crate) async fn get_event_security_config_json(pool: &SqlitePool) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT value FROM system_settings WHERE key = 'event_security_config'")
        .fetch_optional(pool)
        .await
}

/// 保存事件安全配置 JSON（upsert）。
pub(crate) async fn save_event_security_config_json(pool: &SqlitePool, json: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO system_settings (key, value, updated_at) VALUES ('event_security_config', ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(json)
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    /// 读取事件安全配置 JSON。
    pub async fn get_event_security_config_json(&self) -> Result<Option<String>, sqlx::Error> {
        get_event_security_config_json(self.pool()).await
    }

    /// 保存事件安全配置 JSON（upsert）。
    pub async fn save_event_security_config_json(&self, json: &str) -> Result<(), sqlx::Error> {
        save_event_security_config_json(self.pool(), json).await
    }
}

//! Alarm repository traits and SQLite implementations.

use async_trait::async_trait;
use sqlx::SqlitePool;

use super::types::{Alarm, AlarmRule};

#[async_trait]
pub trait AlarmRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Alarm>, sqlx::Error>;
    async fn insert(&self, alarm: &Alarm) -> Result<(), sqlx::Error>;
    async fn mark_resolved(&self, id: &str) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait AlarmRuleRepository: Send + Sync {
    async fn list_enabled(&self, workspace_id: &str) -> Result<Vec<AlarmRule>, sqlx::Error>;
}

/// SQLite AlarmRepository.
/// Trait is defined here; impl lives in tinyiothub-ai for use by cloud/ during migration.
pub struct SqliteAlarmRepository {
    pool: SqlitePool,
}

impl SqliteAlarmRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AlarmRepository for SqliteAlarmRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Alarm>, sqlx::Error> {
        sqlx::query_as::<_, Alarm>(
            "SELECT id, workspace_id, device_id, alarm_type, severity, message, rule_id, resolved, created_at
             FROM device_alarms WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn insert(&self, _alarm: &Alarm) -> Result<(), sqlx::Error> {
        // Delegated to existing cloud/ alarm service during migration
        Ok(())
    }

    async fn mark_resolved(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE device_alarms SET resolved = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl AlarmRuleRepository for SqliteAlarmRepository {
    async fn list_enabled(&self, _workspace_id: &str) -> Result<Vec<AlarmRule>, sqlx::Error> {
        Ok(vec![])
    }
}

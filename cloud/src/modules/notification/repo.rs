// Notification repository implementations
// Consolidated from infrastructure/persistence/repositories/notification_*_repository_impl.rs

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::{debug, error, info};

use super::types::{
    HistoryStatistics, NotificationChannelType, NotificationRecord, NotificationRule,
    NotificationStatus, RuleStatistics,
};
use crate::modules::event::{EventError, Result};
use crate::shared::persistence::Database;

// ──────────────────────────────────────────────
// Notification Rule Repository
// ──────────────────────────────────────────────

/// Notification rule store trait
#[async_trait]
pub trait NotificationRuleStore: Send + Sync {
    async fn create_rule(&self, rule: &NotificationRule) -> Result<()>;
    async fn get_rule(&self, rule_id: &str) -> Result<Option<NotificationRule>>;
    async fn update_rule(&self, rule: &NotificationRule) -> Result<()>;
    async fn delete_rule(&self, rule_id: &str) -> Result<()>;
    async fn list_rules(&self) -> Result<Vec<NotificationRule>>;
    async fn get_active_rules(&self) -> Result<Vec<NotificationRule>>;
}

/// Repository trait for notification rules
#[async_trait]
pub trait NotificationRuleRepository: Send + Sync {
    async fn create_rule(&self, rule: &NotificationRule) -> Result<()>;
    async fn get_rule(&self, rule_id: &str) -> Result<Option<NotificationRule>>;
    async fn get_all_rules(&self) -> Result<Vec<NotificationRule>>;
    async fn get_enabled_rules(&self) -> Result<Vec<NotificationRule>>;
    async fn update_rule(&self, rule: &NotificationRule) -> Result<()>;
    async fn delete_rule(&self, rule_id: &str) -> Result<()>;
    async fn get_rules_by_event_type(
        &self,
        event_type: &str,
        event_subtype: Option<&str>,
    ) -> Result<Vec<NotificationRule>>;
}

/// SQLite implementation of notification rule repository
pub struct NotificationRuleRepositoryImpl {
    db: Arc<Database>,
}

impl NotificationRuleRepositoryImpl {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    fn parse_timestamp(timestamp_str: &str, field_name: &str) -> Result<DateTime<Utc>> {
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
            return Ok(dt.with_timezone(&Utc));
        }
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S") {
            return Ok(dt.and_utc());
        }
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S%.f") {
            return Ok(dt.and_utc());
        }
        Err(EventError::Validation {
            message: format!("Invalid {} timestamp: {}", field_name, timestamp_str),
        })
    }

    fn row_to_notification_rule(&self, row: &sqlx::sqlite::SqliteRow) -> Result<NotificationRule> {
        let notification_methods_str: String = row.try_get("notification_methods")?;
        let notification_methods: Vec<String> =
            serde_json::from_str(&notification_methods_str).map_err(EventError::Serialization)?;

        let notification_methods: Result<Vec<NotificationChannelType>> = notification_methods
            .into_iter()
            .map(|method| {
                NotificationChannelType::parse_str(&method).ok_or_else(|| EventError::Validation {
                    message: format!("Invalid notification method: {}", method),
                })
            })
            .collect();
        let notification_methods = notification_methods?;

        let recipients_str: String = row.try_get("recipients")?;
        let recipients: Vec<String> =
            serde_json::from_str(&recipients_str).map_err(EventError::Serialization)?;

        let device_filter_str: Option<String> = row.try_get("device_filter")?;
        let device_filter = if let Some(filter_str) = device_filter_str {
            Some(
                serde_json::from_str::<serde_json::Value>(&filter_str)
                    .map_err(EventError::Serialization)?,
            )
        } else {
            None
        };

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = Self::parse_timestamp(&created_at_str, "created_at")?;

        let updated_at_str: String = row.try_get("updated_at")?;
        let updated_at = Self::parse_timestamp(&updated_at_str, "updated_at")?;

        Ok(NotificationRule {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            event_type: row.try_get("event_type")?,
            event_subtype: row.try_get("event_subtype")?,
            event_level: row.try_get("event_level")?,
            device_filter,
            notification_methods: notification_methods.clone(),
            recipients,
            enabled: row.try_get::<bool, _>("enabled")?,
            created_at,
            updated_at,
            workspace_id: row.try_get("workspace_id").ok(),
            event_types: Vec::new(),
            event_levels: Vec::new(),
            channels: notification_methods,
            conditions: std::collections::HashMap::new(),
            is_active: row.try_get::<bool, _>("enabled")?,
        })
    }

    fn escape_like(s: &str) -> String {
        s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    }

    /// Get rules by notification method
    pub async fn get_rules_by_method(
        &self,
        method: NotificationChannelType,
    ) -> Result<Vec<NotificationRule>> {
        let pool = self.db.pool();
        let method_str = method.as_str();
        let escaped_method = Self::escape_like(method_str);
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, event_type, event_subtype, event_level,
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at, workspace_id
            FROM notification_rules
            WHERE enabled = 1 AND notification_methods LIKE ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(format!("%\"{}\"", escaped_method))
        .fetch_all(pool)
        .await?;

        let mut rules = Vec::new();
        for row in rows {
            match self.row_to_notification_rule(&row) {
                Ok(rule) => {
                    if rule.notification_methods.contains(&method) {
                        rules.push(rule);
                    }
                }
                Err(e) => {
                    error!("Failed to parse notification rule: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} notification rules for method: {:?}", rules.len(), method);
        Ok(rules)
    }

    /// Enable or disable a rule
    pub async fn set_rule_enabled(&self, rule_id: &str, enabled: bool) -> Result<()> {
        let pool = self.db.pool();
        let result =
            sqlx::query("UPDATE notification_rules SET enabled = ?, updated_at = ? WHERE id = ?")
                .bind(enabled)
                .bind(Utc::now().to_rfc3339())
                .bind(rule_id)
                .execute(pool)
                .await?;

        if result.rows_affected() == 0 {
            return Err(EventError::NotFound { id: rule_id.to_string() });
        }
        info!("Set notification rule {} enabled: {}", rule_id, enabled);
        Ok(())
    }

    /// Get rule statistics
    pub async fn get_rule_statistics(&self) -> Result<RuleStatistics> {
        let pool = self.db.pool();
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_rules,
                COUNT(CASE WHEN enabled = 1 THEN 1 END) as enabled_rules,
                COUNT(CASE WHEN enabled = 0 THEN 1 END) as disabled_rules
            FROM notification_rules
            "#,
        )
        .fetch_one(pool)
        .await?;

        let total_rules: i64 = row.try_get("total_rules")?;
        let enabled_rules: i64 = row.try_get("enabled_rules")?;
        let disabled_rules: i64 = row.try_get("disabled_rules")?;

        Ok(RuleStatistics {
            total_rules: total_rules as u64,
            enabled_rules: enabled_rules as u64,
            disabled_rules: disabled_rules as u64,
        })
    }
}

#[async_trait]
impl NotificationRuleRepository for NotificationRuleRepositoryImpl {
    async fn create_rule(&self, rule: &NotificationRule) -> Result<()> {
        let pool = self.db.pool();
        let notification_methods_json = serde_json::to_string(
            &rule.notification_methods.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
        )?;
        let recipients_json = serde_json::to_string(&rule.recipients)?;
        let device_filter_json = if let Some(ref filter) = rule.device_filter {
            Some(serde_json::to_string(filter)?)
        } else {
            None
        };

        sqlx::query(
            r#"
            INSERT INTO notification_rules (
                id, name, description, event_type, event_subtype, event_level,
                device_filter, notification_methods, recipients, enabled, created_at, updated_at, workspace_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(&rule.event_type)
        .bind(&rule.event_subtype)
        .bind(rule.event_level)
        .bind(device_filter_json)
        .bind(notification_methods_json)
        .bind(recipients_json)
        .bind(rule.enabled)
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.updated_at.to_rfc3339())
        .bind(&rule.workspace_id)
        .execute(pool)
        .await?;

        info!("Created notification rule: {} ({})", rule.name, rule.id);
        Ok(())
    }

    async fn get_rule(&self, rule_id: &str) -> Result<Option<NotificationRule>> {
        let pool = self.db.pool();
        let row = sqlx::query(
            r#"
            SELECT id, name, description, event_type, event_subtype, event_level,
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at, workspace_id
            FROM notification_rules
            WHERE id = ?
            "#,
        )
        .bind(rule_id)
        .fetch_optional(pool)
        .await?;

        if let Some(row) = row {
            let rule = self.row_to_notification_rule(&row)?;
            debug!("Retrieved notification rule: {}", rule_id);
            Ok(Some(rule))
        } else {
            Ok(None)
        }
    }

    async fn get_all_rules(&self) -> Result<Vec<NotificationRule>> {
        let pool = self.db.pool();
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, event_type, event_subtype, event_level,
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at, workspace_id
            FROM notification_rules
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?;

        let mut rules = Vec::new();
        for row in rows {
            match self.row_to_notification_rule(&row) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    error!("Failed to parse notification rule: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} notification rules", rules.len());
        Ok(rules)
    }

    async fn get_enabled_rules(&self) -> Result<Vec<NotificationRule>> {
        let pool = self.db.pool();
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, event_type, event_subtype, event_level,
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at, workspace_id
            FROM notification_rules
            WHERE enabled = 1
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?;

        let mut rules = Vec::new();
        for row in rows {
            match self.row_to_notification_rule(&row) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    error!("Failed to parse notification rule: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} enabled notification rules", rules.len());
        Ok(rules)
    }

    async fn update_rule(&self, rule: &NotificationRule) -> Result<()> {
        let pool = self.db.pool();
        let notification_methods_json = serde_json::to_string(
            &rule.notification_methods.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
        )?;
        let recipients_json = serde_json::to_string(&rule.recipients)?;
        let device_filter_json = if let Some(ref filter) = rule.device_filter {
            Some(serde_json::to_string(filter)?)
        } else {
            None
        };

        let result = sqlx::query(
            r#"
            UPDATE notification_rules
            SET name = ?, description = ?, event_type = ?, event_subtype = ?, event_level = ?,
                device_filter = ?, notification_methods = ?, recipients = ?, enabled = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(&rule.event_type)
        .bind(&rule.event_subtype)
        .bind(rule.event_level)
        .bind(device_filter_json)
        .bind(notification_methods_json)
        .bind(recipients_json)
        .bind(rule.enabled)
        .bind(Utc::now().to_rfc3339())
        .bind(&rule.id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(EventError::NotFound { id: rule.id.clone() });
        }
        info!("Updated notification rule: {} ({})", rule.name, rule.id);
        Ok(())
    }

    async fn delete_rule(&self, rule_id: &str) -> Result<()> {
        let pool = self.db.pool();
        let result = sqlx::query("DELETE FROM notification_rules WHERE id = ?")
            .bind(rule_id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(EventError::NotFound { id: rule_id.to_string() });
        }
        info!("Deleted notification rule: {}", rule_id);
        Ok(())
    }

    async fn get_rules_by_event_type(
        &self,
        event_type: &str,
        event_subtype: Option<&str>,
    ) -> Result<Vec<NotificationRule>> {
        let pool = self.db.pool();
        let rows = if let Some(subtype) = event_subtype {
            sqlx::query(
                r#"
                SELECT id, name, description, event_type, event_subtype, event_level,
                       device_filter, notification_methods, recipients, enabled, created_at, updated_at
                FROM notification_rules
                WHERE enabled = 1
                  AND (event_type IS NULL OR event_type = ?)
                  AND (event_subtype IS NULL OR event_subtype = ?)
                ORDER BY created_at DESC
                "#,
            )
            .bind(event_type)
            .bind(subtype)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, name, description, event_type, event_subtype, event_level,
                       device_filter, notification_methods, recipients, enabled, created_at, updated_at
                FROM notification_rules
                WHERE enabled = 1
                  AND (event_type IS NULL OR event_type = ?)
                ORDER BY created_at DESC
                "#,
            )
            .bind(event_type)
            .fetch_all(pool)
            .await?
        };

        let mut rules = Vec::new();
        for row in rows {
            match self.row_to_notification_rule(&row) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    error!("Failed to parse notification rule: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} notification rules for event type: {}", rules.len(), event_type);
        Ok(rules)
    }
}

// ──────────────────────────────────────────────
// Notification History Repository
// ──────────────────────────────────────────────

/// Notification history store trait
#[async_trait]
pub trait NotificationHistoryStore: Send + Sync {
    async fn store_record(&self, record: &NotificationRecord) -> Result<()>;
    async fn get_records(&self, event_id: &str) -> Result<Vec<NotificationRecord>>;
    async fn update_status(
        &self,
        record_id: &str,
        status: NotificationStatus,
        error_message: Option<String>,
    ) -> Result<()>;
}

/// SQLite implementation of notification history store
pub struct NotificationHistoryRepositoryImpl {
    db: Arc<Database>,
}

impl NotificationHistoryRepositoryImpl {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    fn row_to_notification_record(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<NotificationRecord> {
        let method_str: String = row.try_get("notification_method")?;
        let notification_method =
            NotificationChannelType::parse_str(&method_str).ok_or_else(|| {
                EventError::Validation {
                    message: format!("Invalid notification method: {}", method_str),
                }
            })?;

        let status_str: String = row.try_get("status")?;
        let status =
            NotificationStatus::parse_str(&status_str).ok_or_else(|| EventError::Validation {
                message: format!("Invalid notification status: {}", status_str),
            })?;

        let sent_at_str: Option<String> = row.try_get("sent_at")?;
        let sent_at = if let Some(sent_at_str) = sent_at_str {
            Some(
                DateTime::parse_from_rfc3339(&sent_at_str)
                    .map_err(|e| EventError::Validation {
                        message: format!("Invalid sent_at timestamp: {}", e),
                    })?
                    .with_timezone(&Utc),
            )
        } else {
            None
        };

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| EventError::Validation {
                message: format!("Invalid created_at timestamp: {}", e),
            })?
            .with_timezone(&Utc);

        Ok(NotificationRecord {
            id: row.try_get("id")?,
            event_id: row.try_get("event_id")?,
            rule_id: row.try_get("rule_id")?,
            notification_method,
            recipient: row.try_get("recipient")?,
            status,
            sent_at,
            error_message: row.try_get("error_message")?,
            created_at,
        })
    }

    /// Get notification records by rule ID
    pub async fn get_records_by_rule(&self, rule_id: &str) -> Result<Vec<NotificationRecord>> {
        let pool = self.db.pool();
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, rule_id, notification_method, recipient,
                   status, sent_at, error_message, created_at
            FROM notification_history
            WHERE rule_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(rule_id)
        .fetch_all(pool)
        .await?;

        let mut records = Vec::new();
        for row in rows {
            match self.row_to_notification_record(&row) {
                Ok(record) => records.push(record),
                Err(e) => {
                    error!("Failed to parse notification record: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} notification records for rule: {}", records.len(), rule_id);
        Ok(records)
    }

    /// Get notification records by status
    pub async fn get_records_by_status(
        &self,
        status: NotificationStatus,
    ) -> Result<Vec<NotificationRecord>> {
        let pool = self.db.pool();
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, rule_id, notification_method, recipient,
                   status, sent_at, error_message, created_at
            FROM notification_history
            WHERE status = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(status.as_str())
        .fetch_all(pool)
        .await?;

        let mut records = Vec::new();
        for row in rows {
            match self.row_to_notification_record(&row) {
                Ok(record) => records.push(record),
                Err(e) => {
                    error!("Failed to parse notification record: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} notification records with status: {}", records.len(), status);
        Ok(records)
    }

    /// Get notification statistics
    pub async fn get_statistics(&self, days: i32) -> Result<HistoryStatistics> {
        let pool = self.db.pool();
        let cutoff_date = (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();

        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_notifications,
                COUNT(CASE WHEN status = 'sent' THEN 1 END) as sent_count,
                COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_count,
                COUNT(CASE WHEN status = 'pending' THEN 1 END) as pending_count
            FROM notification_history
            WHERE created_at >= ?
            "#,
        )
        .bind(&cutoff_date)
        .fetch_one(pool)
        .await?;

        let total_notifications: i64 = row.try_get("total_notifications")?;
        let sent_count: i64 = row.try_get("sent_count")?;
        let failed_count: i64 = row.try_get("failed_count")?;
        let pending_count: i64 = row.try_get("pending_count")?;

        let success_rate = if total_notifications > 0 {
            (sent_count as f64 / total_notifications as f64) * 100.0
        } else {
            0.0
        };

        Ok(HistoryStatistics {
            total_notifications: total_notifications as u64,
            sent_count: sent_count as u64,
            failed_count: failed_count as u64,
            pending_count: pending_count as u64,
            success_rate,
            period_days: days,
        })
    }

    /// Clean up old notification records
    pub async fn cleanup_old_records(&self, days: i32) -> Result<u64> {
        let pool = self.db.pool();
        let cutoff_date = (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();

        let result = sqlx::query("DELETE FROM notification_history WHERE created_at < ?")
            .bind(&cutoff_date)
            .execute(pool)
            .await?;

        let deleted_count = result.rows_affected();
        if deleted_count > 0 {
            info!("Cleaned up {} old notification records older than {} days", deleted_count, days);
        }
        Ok(deleted_count)
    }

    /// Get notification records with pagination
    pub async fn get_records_paginated(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<NotificationRecord>, u64)> {
        let pool = self.db.pool();

        let count_row = sqlx::query("SELECT COUNT(*) as total FROM notification_history")
            .fetch_one(pool)
            .await?;
        let total_count: i64 = count_row.try_get("total")?;

        let rows = sqlx::query(
            r#"
            SELECT id, event_id, rule_id, notification_method, recipient,
                   status, sent_at, error_message, created_at
            FROM notification_history
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;

        let mut records = Vec::new();
        for row in rows {
            match self.row_to_notification_record(&row) {
                Ok(record) => records.push(record),
                Err(e) => {
                    error!("Failed to parse notification record: {}", e);
                    continue;
                }
            }
        }
        Ok((records, total_count as u64))
    }
}

#[async_trait]
impl NotificationHistoryStore for NotificationHistoryRepositoryImpl {
    async fn store_record(&self, record: &NotificationRecord) -> Result<()> {
        let pool = self.db.pool();
        let sent_at_str = record.sent_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            r#"
            INSERT INTO notification_history (
                id, event_id, rule_id, notification_method, recipient,
                status, sent_at, error_message, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.id)
        .bind(&record.event_id)
        .bind(&record.rule_id)
        .bind(record.notification_method.as_str())
        .bind(&record.recipient)
        .bind(record.status.as_str())
        .bind(sent_at_str)
        .bind(&record.error_message)
        .bind(record.created_at.to_rfc3339())
        .execute(pool)
        .await?;

        debug!("Stored notification record: {}", record.id);
        Ok(())
    }

    async fn get_records(&self, event_id: &str) -> Result<Vec<NotificationRecord>> {
        let pool = self.db.pool();
        let rows = sqlx::query(
            r#"
            SELECT id, event_id, rule_id, notification_method, recipient,
                   status, sent_at, error_message, created_at
            FROM notification_history
            WHERE event_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(event_id)
        .fetch_all(pool)
        .await?;

        let mut records = Vec::new();
        for row in rows {
            match self.row_to_notification_record(&row) {
                Ok(record) => records.push(record),
                Err(e) => {
                    error!("Failed to parse notification record: {}", e);
                    continue;
                }
            }
        }
        debug!("Retrieved {} notification records for event: {}", records.len(), event_id);
        Ok(records)
    }

    async fn update_status(
        &self,
        record_id: &str,
        status: NotificationStatus,
        error_message: Option<String>,
    ) -> Result<()> {
        let pool = self.db.pool();
        let sent_at =
            if status == NotificationStatus::Sent { Some(Utc::now().to_rfc3339()) } else { None };

        let result = sqlx::query(
            r#"
            UPDATE notification_history
            SET status = ?, sent_at = ?, error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(sent_at)
        .bind(&error_message)
        .bind(record_id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(EventError::NotFound { id: record_id.to_string() });
        }
        debug!("Updated notification record {} status to: {}", record_id, status);
        Ok(())
    }
}

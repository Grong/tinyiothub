use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::{debug, error, info};

use crate::{
    domain::event::{EventError, NotificationChannelType, NotificationRule, Result},
    infrastructure::persistence::Database,
};

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
    /// Create a new notification rule repository
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Parse timestamp from database - handles both RFC3339 and SQLite datetime formats
    fn parse_timestamp(timestamp_str: &str, field_name: &str) -> Result<DateTime<Utc>> {
        // Try RFC3339 first (ISO 8601 with timezone)
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
            return Ok(dt.with_timezone(&Utc));
        }
        // Fallback to SQLite datetime format (YYYY-MM-DD HH:MM:SS)
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S") {
            return Ok(dt.and_utc());
        }
        // Try with microseconds (SQLite datetime can include them)
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S%.f") {
            return Ok(dt.and_utc());
        }
        Err(EventError::Validation {
            message: format!("Invalid {} timestamp: {}", field_name, timestamp_str),
        })
    }

    /// Convert database row to NotificationRule
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

            // Initialize legacy compatibility fields
            event_types: Vec::new(),
            event_levels: Vec::new(),
            channels: notification_methods,
            conditions: std::collections::HashMap::new(),
            is_active: row.try_get::<bool, _>("enabled")?,
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
                device_filter, notification_methods, recipients, enabled, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at
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
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at
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
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at
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

impl NotificationRuleRepositoryImpl {
    /// Escape special characters in LIKE patterns to prevent SQL injection via LIKE wildcards
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
                   device_filter, notification_methods, recipients, enabled, created_at, updated_at
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
                    // Double-check that the rule actually contains the method
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

/// Notification rule statistics
#[derive(Debug, Clone)]
pub struct RuleStatistics {
    pub total_rules: u64,
    pub enabled_rules: u64,
    pub disabled_rules: u64,
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::infrastructure::persistence::database::Database;

    async fn create_test_db() -> Arc<Database> {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();
        crate::infrastructure::persistence::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();

        Arc::new(Database::new(pool))
    }

    #[tokio::test]
    async fn test_create_and_get_rule() {
        let db = create_test_db().await;
        let repo = NotificationRuleRepositoryImpl::new(db);

        let rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "Test Rule".to_string(),
            Some("Test description".to_string()),
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        );

        // Create the rule
        repo.create_rule(&rule).await.unwrap();

        // Retrieve the rule
        let retrieved_rule = repo.get_rule(&rule.id).await.unwrap();
        assert!(retrieved_rule.is_some());

        let retrieved_rule = retrieved_rule.unwrap();
        assert_eq!(retrieved_rule.id, rule.id);
        assert_eq!(retrieved_rule.name, rule.name);
        assert_eq!(retrieved_rule.description, rule.description);
        assert_eq!(retrieved_rule.notification_methods, rule.notification_methods);
        assert_eq!(retrieved_rule.recipients, rule.recipients);
    }

    #[tokio::test]
    async fn test_update_rule() {
        let db = create_test_db().await;
        let repo = NotificationRuleRepositoryImpl::new(db);

        let mut rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "Test Rule".to_string(),
            Some("Test description".to_string()),
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        );

        // Create the rule
        repo.create_rule(&rule).await.unwrap();

        // Update the rule
        rule.name = "Updated Rule".to_string();
        rule.notification_methods.push(NotificationChannelType::Sms);
        rule.recipients.push("+1234567890".to_string());

        repo.update_rule(&rule).await.unwrap();

        // Retrieve and verify
        let retrieved_rule = repo.get_rule(&rule.id).await.unwrap().unwrap();
        assert_eq!(retrieved_rule.name, "Updated Rule");
        assert_eq!(retrieved_rule.notification_methods.len(), 2);
        assert_eq!(retrieved_rule.recipients.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_rule() {
        let db = create_test_db().await;
        let repo = NotificationRuleRepositoryImpl::new(db);

        let rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "Test Rule".to_string(),
            None,
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        );

        // Create the rule
        repo.create_rule(&rule).await.unwrap();

        // Verify it exists
        assert!(repo.get_rule(&rule.id).await.unwrap().is_some());

        // Delete the rule
        repo.delete_rule(&rule.id).await.unwrap();

        // Verify it's gone
        assert!(repo.get_rule(&rule.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_enabled_rules() {
        let db = create_test_db().await;
        let repo = NotificationRuleRepositoryImpl::new(db);

        // Create enabled rule
        let enabled_rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "Enabled Rule".to_string(),
            None,
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        );
        repo.create_rule(&enabled_rule).await.unwrap();

        // Create disabled rule
        let disabled_rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "Disabled Rule".to_string(),
            None,
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        )
        .set_enabled(false);
        repo.create_rule(&disabled_rule).await.unwrap();

        // Get enabled rules
        let enabled_rules = repo.get_enabled_rules().await.unwrap();
        assert_eq!(enabled_rules.len(), 1);
        assert_eq!(enabled_rules[0].id, enabled_rule.id);

        // Get all rules
        let all_rules = repo.get_all_rules().await.unwrap();
        assert_eq!(all_rules.len(), 2);
    }

    #[tokio::test]
    async fn test_get_rules_by_event_type() {
        let db = create_test_db().await;
        let repo = NotificationRuleRepositoryImpl::new(db);

        // Create rule for system events
        let system_rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "System Rule".to_string(),
            None,
            vec![NotificationChannelType::Email],
            vec!["admin@example.com".to_string()],
        )
        .with_event_type("system".to_string());
        repo.create_rule(&system_rule).await.unwrap();

        // Create rule for device events
        let device_rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "Device Rule".to_string(),
            None,
            vec![NotificationChannelType::Sms],
            vec!["+1234567890".to_string()],
        )
        .with_event_type("device".to_string());
        repo.create_rule(&device_rule).await.unwrap();

        // Create general rule (no event type filter)
        let general_rule = NotificationRule::new(
            Uuid::new_v4().to_string(),
            "General Rule".to_string(),
            None,
            vec![NotificationChannelType::Email],
            vec!["all@example.com".to_string()],
        );
        repo.create_rule(&general_rule).await.unwrap();

        // Get rules for system events
        let system_rules = repo.get_rules_by_event_type("system", None).await.unwrap();
        assert_eq!(system_rules.len(), 2); // system_rule + general_rule

        // Get rules for device events
        let device_rules = repo.get_rules_by_event_type("device", None).await.unwrap();
        assert_eq!(device_rules.len(), 2); // device_rule + general_rule
    }

    #[tokio::test]
    async fn test_rule_statistics() {
        let db = create_test_db().await;

        // Clear any seed data from migrations to ensure test isolation
        let pool = db.pool();
        sqlx::query("DELETE FROM notification_rules").execute(pool).await.unwrap();

        let repo = NotificationRuleRepositoryImpl::new(db);

        // Create some rules
        for i in 0..5 {
            let enabled = i < 3; // First 3 enabled, last 2 disabled
            let rule = NotificationRule::new(
                Uuid::new_v4().to_string(),
                format!("Rule {}", i),
                None,
                vec![NotificationChannelType::Email],
                vec!["test@example.com".to_string()],
            )
            .set_enabled(enabled);

            repo.create_rule(&rule).await.unwrap();
        }

        let stats = repo.get_rule_statistics().await.unwrap();
        assert_eq!(stats.total_rules, 5);
        assert_eq!(stats.enabled_rules, 3);
        assert_eq!(stats.disabled_rules, 2);
    }
}

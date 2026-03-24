use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::{debug, error, info};

use crate::{
    domain::event::{
        EventError, NotificationChannelType, NotificationRecord, NotificationStatus, Result,
    },
    infrastructure::persistence::database::Database,
};

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
    /// Create a new notification history repository
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Convert database row to NotificationRecord
    fn row_to_notification_record(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<NotificationRecord> {
        let method_str: String = row.try_get("notification_method")?;
        let notification_method =
            NotificationChannelType::from_str(&method_str).ok_or_else(|| {
                EventError::Validation {
                    message: format!("Invalid notification method: {}", method_str),
                }
            })?;

        let status_str: String = row.try_get("status")?;
        let status =
            NotificationStatus::from_str(&status_str).ok_or_else(|| EventError::Validation {
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

impl NotificationHistoryRepositoryImpl {
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
    pub async fn get_statistics(&self, days: i32) -> Result<NotificationStatistics> {
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

        Ok(NotificationStatistics {
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

        // Get total count
        let count_row = sqlx::query("SELECT COUNT(*) as total FROM notification_history")
            .fetch_one(pool)
            .await?;
        let total_count: i64 = count_row.try_get("total")?;

        // Get records with pagination
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

/// Notification statistics
#[derive(Debug, Clone)]
pub struct NotificationStatistics {
    pub total_notifications: u64,
    pub sent_count: u64,
    pub failed_count: u64,
    pub pending_count: u64,
    pub success_rate: f64,
    pub period_days: i32,
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::infrastructure::persistence::database::Database;

    async fn create_test_db() -> Arc<Database> {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let db = Database::new(pool);
        Arc::new(db)
    }

    #[tokio::test]
    async fn test_store_and_retrieve_record() {
        let db = create_test_db().await;
        let repo = NotificationHistoryRepositoryImpl::new(db);

        let record = NotificationRecord {
            id: Uuid::new_v4().to_string(),
            event_id: "test-event-1".to_string(),
            rule_id: "test-rule-1".to_string(),
            notification_method: NotificationChannelType::Email,
            recipient: "test@example.com".to_string(),
            status: NotificationStatus::Pending,
            sent_at: None,
            error_message: None,
            created_at: Utc::now(),
        };

        // Store the record
        repo.store_record(&record).await.unwrap();

        // Retrieve records for the event
        let retrieved_records = repo.get_records(&record.event_id).await.unwrap();
        assert_eq!(retrieved_records.len(), 1);
        assert_eq!(retrieved_records[0].id, record.id);
        assert_eq!(retrieved_records[0].recipient, record.recipient);
    }

    #[tokio::test]
    async fn test_update_status() {
        let db = create_test_db().await;
        let repo = NotificationHistoryRepositoryImpl::new(db);

        let record = NotificationRecord {
            id: Uuid::new_v4().to_string(),
            event_id: "test-event-2".to_string(),
            rule_id: "test-rule-2".to_string(),
            notification_method: NotificationChannelType::Sms,
            recipient: "+1234567890".to_string(),
            status: NotificationStatus::Pending,
            sent_at: None,
            error_message: None,
            created_at: Utc::now(),
        };

        // Store the record
        repo.store_record(&record).await.unwrap();

        // Update status to sent
        repo.update_status(&record.id, NotificationStatus::Sent, None).await.unwrap();

        // Retrieve and verify
        let retrieved_records = repo.get_records(&record.event_id).await.unwrap();
        assert_eq!(retrieved_records.len(), 1);
        assert_eq!(retrieved_records[0].status, NotificationStatus::Sent);
        assert!(retrieved_records[0].sent_at.is_some());
    }

    #[tokio::test]
    async fn test_update_status_with_error() {
        let db = create_test_db().await;
        let repo = NotificationHistoryRepositoryImpl::new(db);

        let record = NotificationRecord {
            id: Uuid::new_v4().to_string(),
            event_id: "test-event-3".to_string(),
            rule_id: "test-rule-3".to_string(),
            notification_method: NotificationChannelType::Email,
            recipient: "test@example.com".to_string(),
            status: NotificationStatus::Pending,
            sent_at: None,
            error_message: None,
            created_at: Utc::now(),
        };

        // Store the record
        repo.store_record(&record).await.unwrap();

        // Update status to failed with error message
        let error_msg = "SMTP connection failed".to_string();
        repo.update_status(&record.id, NotificationStatus::Failed, Some(error_msg.clone()))
            .await
            .unwrap();

        // Retrieve and verify
        let retrieved_records = repo.get_records(&record.event_id).await.unwrap();
        assert_eq!(retrieved_records.len(), 1);
        assert_eq!(retrieved_records[0].status, NotificationStatus::Failed);
        assert_eq!(retrieved_records[0].error_message, Some(error_msg));
    }

    #[tokio::test]
    async fn test_get_statistics() {
        let db = create_test_db().await;
        let repo = NotificationHistoryRepositoryImpl::new(db);

        // Create test records
        for i in 0..10 {
            let status = match i % 3 {
                0 => NotificationStatus::Sent,
                1 => NotificationStatus::Failed,
                _ => NotificationStatus::Pending,
            };
            let sent_at =
                if status.clone() == NotificationStatus::Sent { Some(Utc::now()) } else { None };

            let record = NotificationRecord {
                id: Uuid::new_v4().to_string(),
                event_id: format!("test-event-{}", i),
                rule_id: "test-rule".to_string(),
                notification_method: NotificationChannelType::Email,
                recipient: "test@example.com".to_string(),
                status,
                sent_at,
                error_message: None,
                created_at: Utc::now(),
            };

            repo.store_record(&record).await.unwrap();
        }

        // Get statistics
        let stats = repo.get_statistics(7).await.unwrap();
        assert_eq!(stats.total_notifications, 10);
        assert!(stats.sent_count > 0);
        assert!(stats.failed_count > 0);
        assert!(stats.pending_count > 0);
        assert!(stats.success_rate >= 0.0 && stats.success_rate <= 100.0);
    }

    #[tokio::test]
    async fn test_pagination() {
        let db = create_test_db().await;
        let repo = NotificationHistoryRepositoryImpl::new(db);

        // Create test records
        for i in 0..25 {
            let record = NotificationRecord {
                id: Uuid::new_v4().to_string(),
                event_id: format!("test-event-{}", i),
                rule_id: "test-rule".to_string(),
                notification_method: NotificationChannelType::Email,
                recipient: "test@example.com".to_string(),
                status: NotificationStatus::Sent,
                sent_at: Some(Utc::now()),
                error_message: None,
                created_at: Utc::now(),
            };

            repo.store_record(&record).await.unwrap();
        }

        // Test pagination
        let (first_page, total_count) = repo.get_records_paginated(0, 10).await.unwrap();
        assert_eq!(first_page.len(), 10);
        assert_eq!(total_count, 25);

        let (second_page, _) = repo.get_records_paginated(10, 10).await.unwrap();
        assert_eq!(second_page.len(), 10);

        let (third_page, _) = repo.get_records_paginated(20, 10).await.unwrap();
        assert_eq!(third_page.len(), 5);
    }
}

// Event audit logging implementations
use std::sync::Arc;

use crate::domains::event::{
    EventError, Result,
    entities::Event,
    value_objects::{EventId, EventLevel, EventType},
};
use chrono::{DateTime, Utc};
use tracing::{error, info};

pub use tinyiothub_storage::audit_log::AuditLogEntry;

/// Event audit log trait
#[async_trait::async_trait]
pub trait EventAuditLog: Send + Sync {
    /// Log a generic audit entry
    async fn log(&self, entry: AuditLogEntry) -> Result<()>;

    /// Log event creation
    async fn log_event_created(&self, user_id: &str, event_id: &EventId, event: &Event) -> Result<()>;

    /// Log event access
    async fn log_event_accessed(&self, user_id: &str, event_id: &EventId) -> Result<()>;

    /// Log event update
    async fn log_event_updated(
        &self,
        user_id: &str,
        event_id: &EventId,
        old_event: &Event,
        new_event: &Event,
    ) -> Result<()>;

    /// Log event deletion
    async fn log_event_deleted(&self, user_id: &str, event_id: &EventId, event: &Event) -> Result<()>;

    /// Log access denied
    async fn log_access_denied(&self, user_id: &str, action: &str, resource: &str, reason: &str) -> Result<()>;

    /// Log event query
    async fn log_event_query(
        &self,
        user_id: &str,
        event_type: Option<EventType>,
        level: Option<EventLevel>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        result_count: usize,
    ) -> Result<()>;

    /// Get user audit logs
    async fn get_user_audit_logs(&self, user_id: &str, limit: Option<usize>) -> Result<Vec<AuditLogEntry>>;

    /// Get event audit logs
    async fn get_event_audit_logs(&self, event_id: &EventId, limit: Option<usize>) -> Result<Vec<AuditLogEntry>>;

    /// Get all audit logs (admin only)
    async fn get_all_audit_logs(&self, limit: Option<usize>, offset: Option<usize>) -> Result<Vec<AuditLogEntry>>;

    /// Clean up old logs
    async fn cleanup_old_logs(&self, retention_days: u32) -> Result<usize>;
}

/// Db-backed audit log implementation
pub struct DatabaseAuditLog {
    db: Arc<tinyiothub_storage::Db>,
}

impl DatabaseAuditLog {
    pub fn new(db: Arc<tinyiothub_storage::Db>) -> Self {
        Self { db }
    }

    pub async fn initialize(&self) -> Result<()> {
        self.db.init_audit_log_storage().await.map_err(EventError::Database)?;

        info!("Audit log database initialized successfully");
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventAuditLog for DatabaseAuditLog {
    async fn log(&self, entry: AuditLogEntry) -> Result<()> {
        self.db.insert_audit_log(&entry).await.map_err(|e| {
            error!("Failed to log audit entry: {}", e);
            EventError::Database(e)
        })?;

        Ok(())
    }

    async fn log_event_created(&self, user_id: &str, event_id: &EventId, event: &Event) -> Result<()> {
        let entry = AuditLogEntry::new("event_created".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(event.event_type().to_string())
            .with_event_level(event.level().as_str().to_string())
            .with_details(format!("Event created: {}", event.content().title()));

        self.log(entry).await
    }

    async fn log_event_accessed(&self, user_id: &str, event_id: &EventId) -> Result<()> {
        let entry = AuditLogEntry::new("event_accessed".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string());

        self.log(entry).await
    }

    async fn log_event_updated(
        &self,
        user_id: &str,
        event_id: &EventId,
        _old_event: &Event,
        new_event: &Event,
    ) -> Result<()> {
        let entry = AuditLogEntry::new("event_updated".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(new_event.event_type().to_string())
            .with_event_level(new_event.level().as_str().to_string())
            .with_details("Event content updated".to_string());

        self.log(entry).await
    }

    async fn log_event_deleted(&self, user_id: &str, event_id: &EventId, event: &Event) -> Result<()> {
        let entry = AuditLogEntry::new("event_deleted".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(event.event_type().to_string())
            .with_event_level(event.level().as_str().to_string())
            .with_details("Event deleted".to_string());

        self.log(entry).await
    }

    async fn log_access_denied(&self, user_id: &str, action: &str, resource: &str, reason: &str) -> Result<()> {
        let entry = AuditLogEntry::new("access_denied".to_string(), Some(user_id.to_string()))
            .with_result("denied".to_string())
            .with_details(format!(
                "Action: {}, Resource: {}, Reason: {}",
                action, resource, reason
            ));

        self.log(entry).await
    }

    async fn log_event_query(
        &self,
        user_id: &str,
        event_type: Option<EventType>,
        level: Option<EventLevel>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        result_count: usize,
    ) -> Result<()> {
        let details = serde_json::json!({
            "event_type": event_type.map(|t| t.to_string()),
            "level": level.map(|l| l.as_str()),
            "start_time": start_time.map(|t| t.to_rfc3339()),
            "end_time": end_time.map(|t| t.to_rfc3339()),
            "result_count": result_count
        });

        let entry =
            AuditLogEntry::new("event_query".to_string(), Some(user_id.to_string())).with_details(details.to_string());

        self.log(entry).await
    }

    async fn get_user_audit_logs(&self, user_id: &str, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        let limit = limit.unwrap_or(100).min(1000);

        let entries = self
            .db
            .list_audit_logs_by_user(user_id, limit as i64)
            .await
            .map_err(EventError::Database)?;

        Ok(entries)
    }

    async fn get_event_audit_logs(&self, event_id: &EventId, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        let limit = limit.unwrap_or(100).min(1000);

        let entries = self
            .db
            .list_audit_logs_by_event(&event_id.to_string(), limit as i64)
            .await
            .map_err(EventError::Database)?;

        Ok(entries)
    }

    async fn get_all_audit_logs(&self, limit: Option<usize>, offset: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        let limit = limit.unwrap_or(100).min(1000);
        let offset = offset.unwrap_or(0);

        let entries = self
            .db
            .list_all_audit_logs(limit as i64, offset as i64)
            .await
            .map_err(EventError::Database)?;

        Ok(entries)
    }

    async fn cleanup_old_logs(&self, retention_days: u32) -> Result<usize> {
        let cutoff_date = Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff_date.format("%Y-%m-%d %H:%M:%S").to_string();

        let deleted_count = self
            .db
            .delete_old_audit_logs(&cutoff_str)
            .await
            .map_err(EventError::Database)? as usize;
        info!("Cleaned up {} old audit log entries", deleted_count);

        Ok(deleted_count)
    }
}

/// In-memory audit log implementation (for testing)
pub struct InMemoryAuditLog {
    entries: Arc<tokio::sync::RwLock<Vec<AuditLogEntry>>>,
}

impl Default for InMemoryAuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryAuditLog {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl EventAuditLog for InMemoryAuditLog {
    async fn log(&self, entry: AuditLogEntry) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.push(entry);
        Ok(())
    }

    async fn log_event_created(&self, user_id: &str, event_id: &EventId, event: &Event) -> Result<()> {
        let entry = AuditLogEntry::new("event_created".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(event.event_type().to_string())
            .with_event_level(event.level().as_str().to_string());

        self.log(entry).await
    }

    async fn log_event_accessed(&self, user_id: &str, event_id: &EventId) -> Result<()> {
        let entry = AuditLogEntry::new("event_accessed".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string());

        self.log(entry).await
    }

    async fn log_event_updated(
        &self,
        user_id: &str,
        event_id: &EventId,
        _old_event: &Event,
        new_event: &Event,
    ) -> Result<()> {
        let entry = AuditLogEntry::new("event_updated".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(new_event.event_type().to_string())
            .with_event_level(new_event.level().as_str().to_string());

        self.log(entry).await
    }

    async fn log_event_deleted(&self, user_id: &str, event_id: &EventId, event: &Event) -> Result<()> {
        let entry = AuditLogEntry::new("event_deleted".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(event.event_type().to_string())
            .with_event_level(event.level().as_str().to_string());

        self.log(entry).await
    }

    async fn log_access_denied(&self, user_id: &str, action: &str, resource: &str, reason: &str) -> Result<()> {
        let entry = AuditLogEntry::new("access_denied".to_string(), Some(user_id.to_string()))
            .with_result("denied".to_string())
            .with_details(format!(
                "Action: {}, Resource: {}, Reason: {}",
                action, resource, reason
            ));

        self.log(entry).await
    }

    async fn log_event_query(
        &self,
        user_id: &str,
        _event_type: Option<EventType>,
        _level: Option<EventLevel>,
        _start_time: Option<DateTime<Utc>>,
        _end_time: Option<DateTime<Utc>>,
        result_count: usize,
    ) -> Result<()> {
        let entry = AuditLogEntry::new("event_query".to_string(), Some(user_id.to_string()))
            .with_details(format!("Query returned {} results", result_count));

        self.log(entry).await
    }

    async fn get_user_audit_logs(&self, user_id: &str, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        let entries = self.entries.read().await;
        let limit = limit.unwrap_or(100);

        let filtered: Vec<AuditLogEntry> = entries
            .iter()
            .filter(|entry| entry.user_id.as_ref() == Some(&user_id.to_string()))
            .take(limit)
            .cloned()
            .collect();

        Ok(filtered)
    }

    async fn get_event_audit_logs(&self, event_id: &EventId, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        let entries = self.entries.read().await;
        let limit = limit.unwrap_or(100);

        let filtered: Vec<AuditLogEntry> = entries
            .iter()
            .filter(|entry| entry.event_id.as_ref() == Some(&event_id.to_string()))
            .take(limit)
            .cloned()
            .collect();

        Ok(filtered)
    }

    async fn get_all_audit_logs(&self, limit: Option<usize>, offset: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        let entries = self.entries.read().await;
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let filtered: Vec<AuditLogEntry> = entries.iter().skip(offset).take(limit).cloned().collect();

        Ok(filtered)
    }

    async fn cleanup_old_logs(&self, retention_days: u32) -> Result<usize> {
        let cutoff_date = Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff_date.format("%Y-%m-%d %H:%M:%S").to_string();

        let mut entries = self.entries.write().await;
        let initial_count = entries.len();

        entries.retain(|entry| entry.created_at >= cutoff_str);

        let deleted_count = initial_count - entries.len();
        Ok(deleted_count)
    }
}

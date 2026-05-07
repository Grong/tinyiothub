// Event audit logging implementations
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::modules::event::{
    EventError, Result,
    entities::Event,
    value_objects::{EventId, EventLevel, EventType},
};

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub action: String,
    pub user_id: Option<String>,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub event_level: Option<String>,
    pub result: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
}

impl AuditLogEntry {
    pub fn new(action: String, user_id: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            action,
            user_id,
            event_id: None,
            event_type: None,
            event_level: None,
            result: Some("success".to_string()),
            details: None,
            ip_address: None,
            user_agent: None,
            created_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    pub fn with_event_id(mut self, event_id: String) -> Self {
        self.event_id = Some(event_id);
        self
    }

    pub fn with_event_type(mut self, event_type: String) -> Self {
        self.event_type = Some(event_type);
        self
    }

    pub fn with_event_level(mut self, event_level: String) -> Self {
        self.event_level = Some(event_level);
        self
    }

    pub fn with_result(mut self, result: String) -> Self {
        self.result = Some(result);
        self
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_ip_address(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }

    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }
}

/// Event audit log trait
#[async_trait::async_trait]
pub trait EventAuditLog: Send + Sync {
    /// Log a generic audit entry
    async fn log(&self, entry: AuditLogEntry) -> Result<()>;

    /// Log event creation
    async fn log_event_created(
        &self,
        user_id: &str,
        event_id: &EventId,
        event: &Event,
    ) -> Result<()>;

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
    async fn log_event_deleted(
        &self,
        user_id: &str,
        event_id: &EventId,
        event: &Event,
    ) -> Result<()>;

    /// Log access denied
    async fn log_access_denied(
        &self,
        user_id: &str,
        action: &str,
        resource: &str,
        reason: &str,
    ) -> Result<()>;

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
    async fn get_user_audit_logs(
        &self,
        user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>>;

    /// Get event audit logs
    async fn get_event_audit_logs(
        &self,
        event_id: &EventId,
        limit: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>>;

    /// Get all audit logs (admin only)
    async fn get_all_audit_logs(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>>;

    /// Clean up old logs
    async fn cleanup_old_logs(&self, retention_days: u32) -> Result<usize>;
}

/// Database-backed audit log implementation
pub struct DatabaseAuditLog {
    db: Arc<crate::shared::persistence::Database>,
}

impl DatabaseAuditLog {
    pub fn new(db: Arc<crate::shared::persistence::Database>) -> Self {
        Self { db }
    }

    pub async fn initialize(&self) -> Result<()> {
        // Create audit log table if it doesn't exist
        let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                action TEXT NOT NULL,
                user_id TEXT,
                event_id TEXT,
                event_type TEXT,
                event_level TEXT,
                result TEXT,
                details TEXT,
                ip_address TEXT,
                user_agent TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE SET NULL
            )
        "#;

        sqlx::query(create_table_sql)
            .execute(self.db.pool())
            .await
            .map_err(EventError::Database)?;

        // Create indexes for better query performance
        let create_indexes_sql = vec![
            "CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id)",
            "CREATE INDEX IF NOT EXISTS idx_audit_logs_event_id ON audit_logs(event_id)",
            "CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at)",
            "CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action)",
        ];

        for sql in create_indexes_sql {
            sqlx::query(sql).execute(self.db.pool()).await.map_err(EventError::Database)?;
        }

        info!("Audit log database initialized successfully");
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventAuditLog for DatabaseAuditLog {
    async fn log(&self, entry: AuditLogEntry) -> Result<()> {
        let sql = r#"
            INSERT INTO audit_logs (
                id, action, user_id, event_id, event_type, event_level,
                result, details, ip_address, user_agent, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        sqlx::query(sql)
            .bind(&entry.id)
            .bind(&entry.action)
            .bind(&entry.user_id)
            .bind(&entry.event_id)
            .bind(&entry.event_type)
            .bind(&entry.event_level)
            .bind(&entry.result)
            .bind(&entry.details)
            .bind(&entry.ip_address)
            .bind(&entry.user_agent)
            .bind(&entry.created_at)
            .execute(self.db.pool())
            .await
            .map_err(|e| {
                error!("Failed to log audit entry: {}", e);
                EventError::Database(e)
            })?;

        Ok(())
    }

    async fn log_event_created(
        &self,
        user_id: &str,
        event_id: &EventId,
        event: &Event,
    ) -> Result<()> {
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

    async fn log_event_deleted(
        &self,
        user_id: &str,
        event_id: &EventId,
        event: &Event,
    ) -> Result<()> {
        let entry = AuditLogEntry::new("event_deleted".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(event.event_type().to_string())
            .with_event_level(event.level().as_str().to_string())
            .with_details("Event deleted".to_string());

        self.log(entry).await
    }

    async fn log_access_denied(
        &self,
        user_id: &str,
        action: &str,
        resource: &str,
        reason: &str,
    ) -> Result<()> {
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

        let entry = AuditLogEntry::new("event_query".to_string(), Some(user_id.to_string()))
            .with_details(details.to_string());

        self.log(entry).await
    }

    async fn get_user_audit_logs(
        &self,
        user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>> {
        let limit = limit.unwrap_or(100).min(1000);

        let sql = r#"
            SELECT id, action, user_id, event_id, event_type, event_level,
                   result, details, ip_address, user_agent, created_at
            FROM audit_logs
            WHERE user_id = ?
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
            ),
        >(sql)
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(self.db.pool())
        .await
        .map_err(EventError::Database)?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    action,
                    user_id,
                    event_id,
                    event_type,
                    event_level,
                    result,
                    details,
                    ip_address,
                    user_agent,
                    created_at,
                )| {
                    AuditLogEntry {
                        id,
                        action,
                        user_id,
                        event_id,
                        event_type,
                        event_level,
                        result,
                        details,
                        ip_address,
                        user_agent,
                        created_at,
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    async fn get_event_audit_logs(
        &self,
        event_id: &EventId,
        limit: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>> {
        let limit = limit.unwrap_or(100).min(1000);

        let sql = r#"
            SELECT id, action, user_id, event_id, event_type, event_level,
                   result, details, ip_address, user_agent, created_at
            FROM audit_logs
            WHERE event_id = ?
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
            ),
        >(sql)
        .bind(event_id.to_string())
        .bind(limit as i64)
        .fetch_all(self.db.pool())
        .await
        .map_err(EventError::Database)?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    action,
                    user_id,
                    event_id,
                    event_type,
                    event_level,
                    result,
                    details,
                    ip_address,
                    user_agent,
                    created_at,
                )| {
                    AuditLogEntry {
                        id,
                        action,
                        user_id,
                        event_id,
                        event_type,
                        event_level,
                        result,
                        details,
                        ip_address,
                        user_agent,
                        created_at,
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    async fn get_all_audit_logs(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>> {
        let limit = limit.unwrap_or(100).min(1000);
        let offset = offset.unwrap_or(0);

        let sql = r#"
            SELECT id, action, user_id, event_id, event_type, event_level,
                   result, details, ip_address, user_agent, created_at
            FROM audit_logs
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
        "#;

        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
            ),
        >(sql)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.db.pool())
        .await
        .map_err(EventError::Database)?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    action,
                    user_id,
                    event_id,
                    event_type,
                    event_level,
                    result,
                    details,
                    ip_address,
                    user_agent,
                    created_at,
                )| {
                    AuditLogEntry {
                        id,
                        action,
                        user_id,
                        event_id,
                        event_type,
                        event_level,
                        result,
                        details,
                        ip_address,
                        user_agent,
                        created_at,
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    async fn cleanup_old_logs(&self, retention_days: u32) -> Result<usize> {
        let cutoff_date = Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff_date.format("%Y-%m-%d %H:%M:%S").to_string();

        let sql = "DELETE FROM audit_logs WHERE created_at < ?";

        let result = sqlx::query(sql)
            .bind(cutoff_str)
            .execute(self.db.pool())
            .await
            .map_err(EventError::Database)?;

        let deleted_count = result.rows_affected() as usize;
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
        Self { entries: Arc::new(tokio::sync::RwLock::new(Vec::new())) }
    }
}

#[async_trait::async_trait]
impl EventAuditLog for InMemoryAuditLog {
    async fn log(&self, entry: AuditLogEntry) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.push(entry);
        Ok(())
    }

    async fn log_event_created(
        &self,
        user_id: &str,
        event_id: &EventId,
        event: &Event,
    ) -> Result<()> {
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

    async fn log_event_deleted(
        &self,
        user_id: &str,
        event_id: &EventId,
        event: &Event,
    ) -> Result<()> {
        let entry = AuditLogEntry::new("event_deleted".to_string(), Some(user_id.to_string()))
            .with_event_id(event_id.to_string())
            .with_event_type(event.event_type().to_string())
            .with_event_level(event.level().as_str().to_string());

        self.log(entry).await
    }

    async fn log_access_denied(
        &self,
        user_id: &str,
        action: &str,
        resource: &str,
        reason: &str,
    ) -> Result<()> {
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

    async fn get_user_audit_logs(
        &self,
        user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>> {
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

    async fn get_event_audit_logs(
        &self,
        event_id: &EventId,
        limit: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>> {
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

    async fn get_all_audit_logs(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<AuditLogEntry>> {
        let entries = self.entries.read().await;
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let filtered: Vec<AuditLogEntry> =
            entries.iter().skip(offset).take(limit).cloned().collect();

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

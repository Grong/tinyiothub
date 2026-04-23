use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::event::{value_objects::EventLevel, EventError, Result};

/// Notification Status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NotificationStatus {
    Pending,
    Sent,
    Failed,
    Acknowledged,
}

impl std::fmt::Display for NotificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationStatus::Pending => write!(f, "Pending"),
            NotificationStatus::Sent => write!(f, "Sent"),
            NotificationStatus::Failed => write!(f, "Failed"),
            NotificationStatus::Acknowledged => write!(f, "Acknowledged"),
        }
    }
}

impl NotificationStatus {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(NotificationStatus::Pending),
            "sent" => Some(NotificationStatus::Sent),
            "acknowledged" => Some(NotificationStatus::Acknowledged),
            s if s.starts_with("failed") => Some(NotificationStatus::Failed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            NotificationStatus::Pending => "pending",
            NotificationStatus::Sent => "sent",
            NotificationStatus::Failed => "failed",
            NotificationStatus::Acknowledged => "acknowledged",
        }
    }
}

/// Notification Channel Type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotificationChannelType {
    Email,
    Sms,
    Sse,
    Webhook,
}

impl NotificationChannelType {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "email" => Some(NotificationChannelType::Email),
            "sms" => Some(NotificationChannelType::Sms),
            "sse" => Some(NotificationChannelType::Sse),
            "webhook" => Some(NotificationChannelType::Webhook),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            NotificationChannelType::Email => "email",
            NotificationChannelType::Sms => "sms",
            NotificationChannelType::Sse => "sse",
            NotificationChannelType::Webhook => "webhook",
        }
    }
}

/// Notification Rule Entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<serde_json::Value>, // Will be properly typed later
    pub notification_methods: Vec<NotificationChannelType>,
    pub recipients: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Legacy compatibility fields
    pub event_types: Vec<String>,
    pub event_levels: Vec<crate::domain::event::value_objects::EventLevel>,
    pub channels: Vec<NotificationChannelType>,
    pub conditions: std::collections::HashMap<String, String>,
    pub is_active: bool,
}

impl NotificationRule {
    /// Create a new notification rule
    pub fn new(
        id: String,
        name: String,
        description: Option<String>,
        notification_methods: Vec<NotificationChannelType>,
        recipients: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            description,
            event_type: None,
            event_subtype: None,
            event_level: None,
            device_filter: None,
            notification_methods: notification_methods.clone(),
            recipients,
            enabled: true,
            created_at: now,
            updated_at: now,

            // Initialize legacy compatibility fields
            event_types: Vec::new(),
            event_levels: Vec::new(),
            channels: notification_methods,
            conditions: std::collections::HashMap::new(),
            is_active: true,
        }
    }

    /// Set enabled status
    pub fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self.updated_at = Utc::now();
        self
    }

    /// Set event type filter
    pub fn with_event_type(mut self, event_type: String) -> Self {
        self.event_type = Some(event_type);
        self.updated_at = Utc::now();
        self
    }

    /// Set event subtype filter
    pub fn with_event_subtype(mut self, event_subtype: String) -> Self {
        self.event_subtype = Some(event_subtype);
        self.updated_at = Utc::now();
        self
    }

    /// Set event level filter
    pub fn with_event_level(mut self, event_level: i32) -> Self {
        self.event_level = Some(event_level);
        self.updated_at = Utc::now();
        self
    }

    /// Set device filter
    pub fn with_device_filter(mut self, device_filter: serde_json::Value) -> Self {
        self.device_filter = Some(device_filter);
        self.updated_at = Utc::now();
        self
    }
}

/// Notification Record Entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: String,
    pub event_id: String, // Changed from EventId to String for compatibility
    pub rule_id: String,
    pub notification_method: NotificationChannelType, // Changed field name for compatibility
    pub recipient: String,
    pub status: NotificationStatus,
    pub sent_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Notification Aggregate Root
///
/// Manages notification rules and records, enforcing business logic
/// for notification delivery and acknowledgment.
pub struct NotificationAggregate {
    rule: NotificationRule,
    records: Vec<NotificationRecord>,
    version: u64,
}

impl NotificationAggregate {
    /// Create a new notification aggregate
    pub fn new(
        name: String,
        event_types: Vec<String>,
        event_levels: Vec<EventLevel>,
        channels: Vec<NotificationChannelType>,
        recipients: Vec<String>,
    ) -> Result<Self> {
        // Business rule: Must have at least one channel and recipient
        if channels.is_empty() {
            return Err(EventError::Validation {
                message: "Notification rule must have at least one channel".to_string(),
            });
        }

        if recipients.is_empty() {
            return Err(EventError::Validation {
                message: "Notification rule must have at least one recipient".to_string(),
            });
        }

        let now = Utc::now();
        let rule = NotificationRule {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: None,
            event_type: event_types.first().cloned(),
            event_subtype: None,
            event_level: event_levels.first().map(|l| *l as i32),
            device_filter: None,
            notification_methods: channels.clone(),
            recipients,
            enabled: true,
            created_at: now,
            updated_at: now,

            // Initialize legacy compatibility fields
            event_types,
            event_levels,
            channels,
            conditions: std::collections::HashMap::new(),
            is_active: true,
        };

        Ok(Self { rule, records: Vec::new(), version: 1 })
    }

    /// Create aggregate from existing rule
    pub fn from_rule(rule: NotificationRule) -> Self {
        Self { rule, records: Vec::new(), version: 1 }
    }

    /// Get the notification rule
    pub fn rule(&self) -> &NotificationRule {
        &self.rule
    }

    /// Get notification records
    pub fn records(&self) -> &[NotificationRecord] {
        &self.records
    }

    /// Get aggregate version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Check if rule matches event (business logic)
    pub fn matches_event(&self, event_type: &str, event_level: &EventLevel) -> bool {
        if !self.rule.enabled {
            return false;
        }

        // Check event type match
        let type_match = self.rule.event_type.is_none()
            || self.rule.event_type.as_ref() == Some(&event_type.to_string());

        // Check event level match
        let level_match =
            self.rule.event_level.is_none() || self.rule.event_level == Some(*event_level as i32);

        type_match && level_match
    }

    /// Create notification records for an event (business logic)
    pub fn create_notifications(
        &mut self,
        event_id: String,
        _message: String,
    ) -> Result<Vec<NotificationRecord>> {
        if !self.rule.enabled {
            return Ok(Vec::new());
        }

        let mut new_records = Vec::new();

        for channel in &self.rule.notification_methods {
            for recipient in &self.rule.recipients {
                let record = NotificationRecord {
                    id: uuid::Uuid::new_v4().to_string(),
                    event_id: event_id.clone(),
                    rule_id: self.rule.id.clone(),
                    notification_method: channel.clone(),
                    recipient: recipient.clone(),
                    status: NotificationStatus::Pending,
                    sent_at: None,
                    error_message: None,
                    created_at: Utc::now(),
                };

                new_records.push(record.clone());
                self.records.push(record);
            }
        }

        self.version += 1;
        Ok(new_records)
    }

    /// Mark notification as sent (business logic)
    pub fn mark_notification_sent(&mut self, notification_id: &str) -> Result<()> {
        if let Some(record) = self.records.iter_mut().find(|r| r.id == notification_id) {
            record.status = NotificationStatus::Sent;
            record.sent_at = Some(Utc::now());
            self.version += 1;
            Ok(())
        } else {
            Err(EventError::NotFound { id: notification_id.to_string() })
        }
    }

    /// Mark notification as failed (business logic)
    pub fn mark_notification_failed(&mut self, notification_id: &str, error: String) -> Result<()> {
        if let Some(record) = self.records.iter_mut().find(|r| r.id == notification_id) {
            record.status = NotificationStatus::Failed;
            record.error_message = Some(error);
            self.version += 1;
            Ok(())
        } else {
            Err(EventError::NotFound { id: notification_id.to_string() })
        }
    }

    /// Check if notification should be retried (business rule)
    pub fn should_retry_notification(&self, notification_id: &str) -> bool {
        if let Some(record) = self.records.iter().find(|r| r.id == notification_id) {
            matches!(record.status, NotificationStatus::Failed)
        } else {
            false
        }
    }

    /// Update rule configuration
    pub fn update_rule(
        &mut self,
        name: Option<String>,
        event_types: Option<Vec<String>>,
        event_levels: Option<Vec<EventLevel>>,
        channels: Option<Vec<NotificationChannelType>>,
        recipients: Option<Vec<String>>,
    ) -> Result<()> {
        if let Some(name) = name {
            self.rule.name = name;
        }

        if let Some(event_types) = event_types {
            self.rule.event_type = event_types.first().cloned();
        }

        if let Some(event_levels) = event_levels {
            self.rule.event_level = event_levels.first().map(|l| *l as i32);
        }

        if let Some(channels) = channels {
            if channels.is_empty() {
                return Err(EventError::Validation {
                    message: "Notification rule must have at least one channel".to_string(),
                });
            }
            self.rule.notification_methods = channels;
        }

        if let Some(recipients) = recipients {
            if recipients.is_empty() {
                return Err(EventError::Validation {
                    message: "Notification rule must have at least one recipient".to_string(),
                });
            }
            self.rule.recipients = recipients;
        }

        self.rule.updated_at = Utc::now();
        self.version += 1;
        Ok(())
    }

    /// Activate/deactivate rule
    pub fn set_active(&mut self, is_active: bool) {
        self.rule.enabled = is_active;
        self.rule.updated_at = Utc::now();
        self.version += 1;
    }

    /// Get pending notifications count
    pub fn pending_notifications_count(&self) -> usize {
        self.records.iter().filter(|r| matches!(r.status, NotificationStatus::Pending)).count()
    }

    /// Get failed notifications that can be retried
    pub fn retryable_notifications(&self) -> Vec<&NotificationRecord> {
        self.records.iter().filter(|r| matches!(r.status, NotificationStatus::Failed)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::EventId;

    #[test]
    fn test_create_notification_aggregate() {
        let aggregate = NotificationAggregate::new(
            "Test Rule".to_string(),
            vec!["device.error".to_string()],
            vec![EventLevel::Error],
            vec![NotificationChannelType::Email],
            vec!["admin@example.com".to_string()],
        )
        .unwrap();

        assert_eq!(aggregate.rule().name, "Test Rule");
        assert_eq!(aggregate.version(), 1);
    }

    #[test]
    fn test_event_matching() {
        let aggregate = NotificationAggregate::new(
            "Test Rule".to_string(),
            vec!["device.error".to_string()],
            vec![EventLevel::Error],
            vec![NotificationChannelType::Email],
            vec!["admin@example.com".to_string()],
        )
        .unwrap();

        assert!(aggregate.matches_event("device.error", &EventLevel::Error));
        assert!(!aggregate.matches_event("device.info", &EventLevel::Info));
    }

    #[test]
    fn test_create_notifications() {
        let mut aggregate = NotificationAggregate::new(
            "Test Rule".to_string(),
            vec!["device.error".to_string()],
            vec![EventLevel::Error],
            vec![NotificationChannelType::Email, NotificationChannelType::Sms],
            vec!["admin@example.com".to_string(), "admin2@example.com".to_string()],
        )
        .unwrap();

        let event_id = EventId::new();
        let records = aggregate
            .create_notifications(event_id.to_string(), "Test message".to_string())
            .unwrap();

        // Should create 4 records (2 channels × 2 recipients)
        assert_eq!(records.len(), 4);
        assert_eq!(aggregate.records().len(), 4);
    }

    #[test]
    fn test_notification_status_updates() {
        let mut aggregate = NotificationAggregate::new(
            "Test Rule".to_string(),
            vec!["device.error".to_string()],
            vec![EventLevel::Error],
            vec![NotificationChannelType::Email],
            vec!["admin@example.com".to_string()],
        )
        .unwrap();

        let event_id = EventId::new();
        let records = aggregate
            .create_notifications(event_id.to_string(), "Test message".to_string())
            .unwrap();
        let notification_id = &records[0].id;

        // Mark as sent
        aggregate.mark_notification_sent(notification_id).unwrap();
        let record = aggregate.records().iter().find(|r| r.id == *notification_id).unwrap();
        assert!(matches!(record.status, NotificationStatus::Sent));
        assert!(record.sent_at.is_some());

        // Mark as failed
        aggregate.mark_notification_failed(notification_id, "Network error".to_string()).unwrap();
        let record = aggregate.records().iter().find(|r| r.id == *notification_id).unwrap();
        assert!(matches!(record.status, NotificationStatus::Failed));
        // Note: retry_count field doesn't exist in current NotificationRecord structure
    }
}

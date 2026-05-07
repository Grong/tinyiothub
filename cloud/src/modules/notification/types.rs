// Notification module types
// Consolidated from domain/event/aggregates/notification_aggregate.rs,
// domain/event/services/notification_service.rs, and api/notifications/management.rs

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::modules::event::{
    errors::{DomainResult, NotificationDomainError},
    value_objects::{EventId, EventLevel},
    EventError, Result,
};

// ──────────────────────────────────────────────
// Core domain types (from notification_aggregate.rs)
// ──────────────────────────────────────────────

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
    pub device_filter: Option<serde_json::Value>,
    pub notification_methods: Vec<NotificationChannelType>,
    pub recipients: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub workspace_id: Option<String>,

    // Legacy compatibility fields
    pub event_types: Vec<String>,
    pub event_levels: Vec<EventLevel>,
    pub channels: Vec<NotificationChannelType>,
    pub conditions: HashMap<String, String>,
    pub is_active: bool,
}

impl NotificationRule {
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
            workspace_id: None,
            event_types: Vec::new(),
            event_levels: Vec::new(),
            channels: notification_methods,
            conditions: HashMap::new(),
            is_active: true,
        }
    }

    pub fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self.updated_at = Utc::now();
        self
    }

    pub fn with_event_type(mut self, event_type: String) -> Self {
        self.event_type = Some(event_type);
        self.updated_at = Utc::now();
        self
    }

    pub fn with_event_subtype(mut self, event_subtype: String) -> Self {
        self.event_subtype = Some(event_subtype);
        self.updated_at = Utc::now();
        self
    }

    pub fn with_event_level(mut self, event_level: i32) -> Self {
        self.event_level = Some(event_level);
        self.updated_at = Utc::now();
        self
    }

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
    pub event_id: String,
    pub rule_id: String,
    pub notification_method: NotificationChannelType,
    pub recipient: String,
    pub status: NotificationStatus,
    pub sent_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Notification Aggregate Root
pub struct NotificationAggregate {
    rule: NotificationRule,
    records: Vec<NotificationRecord>,
    version: u64,
}

impl NotificationAggregate {
    pub fn new(
        name: String,
        event_types: Vec<String>,
        event_levels: Vec<EventLevel>,
        channels: Vec<NotificationChannelType>,
        recipients: Vec<String>,
    ) -> Result<Self> {
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
            workspace_id: None,
            event_types,
            event_levels,
            channels,
            conditions: HashMap::new(),
            is_active: true,
        };

        Ok(Self { rule, records: Vec::new(), version: 1 })
    }

    pub fn from_rule(rule: NotificationRule) -> Self {
        Self { rule, records: Vec::new(), version: 1 }
    }

    pub fn rule(&self) -> &NotificationRule {
        &self.rule
    }

    pub fn records(&self) -> &[NotificationRecord] {
        &self.records
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn matches_event(&self, event_type: &str, event_level: &EventLevel) -> bool {
        if !self.rule.enabled {
            return false;
        }
        let type_match = self.rule.event_type.is_none()
            || self.rule.event_type.as_ref() == Some(&event_type.to_string());
        let level_match =
            self.rule.event_level.is_none() || self.rule.event_level == Some(*event_level as i32);
        type_match && level_match
    }

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

    pub fn should_retry_notification(&self, notification_id: &str) -> bool {
        if let Some(record) = self.records.iter().find(|r| r.id == notification_id) {
            matches!(record.status, NotificationStatus::Failed)
        } else {
            false
        }
    }

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

    pub fn set_active(&mut self, is_active: bool) {
        self.rule.enabled = is_active;
        self.rule.updated_at = Utc::now();
        self.version += 1;
    }

    pub fn pending_notifications_count(&self) -> usize {
        self.records.iter().filter(|r| matches!(r.status, NotificationStatus::Pending)).count()
    }

    pub fn retryable_notifications(&self) -> Vec<&NotificationRecord> {
        self.records.iter().filter(|r| matches!(r.status, NotificationStatus::Failed)).collect()
    }
}

// ──────────────────────────────────────────────
// Service types (from notification_service.rs)
// ──────────────────────────────────────────────

/// Notification level (alias for EventLevel)
pub type NotificationLevel = EventLevel;

/// Notification message for delivery
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    pub id: String,
    pub event_id: EventId,
    pub channel: NotificationChannelType,
    pub recipient: String,
    pub subject: Option<String>,
    pub content: String,
    pub priority: u8,
    pub created_at: DateTime<Utc>,
    pub title: String,
    pub level: NotificationLevel,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub channels: Vec<NotificationChannelType>,
    pub recipients: Vec<String>,
}

impl NotificationMessage {
    pub fn new(
        title: String,
        content: String,
        level: NotificationLevel,
        channels: Vec<NotificationChannelType>,
        recipients: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_id: EventId::new(),
            channel: channels.first().cloned().unwrap_or(NotificationChannelType::Email),
            recipient: recipients.first().cloned().unwrap_or_default(),
            subject: Some(title.clone()),
            content: content.clone(),
            priority: 1,
            created_at: now,
            title,
            level,
            timestamp: now,
            metadata: HashMap::new(),
            channels,
            recipients,
        }
    }

    pub fn formatted_title(&self) -> String {
        format!("[{}] {}", self.level.as_str().to_uppercase(), self.title)
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Notification channel trait for delivery implementations
#[async_trait::async_trait]
pub trait NotificationChannel: Send + Sync {
    fn channel_type(&self) -> NotificationChannelType;
    async fn send(&self, message: &NotificationMessage) -> std::result::Result<(), String>;
    async fn is_available(&self) -> bool;
    fn get_config(&self) -> HashMap<String, String>;
}

/// Notification statistics (in-memory calculation, from notification_service.rs)
#[derive(Debug, Clone)]
pub struct NotificationStatistics {
    pub total: usize,
    pub pending: usize,
    pub sent: usize,
    pub failed: usize,
    pub acknowledged: usize,
    pub success_rate: f64,
    pub avg_delivery_time_seconds: f64,
}

// ──────────────────────────────────────────────
// Repository types (from *_repository_impl.rs)
// ──────────────────────────────────────────────

/// Notification rule statistics (from rule repository)
#[derive(Debug, Clone)]
pub struct RuleStatistics {
    pub total_rules: u64,
    pub enabled_rules: u64,
    pub disabled_rules: u64,
}

/// Notification history statistics (from history repository)
#[derive(Debug, Clone)]
pub struct HistoryStatistics {
    pub total_notifications: u64,
    pub sent_count: u64,
    pub failed_count: u64,
    pub pending_count: u64,
    pub success_rate: f64,
    pub period_days: i32,
}

// ──────────────────────────────────────────────
// API DTOs (from api/notifications/management.rs)
// ──────────────────────────────────────────────

/// Request to create a new notification rule
#[derive(Debug, Deserialize)]
pub struct CreateNotificationRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<DeviceFilterRequest>,
    pub notification_methods: Vec<String>,
    pub recipients: Vec<String>,
    pub enabled: Option<bool>,
}

/// Request to update a notification rule
#[derive(Debug, Deserialize)]
pub struct UpdateNotificationRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<DeviceFilterRequest>,
    pub notification_methods: Option<Vec<String>>,
    pub recipients: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

/// Device filter request
#[derive(Debug, Deserialize)]
pub struct DeviceFilterRequest {
    pub device_ids: Option<Vec<String>>,
    pub device_types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Notification rule response
#[derive(Debug, Serialize)]
pub struct NotificationRuleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub event_type: Option<String>,
    pub event_subtype: Option<String>,
    pub event_level: Option<i32>,
    pub device_filter: Option<DeviceFilterResponse>,
    pub notification_methods: Vec<String>,
    pub recipients: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Device filter response
#[derive(Debug, Serialize)]
pub struct DeviceFilterResponse {
    pub device_ids: Option<Vec<String>>,
    pub device_types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Notification history response
#[derive(Debug, Serialize)]
pub struct NotificationHistoryResponse {
    pub id: String,
    pub event_id: String,
    pub rule_id: String,
    pub notification_method: String,
    pub recipient: String,
    pub status: String,
    pub sent_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for notification rules
#[derive(Debug, Deserialize)]
pub struct NotificationRuleQuery {
    pub enabled: Option<bool>,
    pub event_type: Option<String>,
    pub notification_method: Option<String>,
}

/// Query parameters for notification history
#[derive(Debug, Deserialize)]
pub struct NotificationHistoryQuery {
    pub event_id: Option<String>,
    pub rule_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Test notification request
#[derive(Debug, Deserialize)]
pub struct TestNotificationRequest {
    pub title: String,
    pub content: String,
    pub level: String,
    pub channels: Vec<String>,
    pub recipients: Vec<String>,
}

/// Helper: convert JsonValue device filter to DeviceFilterResponse
pub fn convert_device_filter(filter: &serde_json::Value) -> DeviceFilterResponse {
    DeviceFilterResponse {
        device_ids: filter.get("device_ids").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
        }),
        device_types: filter.get("device_types").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
        }),
        tags: filter.get("tags").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
        }),
    }
}

/// Helper: convert DeviceFilterRequest to JsonValue
pub fn device_filter_to_json(filter: &DeviceFilterRequest) -> serde_json::Value {
    serde_json::json!({
        "device_ids": filter.device_ids,
        "device_types": filter.device_types,
        "tags": filter.tags
    })
}

// ──────────────────────────────────────────────
// Tests (from notification_aggregate.rs)
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::value_objects::EventId;

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

        // Should create 4 records (2 channels x 2 recipients)
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
    }
}

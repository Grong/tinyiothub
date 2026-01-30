use crate::domain::event::{
    aggregates::{
        NotificationAggregate, NotificationChannelType, NotificationRecord, NotificationRule,
        NotificationStatus,
    },
    errors::{DomainResult, NotificationDomainError},
    specifications::{
        NotificationDeliverySpec, NotificationFilterSpec, NotificationValidationSpec,
    },
    value_objects::{EventId, EventLevel},
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Notification level (alias for EventLevel for channel compatibility)
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
    /// Create a new notification message
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
            channel: channels
                .first()
                .cloned()
                .unwrap_or(NotificationChannelType::Email),
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

    /// Get formatted title with level prefix
    pub fn formatted_title(&self) -> String {
        format!("[{}] {}", self.level.as_str().to_uppercase(), self.title)
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Notification channel trait for delivery implementations
#[async_trait::async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Get the channel type
    fn channel_type(&self) -> NotificationChannelType;

    /// Send a notification message
    async fn send(&self, message: &NotificationMessage) -> Result<(), String>;

    /// Check if the channel is available
    async fn is_available(&self) -> bool;

    /// Get channel configuration
    fn get_config(&self) -> HashMap<String, String>;
}

/// Notification manager for coordinating notification delivery
pub struct NotificationManager {
    channels: HashMap<NotificationChannelType, Box<dyn NotificationChannel>>,
    notification_service: NotificationService,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            notification_service: NotificationService::new(),
        }
    }

    /// Register a notification channel
    pub fn register_channel(&mut self, channel: Box<dyn NotificationChannel>) {
        let channel_type = channel.channel_type();
        self.channels.insert(channel_type, channel);
    }

    /// Send a notification through the appropriate channel
    pub async fn send_notification(&self, message: &NotificationMessage) -> Result<(), String> {
        if let Some(channel) = self.channels.get(&message.channel) {
            if channel.is_available().await {
                channel.send(message).await
            } else {
                Err(format!("Channel {:?} is not available", message.channel))
            }
        } else {
            Err(format!("Channel {:?} is not registered", message.channel))
        }
    }

    /// Get available channels
    pub fn get_available_channels(&self) -> Vec<NotificationChannelType> {
        self.channels.keys().cloned().collect()
    }

    /// Process notifications for an event
    pub async fn process_event_notifications(
        &self,
        event_id: EventId,
        event_type: &str,
        event_level: &EventLevel,
        event_content: &str,
        event_source: &str,
        notification_rules: &mut [NotificationAggregate],
    ) -> Result<Vec<NotificationMessage>, String> {
        let event_metadata = HashMap::new(); // Could be populated with additional event data

        let notification_records = self
            .notification_service
            .process_event_for_notifications(
                event_id.clone(),
                event_type,
                event_level,
                &event_metadata,
                notification_rules,
                event_content.to_string(),
            )
            .map_err(|e| e.to_string())?;

        let mut messages = Vec::new();

        for record in notification_records {
            let message_content = self.notification_service.generate_notification_message(
                event_type,
                event_level,
                event_source,
                event_content,
                &record.notification_method,
            );

            let message = NotificationMessage {
                id: record.id.clone(),
                event_id: event_id.clone(),
                channel: record.notification_method.clone(),
                recipient: record.recipient.clone(),
                subject: Some(format!("Event Alert: {}", event_type)),
                content: message_content.clone(),
                priority: self.notification_service.get_delivery_priority(&record),
                created_at: record.created_at,
                title: format!("Event Alert: {}", event_type),
                level: *event_level,
                timestamp: record.created_at,
                metadata: HashMap::new(),
                channels: vec![record.notification_method.clone()],
                recipients: vec![record.recipient.clone()],
            };

            messages.push(message);
        }

        Ok(messages)
    }

    /// Get all notification rules
    pub async fn get_rules(&self) -> Result<Vec<NotificationRule>, String> {
        // This would typically fetch from a repository
        // For now, return empty vec as placeholder
        Ok(Vec::new())
    }

    /// Add a new notification rule
    pub async fn add_rule(&self, _rule: NotificationRule) -> Result<(), String> {
        // This would typically save to a repository
        // For now, just return Ok as placeholder
        Ok(())
    }

    /// Update an existing notification rule
    pub async fn update_rule(&self, _rule_id: &str, _rule: NotificationRule) -> Result<(), String> {
        // This would typically update in a repository
        // For now, just return Ok as placeholder
        Ok(())
    }

    /// Remove a notification rule
    pub async fn remove_rule(&self, _rule_id: &str) -> Result<(), String> {
        // This would typically delete from a repository
        // For now, just return Ok as placeholder
        Ok(())
    }

    /// Get notification history for an event
    pub async fn get_notification_history(
        &self,
        _event_id: &str,
    ) -> Result<Vec<NotificationRecord>, String> {
        // This would typically fetch from a repository
        // For now, return empty vec as placeholder
        Ok(Vec::new())
    }

    /// Send notification for an event (placeholder method)
    pub async fn notify(
        &self,
        _event: &crate::domain::event::entities::Event,
    ) -> Result<(), String> {
        // This would typically process the event and send notifications
        // For now, just return Ok as placeholder
        Ok(())
    }
}

/// Domain service for notification processing (pure business logic)
///
/// This service encapsulates the core business rules for notification
/// management, delivery logic, and filtering without infrastructure dependencies.
pub struct NotificationService {
    validation_spec: NotificationValidationSpec,
}

impl NotificationService {
    /// Create a new notification service
    pub fn new() -> Self {
        Self {
            validation_spec: NotificationValidationSpec::new(),
        }
    }

    /// Create a new notification rule with validation
    pub fn create_notification_rule(
        &self,
        name: String,
        event_types: Vec<String>,
        event_levels: Vec<EventLevel>,
        channels: Vec<NotificationChannelType>,
        recipients: Vec<String>,
    ) -> DomainResult<NotificationAggregate> {
        // Create notification aggregate
        let aggregate =
            NotificationAggregate::new(name, event_types, event_levels, channels, recipients)
                .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;

        // Validate according to business rules
        self.validation_spec
            .validate(aggregate.rule())
            .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;

        Ok(aggregate)
    }

    /// Process event for notifications (business logic)
    pub fn process_event_for_notifications(
        &self,
        event_id: EventId,
        event_type: &str,
        event_level: &EventLevel,
        event_metadata: &HashMap<String, String>,
        notification_rules: &mut [NotificationAggregate],
        message: String,
    ) -> DomainResult<Vec<NotificationRecord>> {
        let mut all_notifications = Vec::new();

        for rule_aggregate in notification_rules {
            // Check if rule matches the event
            if NotificationFilterSpec::matches_filters(
                rule_aggregate.rule(),
                event_type,
                event_level,
                event_metadata,
            ) {
                // Create notifications for this rule
                let notifications = rule_aggregate
                    .create_notifications(event_id.to_string(), message.clone())
                    .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;

                all_notifications.extend(notifications);
            }
        }

        Ok(all_notifications)
    }

    /// Determine notification delivery priority (business logic)
    pub fn get_delivery_priority(&self, notification: &NotificationRecord) -> u8 {
        NotificationDeliverySpec::get_channel_priority(&notification.notification_method)
    }

    /// Check if notification should be retried (business logic)
    pub fn should_retry_notification(&self, _notification: &NotificationRecord) -> bool {
        NotificationDeliverySpec::should_retry(0, 3) // Using 0 as retry_count since it's not in the struct
    }

    /// Get retry delay for notification (business logic)
    pub fn get_retry_delay(&self, _notification: &NotificationRecord) -> u64 {
        NotificationDeliverySpec::get_retry_delay(0) // Using 0 as retry_count since it's not in the struct
    }

    /// Check if notifications should be batched (business logic)
    pub fn should_batch_notifications(&self, channel: &NotificationChannelType) -> bool {
        NotificationDeliverySpec::should_batch_notifications(channel)
    }

    /// Group notifications for batching (business logic)
    pub fn group_notifications_for_batching<'a>(
        &self,
        notifications: &'a [NotificationRecord],
    ) -> HashMap<(NotificationChannelType, String), Vec<&'a NotificationRecord>> {
        let mut groups = HashMap::new();

        for notification in notifications {
            if self.should_batch_notifications(&notification.notification_method) {
                let key = (
                    notification.notification_method.clone(),
                    notification.recipient.clone(),
                );
                groups
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(notification);
            }
        }

        groups
    }

    /// Filter notifications by delivery status (business logic)
    pub fn filter_notifications_by_status<'a>(
        &self,
        notifications: &'a [NotificationRecord],
        status: &NotificationStatus,
    ) -> Vec<&'a NotificationRecord> {
        notifications
            .iter()
            .filter(|n| std::mem::discriminant(&n.status) == std::mem::discriminant(status))
            .collect()
    }

    /// Get pending notifications that need processing
    pub fn get_pending_notifications<'a>(
        &self,
        notifications: &'a [NotificationRecord],
    ) -> Vec<&'a NotificationRecord> {
        notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Pending))
            .collect()
    }

    /// Get failed notifications that can be retried
    pub fn get_retryable_notifications<'a>(
        &self,
        notifications: &'a [NotificationRecord],
    ) -> Vec<&'a NotificationRecord> {
        notifications
            .iter()
            .filter(|n| {
                matches!(n.status, NotificationStatus::Failed) && self.should_retry_notification(n)
            })
            .collect()
    }

    /// Calculate notification statistics (business intelligence)
    pub fn calculate_notification_statistics(
        &self,
        notifications: &[NotificationRecord],
    ) -> NotificationStatistics {
        let total = notifications.len();
        let pending = notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Pending))
            .count();
        let sent = notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Sent))
            .count();
        let failed = notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Failed))
            .count();
        let acknowledged = notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Acknowledged))
            .count();

        let success_rate = if total > 0 {
            (sent as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Calculate average delivery time for sent notifications
        let delivery_times: Vec<_> = notifications
            .iter()
            .filter_map(|n| {
                n.sent_at
                    .map(|sent_at| sent_at.signed_duration_since(n.created_at).num_seconds())
            })
            .collect();

        let avg_delivery_time = if !delivery_times.is_empty() {
            delivery_times.iter().sum::<i64>() as f64 / delivery_times.len() as f64
        } else {
            0.0
        };

        NotificationStatistics {
            total,
            pending,
            sent,
            failed,
            acknowledged,
            success_rate,
            avg_delivery_time_seconds: avg_delivery_time,
        }
    }

    /// Validate notification rule update (business rules)
    pub fn validate_rule_update(
        &self,
        current_rule: &NotificationRule,
        name: Option<&str>,
        channels: Option<&[NotificationChannelType]>,
        recipients: Option<&[String]>,
    ) -> DomainResult<()> {
        // Create a temporary rule for validation
        let mut temp_rule = current_rule.clone();

        if let Some(name) = name {
            temp_rule.name = name.to_string();
        }

        if let Some(channels) = channels {
            temp_rule.channels = channels.to_vec();
        }

        if let Some(recipients) = recipients {
            temp_rule.recipients = recipients.to_vec();
        }

        // Validate the updated rule
        self.validation_spec
            .validate(&temp_rule)
            .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;

        Ok(())
    }

    /// Check if notification should be suppressed due to rate limiting
    pub fn should_suppress_notification(
        &self,
        rule_id: &str,
        event_type: &str,
        last_notification_time: Option<DateTime<Utc>>,
        min_interval_seconds: u64,
    ) -> bool {
        NotificationFilterSpec::should_suppress_notification(
            rule_id,
            event_type,
            last_notification_time,
            min_interval_seconds,
        )
    }

    /// Generate notification message based on event (business logic)
    pub fn generate_notification_message(
        &self,
        event_type: &str,
        event_level: &EventLevel,
        event_source: &str,
        event_content: &str,
        channel: &NotificationChannelType,
    ) -> String {
        match channel {
            NotificationChannelType::Email => {
                format!(
                    "Event Alert: {} ({})\n\nSource: {}\nLevel: {:?}\nDetails: {}",
                    event_type, event_source, event_source, event_level, event_content
                )
            }
            NotificationChannelType::Sms => {
                format!(
                    "Alert: {} - {} ({:?})",
                    event_type, event_source, event_level
                )
            }
            NotificationChannelType::Sse => {
                format!(
                    "{{\"type\":\"{}\",\"source\":\"{}\",\"level\":\"{:?}\",\"content\":\"{}\"}}",
                    event_type, event_source, event_level, event_content
                )
            }
            NotificationChannelType::Webhook => {
                format!(
                    "{{\"event_type\":\"{}\",\"source\":\"{}\",\"level\":\"{:?}\",\"content\":\"{}\"}}",
                    event_type, event_source, event_level, event_content
                )
            }
        }
    }
}

/// Notification statistics
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

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::EventId;

    #[test]
    fn test_create_notification_rule() {
        let service = NotificationService::new();

        let aggregate = service
            .create_notification_rule(
                "Test Rule".to_string(),
                vec!["device.error".to_string()],
                vec![EventLevel::Error],
                vec![NotificationChannelType::Email],
                vec!["admin@example.com".to_string()],
            )
            .unwrap();

        assert_eq!(aggregate.rule().name, "Test Rule");
    }

    #[test]
    fn test_delivery_priority() {
        let service = NotificationService::new();

        let email_notification = NotificationRecord {
            id: "1".to_string(),
            event_id: EventId::new().to_string(),
            rule_id: "rule-1".to_string(),
            notification_method: NotificationChannelType::Email,
            recipient: "test@example.com".to_string(),
            status: NotificationStatus::Pending,
            sent_at: None,
            error_message: None,
            created_at: Utc::now(),
        };

        let sms_notification = NotificationRecord {
            notification_method: NotificationChannelType::Sms,
            ..email_notification.clone()
        };

        assert_eq!(service.get_delivery_priority(&email_notification), 3);
        assert_eq!(service.get_delivery_priority(&sms_notification), 2);
    }

    #[test]
    fn test_should_batch_notifications() {
        let service = NotificationService::new();

        assert!(service.should_batch_notifications(&NotificationChannelType::Email));
        assert!(!service.should_batch_notifications(&NotificationChannelType::Sms));
    }

    #[test]
    fn test_notification_statistics() {
        let service = NotificationService::new();

        let notifications = vec![
            NotificationRecord {
                id: "1".to_string(),
                event_id: EventId::new().to_string(),
                rule_id: "rule-1".to_string(),
                notification_method: NotificationChannelType::Email,
                recipient: "test@example.com".to_string(),
                status: NotificationStatus::Sent,
                sent_at: Some(Utc::now()),
                error_message: None,
                created_at: Utc::now(),
            },
            NotificationRecord {
                id: "2".to_string(),
                event_id: EventId::new().to_string(),
                rule_id: "rule-1".to_string(),
                notification_method: NotificationChannelType::Email,
                recipient: "test@example.com".to_string(),
                status: NotificationStatus::Failed,
                sent_at: None,
                error_message: Some("Network error".to_string()),
                created_at: Utc::now(),
            },
        ];

        let stats = service.calculate_notification_statistics(&notifications);

        assert_eq!(stats.total, 2);
        assert_eq!(stats.sent, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.success_rate, 50.0);
    }

    #[test]
    fn test_generate_notification_message() {
        let service = NotificationService::new();

        let email_message = service.generate_notification_message(
            "device.error",
            &EventLevel::Error,
            "device-1",
            "Connection lost",
            &NotificationChannelType::Email,
        );

        assert!(email_message.contains("Event Alert"));
        assert!(email_message.contains("device.error"));
        assert!(email_message.contains("Connection lost"));

        let sms_message = service.generate_notification_message(
            "device.error",
            &EventLevel::Error,
            "device-1",
            "Connection lost",
            &NotificationChannelType::Sms,
        );

        assert!(sms_message.contains("Alert"));
        assert!(sms_message.len() < email_message.len()); // SMS should be shorter
    }
}

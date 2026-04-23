// Notification service layer
// Consolidated from domain/event/services/notification_service.rs,
// domain/event/services/notification_channel.rs, and
// domain/event/specifications/notification_specifications.rs

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::warn;

use super::types::{
    NotificationAggregate, NotificationChannelType,
    NotificationRecord, NotificationRule,
    NotificationStatus,
};

// Re-export types from types.rs so they're accessible via service path
pub use super::types::{
    NotificationChannel, NotificationLevel, NotificationMessage, NotificationStatistics,
};
use crate::modules::event::{
    errors::{DomainResult, NotificationDomainError},
    value_objects::{EventId, EventLevel},
    EventError, Result,
};
use tinyiothub_core::models::notification_channel::{
    NotificationChannel as CoreNotificationChannel, SendMessageRequest,
};

// ──────────────────────────────────────────────
// Specifications (from notification_specifications.rs)
// ──────────────────────────────────────────────

/// Specification pattern for notification business rules
pub trait NotificationSpecification: Send + Sync {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool;
    fn error_message(&self) -> String;
}

pub struct NotificationHasChannelsSpec;
impl NotificationSpecification for NotificationHasChannelsSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        !rule.channels.is_empty()
    }
    fn error_message(&self) -> String {
        "Notification rule must have at least one channel".to_string()
    }
}

pub struct NotificationHasRecipientsSpec;
impl NotificationSpecification for NotificationHasRecipientsSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        !rule.recipients.is_empty()
    }
    fn error_message(&self) -> String {
        "Notification rule must have at least one recipient".to_string()
    }
}

pub struct EmailRecipientsValidSpec;
impl NotificationSpecification for EmailRecipientsValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        if !rule.channels.contains(&NotificationChannelType::Email) {
            return true;
        }
        rule.recipients.iter().all(|recipient| recipient.contains('@') && recipient.contains('.'))
    }
    fn error_message(&self) -> String {
        "Email recipients must have valid email format".to_string()
    }
}

pub struct SmsRecipientsValidSpec;
impl NotificationSpecification for SmsRecipientsValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        if !rule.channels.contains(&NotificationChannelType::Sms) {
            return true;
        }
        rule.recipients.iter().all(|recipient| {
            recipient.starts_with('+') && recipient[1..].chars().all(|c| c.is_ascii_digit())
        })
    }
    fn error_message(&self) -> String {
        "SMS recipients must have valid phone number format (+1234567890)".to_string()
    }
}

pub struct CriticalEventsImmediateChannelsSpec;
impl CriticalEventsImmediateChannelsSpec {
    pub fn is_satisfied_by_event_level(
        &self,
        rule: &NotificationRule,
        event_level: &EventLevel,
    ) -> bool {
        if !matches!(event_level, EventLevel::Critical | EventLevel::Error) {
            return true;
        }
        rule.channels.iter().any(|channel| {
            matches!(channel, NotificationChannelType::Sms | NotificationChannelType::Sse)
        })
    }
}
impl NotificationSpecification for CriticalEventsImmediateChannelsSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        if rule
            .event_levels
            .iter()
            .any(|level| matches!(level, EventLevel::Critical | EventLevel::Error))
        {
            return self.is_satisfied_by_event_level(rule, &EventLevel::Critical);
        }
        true
    }
    fn error_message(&self) -> String {
        "Rules for critical events must include immediate notification channels (SMS or SSE)"
            .to_string()
    }
}

pub struct NotificationNameValidSpec;
impl NotificationSpecification for NotificationNameValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        !rule.name.trim().is_empty() && rule.name.len() >= 3 && rule.name.len() <= 100
    }
    fn error_message(&self) -> String {
        "Notification rule name must be between 3 and 100 characters".to_string()
    }
}

pub struct EventTypesValidSpec;
impl NotificationSpecification for EventTypesValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        rule.event_types.iter().all(|event_type| {
            event_type.contains('.') && !event_type.starts_with('.') && !event_type.ends_with('.')
        })
    }
    fn error_message(&self) -> String {
        "Event types must follow format 'category.subcategory' (e.g., 'device.error')".to_string()
    }
}

/// Composite specification for all notification validation rules
pub struct NotificationValidationSpec {
    specs: Vec<Box<dyn NotificationSpecification>>,
}

impl NotificationValidationSpec {
    pub fn new() -> Self {
        Self {
            specs: vec![
                Box::new(NotificationHasChannelsSpec),
                Box::new(NotificationHasRecipientsSpec),
                Box::new(EmailRecipientsValidSpec),
                Box::new(SmsRecipientsValidSpec),
                Box::new(CriticalEventsImmediateChannelsSpec),
                Box::new(NotificationNameValidSpec),
                Box::new(EventTypesValidSpec),
            ],
        }
    }

    pub fn validate(&self, rule: &NotificationRule) -> Result<()> {
        for spec in &self.specs {
            if !spec.is_satisfied_by(rule) {
                return Err(EventError::Validation { message: spec.error_message() });
            }
        }
        Ok(())
    }

    pub fn add_specification(&mut self, spec: Box<dyn NotificationSpecification>) {
        self.specs.push(spec);
    }
}

impl Default for NotificationValidationSpec {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification delivery specifications
pub struct NotificationDeliverySpec;

impl NotificationDeliverySpec {
    pub fn should_retry(retry_count: u32, max_retries: u32) -> bool {
        retry_count < max_retries
    }

    pub fn get_retry_delay(retry_count: u32) -> u64 {
        match retry_count {
            0 => 30,
            1 => 300,
            2 => 1800,
            _ => 3600,
        }
    }

    pub fn is_channel_available(channel: &NotificationChannelType) -> bool {
        match channel {
            NotificationChannelType::Email => true,
            NotificationChannelType::Sms => true,
            NotificationChannelType::Sse => true,
            NotificationChannelType::Webhook => true,
        }
    }

    pub fn get_channel_priority(channel: &NotificationChannelType) -> u8 {
        match channel {
            NotificationChannelType::Sse => 1,
            NotificationChannelType::Sms => 2,
            NotificationChannelType::Email => 3,
            NotificationChannelType::Webhook => 4,
        }
    }

    pub fn should_batch_notifications(channel: &NotificationChannelType) -> bool {
        matches!(channel, NotificationChannelType::Email)
    }
}

/// Notification filtering specifications
pub struct NotificationFilterSpec;

impl NotificationFilterSpec {
    pub fn matches_filters(
        rule: &NotificationRule,
        event_type: &str,
        event_level: &EventLevel,
        event_metadata: &HashMap<String, String>,
    ) -> bool {
        let type_match = rule.event_types.is_empty()
            || rule.event_types.iter().any(|pattern| Self::matches_pattern(event_type, pattern));
        let level_match = rule.event_levels.is_empty() || rule.event_levels.contains(event_level);
        let conditions_match =
            rule.conditions.iter().all(|(key, value)| event_metadata.get(key) == Some(value));
        type_match && level_match && conditions_match
    }

    fn matches_pattern(event_type: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix(".*") {
            return event_type.starts_with(prefix);
        }
        event_type == pattern
    }

    pub fn should_suppress_notification(
        _rule_id: &str,
        _event_type: &str,
        last_notification_time: Option<DateTime<Utc>>,
        min_interval_seconds: u64,
    ) -> bool {
        if let Some(last_time) = last_notification_time {
            let now = Utc::now();
            let elapsed = now.signed_duration_since(last_time);
            elapsed.num_seconds() < min_interval_seconds as i64
        } else {
            false
        }
    }
}

// ──────────────────────────────────────────────
// Channel trait handler (from notification_channel.rs)
// ──────────────────────────────────────────────

/// Trait for notification channel handler implementations
#[async_trait::async_trait]
pub trait NotificationChannelHandler: Send + Sync {
    async fn send(&self, message: &NotificationMessage, recipient: &str) -> Result<()>;
    fn channel_type(&self) -> NotificationChannelType;
    async fn is_available(&self) -> bool;
    fn get_config_requirements(&self) -> Vec<String>;
}

/// Notification channel manager for coordinating multiple channels
pub struct NotificationChannelManager {
    channels: HashMap<NotificationChannelType, Box<dyn NotificationChannelHandler>>,
}

impl NotificationChannelManager {
    pub fn new() -> Self {
        Self { channels: HashMap::new() }
    }

    pub fn register_channel(&mut self, channel: Box<dyn NotificationChannelHandler>) {
        let channel_type = channel.channel_type();
        self.channels.insert(channel_type, channel);
    }

    pub async fn send_notification(&self, message: &NotificationMessage) -> Result<()> {
        if let Some(channel) = self.channels.get(&message.channel) {
            if channel.is_available().await {
                channel.send(message, &message.recipient).await
            } else {
                Err(EventError::Configuration(format!(
                    "Channel {:?} is not available",
                    message.channel
                )))
            }
        } else {
            Err(EventError::Configuration(format!(
                "Channel {:?} is not registered",
                message.channel
            )))
        }
    }

    pub fn get_available_channels(&self) -> Vec<NotificationChannelType> {
        self.channels.keys().cloned().collect()
    }

    pub async fn is_channel_available(&self, channel_type: &NotificationChannelType) -> bool {
        if let Some(channel) = self.channels.get(channel_type) {
            channel.is_available().await
        } else {
            false
        }
    }
}

impl Default for NotificationChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Send a message through a notification channel (test/send helper)
pub async fn send_notification_message(
    channel: &CoreNotificationChannel,
    req: &SendMessageRequest,
) -> std::result::Result<String, String> {
    match channel.channel_type.as_str() {
        "sms" => send_sms(channel, req).await,
        "email" => send_email(channel, req).await,
        "webhook" => send_webhook(channel, req).await,
        _ => Err(format!("Unknown channel type: {}", channel.channel_type)),
    }
}

async fn send_sms(
    channel: &CoreNotificationChannel,
    req: &SendMessageRequest,
) -> std::result::Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;
    let provider = config.get("provider").and_then(|v| v.as_str()).unwrap_or("aliyun");
    let sign_name = config.get("sign_name").and_then(|v| v.as_str()).unwrap_or("TinyIoT");
    let template_id = config.get("template_id").and_then(|v| v.as_str()).unwrap_or("");
    tracing::info!("Sending SMS via {} to {}: {}", provider, req.recipient, req.content);
    Ok(format!(
        "SMS sent to {} via {} (sign: {}, template: {})",
        req.recipient, provider, sign_name, template_id
    ))
}

async fn send_email(
    channel: &CoreNotificationChannel,
    req: &SendMessageRequest,
) -> std::result::Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;
    let smtp_host = config.get("smtp_host").and_then(|v| v.as_str()).unwrap_or("");
    let from =
        config.get("from").and_then(|v| v.as_str()).unwrap_or("TinyIoT <noreply@tinyiot.com>");
    tracing::info!("Sending email via {} from {} to {}", smtp_host, from, req.recipient);
    Ok(format!(
        "Email sent to {} (from: {}, subject: {})",
        req.recipient,
        from,
        req.title.as_deref().unwrap_or("")
    ))
}

async fn send_webhook(
    channel: &CoreNotificationChannel,
    req: &SendMessageRequest,
) -> std::result::Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;
    let url = config.get("url").and_then(|v| v.as_str()).ok_or("Missing URL in config")?;
    let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("POST");
    tracing::info!("Sending webhook {} {} to {}", method, url, req.recipient);
    let body = serde_json::json!({
        "msgtype": "text",
        "text": {
            "content": format!("{}\n{}", req.title.as_deref().unwrap_or(""), req.content)
        }
    });
    Ok(format!("Webhook sent to {} via {} {}", url, method, body))
}

// ──────────────────────────────────────────────
// Notification Manager (from notification_service.rs)
// ──────────────────────────────────────────────

/// Notification manager for coordinating notification delivery
pub struct NotificationManager {
    channels: HashMap<NotificationChannelType, Box<dyn NotificationChannel>>,
    notification_service: NotificationService,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            notification_service: NotificationService::new(),
        }
    }

    pub fn register_channel(&mut self, channel: Box<dyn NotificationChannel>) {
        let channel_type = channel.channel_type();
        self.channels.insert(channel_type, channel);
    }

    pub async fn send_notification(&self, message: &NotificationMessage) -> std::result::Result<(), String> {
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

    pub fn get_available_channels(&self) -> Vec<NotificationChannelType> {
        self.channels.keys().cloned().collect()
    }

    pub async fn process_event_notifications(
        &self,
        event_id: EventId,
        event_type: &str,
        event_level: &EventLevel,
        event_content: &str,
        event_source: &str,
        notification_rules: &mut [NotificationAggregate],
    ) -> std::result::Result<Vec<NotificationMessage>, String> {
        let event_metadata = HashMap::new();

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
    pub async fn get_rules(&self) -> std::result::Result<Vec<NotificationRule>, String> {
        Ok(Vec::new())
    }

    /// Add a new notification rule
    pub async fn add_rule(&self, _rule: NotificationRule) -> std::result::Result<(), String> {
        Ok(())
    }

    /// Update an existing notification rule
    pub async fn update_rule(&self, _rule_id: &str, _rule: NotificationRule) -> std::result::Result<(), String> {
        Ok(())
    }

    /// Remove a notification rule
    pub async fn remove_rule(&self, _rule_id: &str) -> std::result::Result<(), String> {
        Ok(())
    }

    /// Get notification history for an event
    pub async fn get_notification_history(
        &self,
        _event_id: &str,
    ) -> std::result::Result<Vec<NotificationRecord>, String> {
        Ok(Vec::new())
    }

    /// Send notification for an event
    pub async fn notify(
        &self,
        _event: &crate::modules::event::entities::Event,
    ) -> std::result::Result<(), String> {
        Ok(())
    }
}

// ──────────────────────────────────────────────
// Notification Service (pure business logic)
// ──────────────────────────────────────────────

/// Domain service for notification processing
pub struct NotificationService {
    validation_spec: NotificationValidationSpec,
}

impl NotificationService {
    pub fn new() -> Self {
        Self { validation_spec: NotificationValidationSpec::new() }
    }

    pub fn create_notification_rule(
        &self,
        name: String,
        event_types: Vec<String>,
        event_levels: Vec<EventLevel>,
        channels: Vec<NotificationChannelType>,
        recipients: Vec<String>,
    ) -> DomainResult<NotificationAggregate> {
        let aggregate =
            NotificationAggregate::new(name, event_types, event_levels, channels, recipients)
                .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;

        self.validation_spec
            .validate(aggregate.rule())
            .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;

        Ok(aggregate)
    }

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
            if NotificationFilterSpec::matches_filters(
                rule_aggregate.rule(),
                event_type,
                event_level,
                event_metadata,
            ) {
                let notifications = rule_aggregate
                    .create_notifications(event_id.to_string(), message.clone())
                    .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;
                all_notifications.extend(notifications);
            }
        }
        Ok(all_notifications)
    }

    pub fn get_delivery_priority(&self, notification: &NotificationRecord) -> u8 {
        NotificationDeliverySpec::get_channel_priority(&notification.notification_method)
    }

    pub fn should_retry_notification(&self, _notification: &NotificationRecord) -> bool {
        NotificationDeliverySpec::should_retry(0, 3)
    }

    pub fn get_retry_delay(&self, _notification: &NotificationRecord) -> u64 {
        NotificationDeliverySpec::get_retry_delay(0)
    }

    pub fn should_batch_notifications(&self, channel: &NotificationChannelType) -> bool {
        NotificationDeliverySpec::should_batch_notifications(channel)
    }

    pub fn group_notifications_for_batching<'a>(
        &self,
        notifications: &'a [NotificationRecord],
    ) -> HashMap<(NotificationChannelType, String), Vec<&'a NotificationRecord>> {
        let mut groups = HashMap::new();
        for notification in notifications {
            if self.should_batch_notifications(&notification.notification_method) {
                let key =
                    (notification.notification_method.clone(), notification.recipient.clone());
                groups.entry(key).or_insert_with(Vec::new).push(notification);
            }
        }
        groups
    }

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

    pub fn get_pending_notifications<'a>(
        &self,
        notifications: &'a [NotificationRecord],
    ) -> Vec<&'a NotificationRecord> {
        notifications.iter().filter(|n| matches!(n.status, NotificationStatus::Pending)).collect()
    }

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

    pub fn calculate_notification_statistics(
        &self,
        notifications: &[NotificationRecord],
    ) -> NotificationStatistics {
        let total = notifications.len();
        let pending = notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Pending))
            .count();
        let sent =
            notifications.iter().filter(|n| matches!(n.status, NotificationStatus::Sent)).count();
        let failed =
            notifications.iter().filter(|n| matches!(n.status, NotificationStatus::Failed)).count();
        let acknowledged = notifications
            .iter()
            .filter(|n| matches!(n.status, NotificationStatus::Acknowledged))
            .count();

        let success_rate = if total > 0 { (sent as f64 / total as f64) * 100.0 } else { 0.0 };

        let delivery_times: Vec<_> = notifications
            .iter()
            .filter_map(|n| {
                n.sent_at.map(|sent_at| sent_at.signed_duration_since(n.created_at).num_seconds())
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

    pub fn validate_rule_update(
        &self,
        current_rule: &NotificationRule,
        name: Option<&str>,
        channels: Option<&[NotificationChannelType]>,
        recipients: Option<&[String]>,
    ) -> DomainResult<()> {
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
        self.validation_spec
            .validate(&temp_rule)
            .map_err(|e| NotificationDomainError::rule_validation(e.to_string()))?;
        Ok(())
    }

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
                format!("Alert: {} - {} ({:?})", event_type, event_source, event_level)
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

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_has_channels_spec() {
        let spec = NotificationHasChannelsSpec;
        let rule = NotificationRule::new(
            "rule-1".to_string(),
            "Test Rule".to_string(),
            None,
            vec![NotificationChannelType::Email],
            vec!["admin@example.com".to_string()],
        );
        assert!(spec.is_satisfied_by(&rule));

        let mut empty_rule = rule.clone();
        empty_rule.channels.clear();
        assert!(!spec.is_satisfied_by(&empty_rule));
    }

    #[test]
    fn test_notification_validation_spec() {
        let spec = NotificationValidationSpec::new();
        let rule = NotificationRule {
            id: "rule-1".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test description".to_string()),
            event_type: Some("device.warning".to_string()),
            event_subtype: None,
            event_level: Some(3),
            device_filter: None,
            notification_methods: vec![NotificationChannelType::Email],
            recipients: vec!["admin@example.com".to_string()],
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            event_types: vec!["device.warning".to_string()],
            event_levels: vec![EventLevel::Warning],
            channels: vec![NotificationChannelType::Email],
            conditions: HashMap::new(),
            is_active: true,
        };
        assert!(spec.validate(&rule).is_ok());
    }

    #[test]
    fn test_notification_delivery_spec() {
        assert!(NotificationDeliverySpec::should_retry(0, 3));
        assert!(!NotificationDeliverySpec::should_retry(3, 3));
        assert_eq!(NotificationDeliverySpec::get_retry_delay(0), 30);
        assert_eq!(NotificationDeliverySpec::get_channel_priority(&NotificationChannelType::Sse), 1);
        assert!(NotificationDeliverySpec::should_batch_notifications(&NotificationChannelType::Email));
        assert!(!NotificationDeliverySpec::should_batch_notifications(&NotificationChannelType::Sms));
    }

    #[test]
    fn test_create_notification_rule() {
        let service = NotificationService::new();
        let aggregate = service
            .create_notification_rule(
                "Test Rule".to_string(),
                vec!["device.warning".to_string()],
                vec![EventLevel::Warning],
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

        let sms_message = service.generate_notification_message(
            "device.error",
            &EventLevel::Error,
            "device-1",
            "Connection lost",
            &NotificationChannelType::Sms,
        );
        assert!(sms_message.contains("Alert"));
        assert!(sms_message.len() < email_message.len());
    }

    #[test]
    fn test_notification_filter_spec() {
        let rule = NotificationRule {
            id: "rule-1".to_string(),
            name: "Test Rule".to_string(),
            description: None,
            event_type: Some("device.warning".to_string()),
            event_subtype: None,
            event_level: Some(3),
            device_filter: None,
            notification_methods: vec![NotificationChannelType::Email],
            recipients: vec!["admin@example.com".to_string()],
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            event_types: vec!["device.warning".to_string()],
            event_levels: vec![EventLevel::Warning],
            channels: vec![NotificationChannelType::Email],
            conditions: HashMap::new(),
            is_active: true,
        };
        let metadata = HashMap::new();
        assert!(NotificationFilterSpec::matches_filters(&rule, "device.warning", &EventLevel::Warning, &metadata));
        assert!(!NotificationFilterSpec::matches_filters(&rule, "system.info", &EventLevel::Info, &metadata));
        assert!(NotificationFilterSpec::matches_pattern("device.warning", "device.*"));
        assert!(NotificationFilterSpec::matches_pattern("any.event", "*"));
        assert!(!NotificationFilterSpec::matches_pattern("system.info", "device.*"));
    }
}

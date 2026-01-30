use crate::domain::event::{
    aggregates::{NotificationChannelType, NotificationRule},
    value_objects::EventLevel,
    EventError, Result,
};
use std::collections::HashMap;

/// Specification pattern for notification business rules
pub trait NotificationSpecification: Send + Sync {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool;
    fn error_message(&self) -> String;
}

/// Notification rule must have at least one channel
pub struct NotificationHasChannelsSpec;

impl NotificationSpecification for NotificationHasChannelsSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        !rule.channels.is_empty()
    }

    fn error_message(&self) -> String {
        "Notification rule must have at least one channel".to_string()
    }
}

/// Notification rule must have at least one recipient
pub struct NotificationHasRecipientsSpec;

impl NotificationSpecification for NotificationHasRecipientsSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        !rule.recipients.is_empty()
    }

    fn error_message(&self) -> String {
        "Notification rule must have at least one recipient".to_string()
    }
}

/// Email recipients must have valid email format
pub struct EmailRecipientsValidSpec;

impl NotificationSpecification for EmailRecipientsValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        if !rule.channels.contains(&NotificationChannelType::Email) {
            return true; // Skip validation if email channel not used
        }

        rule.recipients.iter().all(|recipient| {
            // Simple email validation
            recipient.contains('@') && recipient.contains('.')
        })
    }

    fn error_message(&self) -> String {
        "Email recipients must have valid email format".to_string()
    }
}

/// SMS recipients must have valid phone format
pub struct SmsRecipientsValidSpec;

impl NotificationSpecification for SmsRecipientsValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        if !rule.channels.contains(&NotificationChannelType::Sms) {
            return true; // Skip validation if SMS channel not used
        }

        rule.recipients.iter().all(|recipient| {
            // Simple phone validation - starts with + and contains only digits
            recipient.starts_with('+') && recipient[1..].chars().all(|c| c.is_ascii_digit())
        })
    }

    fn error_message(&self) -> String {
        "SMS recipients must have valid phone number format (+1234567890)".to_string()
    }
}

/// Critical events must have immediate notification channels
pub struct CriticalEventsImmediateChannelsSpec;

impl CriticalEventsImmediateChannelsSpec {
    pub fn is_satisfied_by_event_level(
        &self,
        rule: &NotificationRule,
        event_level: &EventLevel,
    ) -> bool {
        if !matches!(event_level, EventLevel::Critical | EventLevel::Error) {
            return true; // Non-critical events can use any channel
        }

        // Critical events should have at least one immediate channel (SMS or SSE)
        rule.channels.iter().any(|channel| {
            matches!(
                channel,
                NotificationChannelType::Sms | NotificationChannelType::Sse
            )
        })
    }
}

impl NotificationSpecification for CriticalEventsImmediateChannelsSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        // Check if rule covers critical events
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

/// Notification rule name must be unique and descriptive
pub struct NotificationNameValidSpec;

impl NotificationSpecification for NotificationNameValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        !rule.name.trim().is_empty() && rule.name.len() >= 3 && rule.name.len() <= 100
    }

    fn error_message(&self) -> String {
        "Notification rule name must be between 3 and 100 characters".to_string()
    }
}

/// Event types must be valid format
pub struct EventTypesValidSpec;

impl NotificationSpecification for EventTypesValidSpec {
    fn is_satisfied_by(&self, rule: &NotificationRule) -> bool {
        rule.event_types.iter().all(|event_type| {
            // Event types should follow pattern: category.subcategory
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
                return Err(EventError::Validation {
                    message: spec.error_message(),
                });
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
    /// Check if notification should be retried
    pub fn should_retry(retry_count: u32, max_retries: u32) -> bool {
        retry_count < max_retries
    }

    /// Get retry delay in seconds (exponential backoff)
    pub fn get_retry_delay(retry_count: u32) -> u64 {
        match retry_count {
            0 => 30,   // 30 seconds
            1 => 300,  // 5 minutes
            2 => 1800, // 30 minutes
            _ => 3600, // 1 hour
        }
    }

    /// Check if notification channel is available
    pub fn is_channel_available(channel: &NotificationChannelType) -> bool {
        match channel {
            NotificationChannelType::Email => true,   // Always available
            NotificationChannelType::Sms => true,     // Always available
            NotificationChannelType::Sse => true,     // Always available
            NotificationChannelType::Webhook => true, // Always available
        }
    }

    /// Get channel priority (1 = highest, 4 = lowest)
    pub fn get_channel_priority(channel: &NotificationChannelType) -> u8 {
        match channel {
            NotificationChannelType::Sse => 1,     // Immediate
            NotificationChannelType::Sms => 2,     // Fast
            NotificationChannelType::Email => 3,   // Standard
            NotificationChannelType::Webhook => 4, // Lowest
        }
    }

    /// Check if notification should be batched
    pub fn should_batch_notifications(channel: &NotificationChannelType) -> bool {
        matches!(channel, NotificationChannelType::Email)
    }
}

/// Notification filtering specifications
pub struct NotificationFilterSpec;

impl NotificationFilterSpec {
    /// Check if event matches notification rule filters
    pub fn matches_filters(
        rule: &NotificationRule,
        event_type: &str,
        event_level: &EventLevel,
        event_metadata: &HashMap<String, String>,
    ) -> bool {
        // Check event type filter
        let type_match = rule.event_types.is_empty()
            || rule
                .event_types
                .iter()
                .any(|pattern| Self::matches_pattern(event_type, pattern));

        // Check event level filter
        let level_match = rule.event_levels.is_empty() || rule.event_levels.contains(event_level);

        // Check custom conditions
        let conditions_match = rule
            .conditions
            .iter()
            .all(|(key, value)| event_metadata.get(key) == Some(value));

        type_match && level_match && conditions_match
    }

    /// Pattern matching for event types (supports wildcards)
    fn matches_pattern(event_type: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if let Some(prefix) = pattern.strip_suffix(".*") {
            return event_type.starts_with(prefix);
        }

        event_type == pattern
    }

    /// Check if notification should be suppressed (rate limiting)
    pub fn should_suppress_notification(
        _rule_id: &str,
        _event_type: &str,
        last_notification_time: Option<chrono::DateTime<chrono::Utc>>,
        min_interval_seconds: u64,
    ) -> bool {
        if let Some(last_time) = last_notification_time {
            let now = chrono::Utc::now();
            let elapsed = now.signed_duration_since(last_time);
            elapsed.num_seconds() < min_interval_seconds as i64
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_valid_notification_rule() -> NotificationRule {
        NotificationRule {
            id: "rule-1".to_string(),
            name: "Test Rule".to_string(),
            description: Some("Test description".to_string()),
            event_type: Some("device.error".to_string()),
            event_subtype: None,
            event_level: Some(3), // Error level
            device_filter: None,
            notification_methods: vec![NotificationChannelType::Email],
            recipients: vec!["admin@example.com".to_string()],
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            // Legacy compatibility fields
            event_types: vec!["device.error".to_string()],
            event_levels: vec![EventLevel::Error],
            channels: vec![NotificationChannelType::Email],
            conditions: HashMap::new(),
            is_active: true,
        }
    }

    #[test]
    fn test_notification_has_channels_spec() {
        let spec = NotificationHasChannelsSpec;
        let rule = create_valid_notification_rule();
        assert!(spec.is_satisfied_by(&rule));

        let mut empty_rule = rule.clone();
        empty_rule.channels.clear();
        assert!(!spec.is_satisfied_by(&empty_rule));
    }

    #[test]
    fn test_notification_has_recipients_spec() {
        let spec = NotificationHasRecipientsSpec;
        let rule = create_valid_notification_rule();
        assert!(spec.is_satisfied_by(&rule));

        let mut empty_rule = rule.clone();
        empty_rule.recipients.clear();
        assert!(!spec.is_satisfied_by(&empty_rule));
    }

    #[test]
    fn test_email_recipients_valid_spec() {
        let spec = EmailRecipientsValidSpec;
        let rule = create_valid_notification_rule();
        assert!(spec.is_satisfied_by(&rule));

        let mut invalid_rule = rule.clone();
        invalid_rule.recipients = vec!["invalid-email".to_string()];
        assert!(!spec.is_satisfied_by(&invalid_rule));
    }

    #[test]
    fn test_sms_recipients_valid_spec() {
        let spec = SmsRecipientsValidSpec;

        let mut sms_rule = create_valid_notification_rule();
        sms_rule.channels = vec![NotificationChannelType::Sms];
        sms_rule.recipients = vec!["+1234567890".to_string()];
        assert!(spec.is_satisfied_by(&sms_rule));

        sms_rule.recipients = vec!["invalid-phone".to_string()];
        assert!(!spec.is_satisfied_by(&sms_rule));
    }

    #[test]
    fn test_critical_events_immediate_channels_spec() {
        let spec = CriticalEventsImmediateChannelsSpec;

        let mut critical_rule = create_valid_notification_rule();
        critical_rule.event_levels = vec![EventLevel::Critical];
        critical_rule.channels = vec![NotificationChannelType::Email];
        assert!(!spec.is_satisfied_by(&critical_rule));

        critical_rule.channels = vec![NotificationChannelType::Sms];
        assert!(spec.is_satisfied_by(&critical_rule));
    }

    #[test]
    fn test_notification_validation_spec() {
        let spec = NotificationValidationSpec::new();
        let rule = create_valid_notification_rule();
        assert!(spec.validate(&rule).is_ok());
    }

    #[test]
    fn test_notification_delivery_spec() {
        assert!(NotificationDeliverySpec::should_retry(0, 3));
        assert!(!NotificationDeliverySpec::should_retry(3, 3));

        assert_eq!(NotificationDeliverySpec::get_retry_delay(0), 30);
        assert_eq!(NotificationDeliverySpec::get_retry_delay(1), 300);

        assert!(NotificationDeliverySpec::is_channel_available(
            &NotificationChannelType::Email
        ));

        assert_eq!(
            NotificationDeliverySpec::get_channel_priority(&NotificationChannelType::Sse),
            1
        );
        assert_eq!(
            NotificationDeliverySpec::get_channel_priority(&NotificationChannelType::Email),
            3
        );

        assert!(NotificationDeliverySpec::should_batch_notifications(
            &NotificationChannelType::Email
        ));
        assert!(!NotificationDeliverySpec::should_batch_notifications(
            &NotificationChannelType::Sms
        ));
    }

    #[test]
    fn test_notification_filter_spec() {
        let rule = create_valid_notification_rule();
        let metadata = HashMap::new();

        assert!(NotificationFilterSpec::matches_filters(
            &rule,
            "device.error",
            &EventLevel::Error,
            &metadata
        ));

        assert!(!NotificationFilterSpec::matches_filters(
            &rule,
            "system.info",
            &EventLevel::Info,
            &metadata
        ));

        // Test pattern matching
        assert!(NotificationFilterSpec::matches_pattern(
            "device.error",
            "device.*"
        ));
        assert!(NotificationFilterSpec::matches_pattern("any.event", "*"));
        assert!(!NotificationFilterSpec::matches_pattern(
            "system.info",
            "device.*"
        ));
    }
}

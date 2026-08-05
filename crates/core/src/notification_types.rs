// Shared notification value types
// Sunk from cloud/src/modules/notification/types.rs to cut the event→notification
// edge (P4.0-Task13). Pure value types only — no domain service logic.

use serde::{Deserialize, Serialize};

use crate::models::event::EventLevel;

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

/// Slim read-only view of a notification rule, carrying exactly the fields
/// the event domain needs for rule matching. The owning `NotificationAggregate`
/// (with its full logic) stays in the notification module; conversion happens
/// at the notify-side call boundary via `NotificationAggregate::rule_ref()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRuleRef {
    pub id: String,
    pub enabled: bool,
    pub event_type: Option<String>,
    pub event_level: Option<i32>,
}

impl NotificationRuleRef {
    /// Pure matching logic identical to `NotificationAggregate::matches_event`.
    pub fn matches_event(&self, event_type: &str, event_level: &EventLevel) -> bool {
        if !self.enabled {
            return false;
        }
        let type_match = self.event_type.is_none() || self.event_type.as_deref() == Some(event_type);
        let level_match = self.event_level.is_none() || self.event_level == Some(*event_level as i32);
        type_match && level_match
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_type_round_trip() {
        for ch in [
            NotificationChannelType::Email,
            NotificationChannelType::Sms,
            NotificationChannelType::Sse,
            NotificationChannelType::Webhook,
        ] {
            assert_eq!(NotificationChannelType::parse_str(ch.as_str()), Some(ch));
        }
        assert_eq!(NotificationChannelType::parse_str("nope"), None);
    }

    #[test]
    fn rule_ref_matches_event() {
        let rule = NotificationRuleRef {
            id: "r1".to_string(),
            enabled: true,
            event_type: Some("device".to_string()),
            event_level: Some(EventLevel::Error as i32),
        };
        assert!(rule.matches_event("device", &EventLevel::Error));
        assert!(!rule.matches_event("system", &EventLevel::Error));
        assert!(!rule.matches_event("device", &EventLevel::Info));

        let wildcard = NotificationRuleRef {
            id: "r2".to_string(),
            enabled: true,
            event_type: None,
            event_level: None,
        };
        assert!(wildcard.matches_event("anything", &EventLevel::Debug));

        let disabled = NotificationRuleRef {
            enabled: false,
            ..wildcard.clone()
        };
        assert!(!disabled.matches_event("anything", &EventLevel::Debug));
    }
}

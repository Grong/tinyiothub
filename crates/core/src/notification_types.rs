// Shared notification value types
// Sunk from cloud/src/modules/notification/types.rs to cut the event→notification
// edge (P4.0-Task13). Pure value types only — no domain service logic.
//
// P4-Task18 (F1 resolution): the slim `NotificationRuleRef` view and its
// simplified `matches_event` were removed. They were dead code — the
// production matching path is `NotificationFilterSpec::matches_filters` in
// the notification module (wildcards, multiple types/levels, metadata
// conditions), and nothing called the event-side consumer. Rule matching
// lives solely in the notify domain.

use serde::{Deserialize, Serialize};

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
}

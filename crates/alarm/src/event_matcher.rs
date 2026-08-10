// Event-based alarm matching — rule_type='event'

use tinyiothub_core::models::event::EventLevel;

/// Condition config for rule_type='event' alarm rules.
///
/// Stored as JSON in the `condition_config` column of `device_alarm_rules`.
/// Example: `{"eventName": "temp_high", "minLevel": "warning"}`
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAlarmCondition {
    /// Event name to match against `ThingEventInput::event_name`
    pub event_name: String,
    /// Minimum level required for this alarm to trigger.
    /// Valid values: "info", "warning", "error", "critical"
    pub min_level: String,
}

impl EventAlarmCondition {
    /// Check whether an event matches this alarm condition.
    ///
    /// Returns `true` when `event_name` matches **and** the event level
    /// is at or above the configured minimum.
    pub fn matches(&self, event_name: &str, level: &EventLevel) -> bool {
        if self.event_name != event_name {
            return false;
        }
        let min_level_int = match self.min_level.as_str() {
            "info" => 2,
            "warning" => 3,
            "error" => 4,
            "critical" => 5,
            _ => 5, // unknown → highest (won't match anything below critical)
        };
        level.to_numeric() >= min_level_int
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_alarm_matches() {
        let cond = EventAlarmCondition {
            event_name: "temp_high".into(),
            min_level: "warning".into(),
        };
        assert!(cond.matches("temp_high", &EventLevel::Warning));
        assert!(cond.matches("temp_high", &EventLevel::Critical));
        assert!(!cond.matches("temp_low", &EventLevel::Critical));
        assert!(!cond.matches("temp_high", &EventLevel::Info));
    }

    #[test]
    fn test_event_alarm_min_level_error() {
        let cond = EventAlarmCondition {
            event_name: "overheat".into(),
            min_level: "error".into(),
        };
        assert!(cond.matches("overheat", &EventLevel::Error));
        assert!(cond.matches("overheat", &EventLevel::Critical));
        assert!(!cond.matches("overheat", &EventLevel::Warning));
        assert!(!cond.matches("overheat", &EventLevel::Info));
    }

    #[test]
    fn test_event_alarm_min_level_info_matches_all() {
        let cond = EventAlarmCondition {
            event_name: "status_update".into(),
            min_level: "info".into(),
        };
        assert!(cond.matches("status_update", &EventLevel::Info));
        assert!(cond.matches("status_update", &EventLevel::Critical));
    }

    #[test]
    fn test_event_alarm_different_name_no_match() {
        let cond = EventAlarmCondition {
            event_name: "door_open".into(),
            min_level: "warning".into(),
        };
        assert!(!cond.matches("door_closed", &EventLevel::Critical));
    }

    #[test]
    fn test_event_alarm_unknown_min_level() {
        let cond = EventAlarmCondition {
            event_name: "test".into(),
            min_level: "unknown".into(),
        };
        // Unknown min_level maps to 5 (critical), so only critical matches
        assert!(cond.matches("test", &EventLevel::Critical));
        assert!(!cond.matches("test", &EventLevel::Error));
    }

    #[test]
    fn test_deserialize_event_alarm_condition() {
        let json = r#"{"eventName": "temp_high", "minLevel": "warning"}"#;
        let cond: EventAlarmCondition = serde_json::from_str(json).unwrap();
        assert_eq!(cond.event_name, "temp_high");
        assert_eq!(cond.min_level, "warning");
    }
}

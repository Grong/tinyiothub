use serde::{Deserialize, Serialize};

/// Event severity level value object with numeric weights
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventLevel {
    /// Critical failures requiring immediate attention (weight: 5)
    Critical = 5,
    /// Errors that need attention (weight: 4)
    Error = 4,
    /// Warnings about potential issues (weight: 3)
    Warning = 3,
    /// Informational messages (weight: 2)
    Info = 2,
    /// Debug information (weight: 1)
    Debug = 1,
}

impl EventLevel {
    /// Get numeric weight for sorting and filtering
    pub fn weight(&self) -> u8 {
        *self as u8
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            EventLevel::Critical => "critical",
            EventLevel::Error => "error",
            EventLevel::Warning => "warning",
            EventLevel::Info => "info",
            EventLevel::Debug => "debug",
        }
    }

    /// Parse from string (for repository reconstruction)
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(EventLevel::Critical),
            "error" => Ok(EventLevel::Error),
            "warning" => Ok(EventLevel::Warning),
            "info" => Ok(EventLevel::Info),
            "debug" => Ok(EventLevel::Debug),
            _ => Err(format!("Unknown event level: {}", s)),
        }
    }

    /// Parse from numeric level (for backward compatibility)
    pub fn from_numeric(level: i32) -> Result<Self, String> {
        match level {
            5 => Ok(EventLevel::Critical),
            4 => Ok(EventLevel::Error),
            3 => Ok(EventLevel::Warning),
            2 => Ok(EventLevel::Info),
            1 => Ok(EventLevel::Debug),
            _ => Err(format!("Invalid event level: {}", level)),
        }
    }

    /// Convert to numeric level (for backward compatibility)
    pub fn to_numeric(&self) -> i32 {
        *self as i32
    }

    /// Check if this level should trigger notifications
    pub fn should_notify(&self) -> bool {
        matches!(self, EventLevel::Critical | EventLevel::Error)
    }

    /// Check if this level should update real-time status
    pub fn should_update_real_time_status(&self) -> bool {
        matches!(self, EventLevel::Critical | EventLevel::Error | EventLevel::Warning)
    }
}

impl std::fmt::Display for EventLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_level_conversion() {
        assert_eq!(EventLevel::Critical.weight(), 5);
        assert_eq!(EventLevel::Debug.weight(), 1);

        assert_eq!(EventLevel::from_numeric(5).unwrap(), EventLevel::Critical);
        assert_eq!(EventLevel::from_str("error").unwrap(), EventLevel::Error);

        assert_eq!(EventLevel::Critical.to_numeric(), 5);
        assert_eq!(EventLevel::Debug.as_str(), "debug");
    }

    #[test]
    fn test_event_level_ordering() {
        assert!(EventLevel::Critical > EventLevel::Error);
        assert!(EventLevel::Error > EventLevel::Warning);
        assert!(EventLevel::Warning > EventLevel::Info);
        assert!(EventLevel::Info > EventLevel::Debug);
    }

    #[test]
    fn test_business_rules() {
        assert!(EventLevel::Critical.should_notify());
        assert!(EventLevel::Error.should_notify());
        assert!(!EventLevel::Info.should_notify());

        assert!(EventLevel::Critical.should_update_real_time_status());
        assert!(EventLevel::Warning.should_update_real_time_status());
        assert!(!EventLevel::Info.should_update_real_time_status());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", EventLevel::Critical), "critical");
        assert_eq!(format!("{}", EventLevel::Debug), "debug");
    }
}

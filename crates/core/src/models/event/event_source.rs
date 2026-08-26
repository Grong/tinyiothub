use serde::{Deserialize, Serialize};

/// Event source information value object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventSource {
    source_type: String,
    source_id: String,
    device_id: Option<String>,
    user_id: Option<String>,
}

impl EventSource {
    /// Create a new event source
    pub fn new(source_type: String, source_id: String, device_id: Option<String>, user_id: Option<String>) -> Self {
        Self {
            source_type,
            source_id,
            device_id,
            user_id,
        }
    }

    /// Create a system event source
    pub fn system(source_id: String, user_id: Option<String>) -> Self {
        Self {
            source_type: "system".to_string(),
            source_id,
            device_id: None,
            user_id,
        }
    }

    /// Create a device event source
    pub fn device(device_id: String, source_id: Option<String>) -> Self {
        Self {
            source_type: "device".to_string(),
            source_id: source_id.unwrap_or_else(|| device_id.clone()),
            device_id: Some(device_id),
            user_id: None,
        }
    }

    /// Create a device property event source
    pub fn device_property(device_id: String, property_id: String, _source_id: String) -> Self {
        Self {
            source_type: "device_property".to_string(),
            source_id: format!("{}:{}", device_id, property_id),
            device_id: Some(device_id),
            user_id: None,
        }
    }

    /// Create a user event source
    pub fn user(user_id: String, source_id: String) -> Self {
        Self {
            source_type: "user".to_string(),
            source_id,
            device_id: None,
            user_id: Some(user_id),
        }
    }

    /// Get source type
    pub fn source_type(&self) -> &str {
        &self.source_type
    }

    /// Get source ID
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Get device ID if this is a device event
    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    /// Get user ID if this is a user-related event
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// Check if this is a system event source
    pub fn is_system(&self) -> bool {
        self.source_type == "system"
    }

    /// Check if this is a device event source
    pub fn is_device(&self) -> bool {
        self.source_type == "device" || self.source_type == "device_property"
    }

    /// Check if this is a device event source that is critical
    pub fn is_device_critical(&self) -> bool {
        self.is_device() && self.source_id.contains("critical")
    }

    /// Check if this is a user event source
    pub fn is_user(&self) -> bool {
        self.source_type == "user"
    }

    /// Validate the source according to business rules
    pub fn validate(&self) -> Result<(), String> {
        if self.source_type.is_empty() {
            return Err("Source type cannot be empty".to_string());
        }

        if self.source_id.is_empty() {
            return Err("Source ID cannot be empty".to_string());
        }

        // Validate consistency between source type and IDs
        match self.source_type.as_str() {
            "device" | "device_property" => {
                if self.device_id.is_none() {
                    return Err("Device events must have a device_id".to_string());
                }
            }
            "user" => {
                if self.user_id.is_none() {
                    return Err("User events must have a user_id".to_string());
                }
            }
            "system" => {
                // System events may or may not have user_id
            }
            _ => {
                return Err(format!("Unknown source type: {}", self.source_type));
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for EventSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.source_type, self.source_id)?;

        if let Some(device_id) = &self.device_id {
            write!(f, " (device:{})", device_id)?;
        }

        if let Some(user_id) = &self.user_id {
            write!(f, " (user:{})", user_id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_source() {
        let source = EventSource::system("auth_service".to_string(), Some("user123".to_string()));

        assert_eq!(source.source_type(), "system");
        assert_eq!(source.source_id(), "auth_service");
        assert_eq!(source.user_id(), Some("user123"));
        assert_eq!(source.device_id(), None);
        assert!(source.is_system());
        assert!(!source.is_device());
        assert!(!source.is_user());
    }

    #[test]
    fn test_device_source() {
        let source = EventSource::device("device123".to_string(), Some("modbus_driver".to_string()));

        assert_eq!(source.source_type(), "device");
        assert_eq!(source.source_id(), "modbus_driver");
        assert_eq!(source.device_id(), Some("device123"));
        assert_eq!(source.user_id(), None);
        assert!(!source.is_system());
        assert!(source.is_device());
        assert!(!source.is_user());
    }

    #[test]
    fn test_user_source() {
        let source = EventSource::user("user123".to_string(), "web_ui".to_string());

        assert_eq!(source.source_type(), "user");
        assert_eq!(source.source_id(), "web_ui");
        assert_eq!(source.user_id(), Some("user123"));
        assert_eq!(source.device_id(), None);
        assert!(!source.is_system());
        assert!(!source.is_device());
        assert!(source.is_user());
    }

    #[test]
    fn test_validation() {
        // Valid sources
        assert!(EventSource::system("service".to_string(), None).validate().is_ok());
        assert!(
            EventSource::device("dev1".to_string(), Some("driver".to_string()))
                .validate()
                .is_ok()
        );
        assert!(
            EventSource::user("user1".to_string(), "ui".to_string())
                .validate()
                .is_ok()
        );

        // Invalid sources
        let invalid_device = EventSource::new(
            "device".to_string(),
            "driver".to_string(),
            None, // Missing device_id
            None,
        );
        assert!(invalid_device.validate().is_err());

        let invalid_user = EventSource::new(
            "user".to_string(),
            "ui".to_string(),
            None,
            None, // Missing user_id
        );
        assert!(invalid_user.validate().is_err());
    }

    #[test]
    fn test_display() {
        let system_source = EventSource::system("auth".to_string(), Some("user1".to_string()));
        assert_eq!(format!("{}", system_source), "system:auth (user:user1)");

        let device_source = EventSource::device("dev1".to_string(), Some("driver".to_string()));
        assert_eq!(format!("{}", device_source), "device:driver (device:dev1)");
    }
}

use serde::{Deserialize, Serialize};

/// Event type classification value object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventType {
    System(SystemEventType),
    Device(DeviceEventType),
}

/// System event subtypes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SystemEventType {
    /// User authentication events (login, logout, failed auth)
    UserAuth,
    /// User operation events (CRUD operations, configuration changes)
    UserOperation,
    /// System configuration changes
    SystemConfig,
    /// System errors and exceptions
    SystemError,
}

/// Device event subtypes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DeviceEventType {
    // === 连接相关事件 ===
    /// Device connection status changes (online/offline)
    Connection,

    // === 设备状态事件 ===
    /// Device alarm triggered (设备级别报警)
    DeviceAlarm,
    /// Device alarm cleared/recovered (设备报警恢复正常)
    DeviceNormal,

    // === 属性相关事件 ===
    /// Device property value changed (属性值变化)
    PropertyChange,
    /// Property alarm triggered (属性报警)
    PropertyAlarm,
    /// Property alarm cleared (属性恢复正常)
    PropertyNormal,

    // === 命令相关事件 ===
    /// Command execution started (命令开始执行)
    CommandStarted,
    /// Command execution completed successfully (命令执行成功)
    CommandCompleted,
    /// Command execution failed (命令执行失败)
    CommandFailed,

    // === 设备生命周期事件 ===
    /// Device created (设备创建)
    DeviceCreated,
    /// Device updated (设备更新)
    DeviceUpdated,
    /// Device deleted (设备删除)
    DeviceDeleted,
}

impl EventType {
    /// Get string representation for database storage
    pub fn type_string(&self) -> String {
        match self {
            EventType::System(_) => "system".to_string(),
            EventType::Device(_) => "device".to_string(),
        }
    }

    /// Get subtype string for database storage
    pub fn subtype_string(&self) -> String {
        match self {
            EventType::System(subtype) => match subtype {
                SystemEventType::UserAuth => "user_auth".to_string(),
                SystemEventType::UserOperation => "user_operation".to_string(),
                SystemEventType::SystemConfig => "system_config".to_string(),
                SystemEventType::SystemError => "system_error".to_string(),
            },
            EventType::Device(subtype) => match subtype {
                DeviceEventType::Connection => "connection".to_string(),
                DeviceEventType::DeviceAlarm => "device_alarm".to_string(),
                DeviceEventType::DeviceNormal => "device_normal".to_string(),
                DeviceEventType::PropertyChange => "property_change".to_string(),
                DeviceEventType::PropertyAlarm => "property_alarm".to_string(),
                DeviceEventType::PropertyNormal => "property_normal".to_string(),
                DeviceEventType::CommandStarted => "command_started".to_string(),
                DeviceEventType::CommandCompleted => "command_completed".to_string(),
                DeviceEventType::CommandFailed => "command_failed".to_string(),
                DeviceEventType::DeviceCreated => "device_created".to_string(),
                DeviceEventType::DeviceUpdated => "device_updated".to_string(),
                DeviceEventType::DeviceDeleted => "device_deleted".to_string(),
            },
        }
    }

    /// Check if this is a property-related event
    pub fn is_property_event(&self) -> bool {
        match self {
            EventType::Device(device_type) => device_type.is_property_event(),
            _ => false,
        }
    }

    /// Check if this is a command-related event
    pub fn is_command_event(&self) -> bool {
        match self {
            EventType::Device(device_type) => device_type.is_command_event(),
            _ => false,
        }
    }

    /// Check if this is an alarm-related event
    pub fn is_alarm(&self) -> bool {
        match self {
            EventType::Device(device_type) => device_type.is_alarm(),
            _ => false,
        }
    }

    /// Check if this is a normal/recovery event
    pub fn is_normal(&self) -> bool {
        match self {
            EventType::Device(device_type) => device_type.is_normal(),
            _ => false,
        }
    }

    /// Parse from type and subtype strings (for repository reconstruction)
    pub fn from_strings(type_str: &str, subtype_str: &str) -> Result<Self, String> {
        match type_str {
            "system" => match subtype_str {
                "user_auth" => Ok(EventType::System(SystemEventType::UserAuth)),
                "user_operation" => Ok(EventType::System(SystemEventType::UserOperation)),
                "system_config" => Ok(EventType::System(SystemEventType::SystemConfig)),
                "system_error" => Ok(EventType::System(SystemEventType::SystemError)),
                _ => Err(format!("Unknown system event subtype: {}", subtype_str)),
            },
            "device" => match subtype_str {
                "connection" => Ok(EventType::Device(DeviceEventType::Connection)),
                "device_alarm" => Ok(EventType::Device(DeviceEventType::DeviceAlarm)),
                "device_normal" => Ok(EventType::Device(DeviceEventType::DeviceNormal)),
                "property_change" => Ok(EventType::Device(DeviceEventType::PropertyChange)),
                "property_alarm" => Ok(EventType::Device(DeviceEventType::PropertyAlarm)),
                "property_normal" => Ok(EventType::Device(DeviceEventType::PropertyNormal)),
                "command_started" => Ok(EventType::Device(DeviceEventType::CommandStarted)),
                "command_completed" => Ok(EventType::Device(DeviceEventType::CommandCompleted)),
                "command_failed" => Ok(EventType::Device(DeviceEventType::CommandFailed)),
                "device_created" => Ok(EventType::Device(DeviceEventType::DeviceCreated)),
                "device_updated" => Ok(EventType::Device(DeviceEventType::DeviceUpdated)),
                "device_deleted" => Ok(EventType::Device(DeviceEventType::DeviceDeleted)),
                // Backward compatibility
                "property" => Ok(EventType::Device(DeviceEventType::PropertyChange)),
                "command" => Ok(EventType::Device(DeviceEventType::CommandStarted)),
                _ => Err(format!("Unknown device event subtype: {}", subtype_str)),
            },
            _ => Err(format!("Unknown event type: {}", type_str)),
        }
    }

    /// Parse from dotted notation (e.g., "system.user_auth" or "device.connection")
    /// Used by API endpoints for query parameters
    pub fn from_dotted_string(dotted_str: &str) -> Result<Self, String> {
        let parts: Vec<&str> = dotted_str.split('.').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid event type format: {}", dotted_str));
        }
        Self::from_strings(parts[0], parts[1])
    }

    /// Parse multiple event types from comma-separated dotted notation
    /// Example: "system.user_auth,device.connection,device.property_change"
    pub fn parse_multiple(types_str: &str) -> Result<Vec<Self>, String> {
        let mut types = Vec::new();

        for type_str in types_str.split(',') {
            let type_str = type_str.trim();
            if type_str.is_empty() {
                continue;
            }

            types.push(Self::from_dotted_string(type_str)?);
        }

        Ok(types)
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.type_string(), self.subtype_string())
    }
}

impl DeviceEventType {
    /// Check if this is an alarm-related event
    pub fn is_alarm(&self) -> bool {
        matches!(self, DeviceEventType::DeviceAlarm | DeviceEventType::PropertyAlarm)
    }

    /// Check if this is a normal/recovery event
    pub fn is_normal(&self) -> bool {
        matches!(self, DeviceEventType::DeviceNormal | DeviceEventType::PropertyNormal)
    }

    /// Check if this is a property-related event
    pub fn is_property_event(&self) -> bool {
        matches!(
            self,
            DeviceEventType::PropertyChange
                | DeviceEventType::PropertyAlarm
                | DeviceEventType::PropertyNormal
        )
    }

    /// Check if this is a command-related event
    pub fn is_command_event(&self) -> bool {
        matches!(
            self,
            DeviceEventType::CommandStarted
                | DeviceEventType::CommandCompleted
                | DeviceEventType::CommandFailed
        )
    }

    /// Get a human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            DeviceEventType::Connection => "Connection",
            DeviceEventType::DeviceAlarm => "Device Alarm",
            DeviceEventType::DeviceNormal => "Device Normal",
            DeviceEventType::PropertyChange => "Property Change",
            DeviceEventType::PropertyAlarm => "Property Alarm",
            DeviceEventType::PropertyNormal => "Property Normal",
            DeviceEventType::CommandStarted => "Command Started",
            DeviceEventType::CommandCompleted => "Command Completed",
            DeviceEventType::CommandFailed => "Command Failed",
            DeviceEventType::DeviceCreated => "Device Created",
            DeviceEventType::DeviceUpdated => "Device Updated",
            DeviceEventType::DeviceDeleted => "Device Deleted",
        }
    }

    /// Get the severity level for this event type
    pub fn default_severity(&self) -> crate::domain::event::value_objects::EventLevel {
        use crate::domain::event::value_objects::EventLevel;

        match self {
            DeviceEventType::DeviceAlarm | DeviceEventType::PropertyAlarm => EventLevel::Warning,
            DeviceEventType::DeviceNormal | DeviceEventType::PropertyNormal => EventLevel::Info,
            DeviceEventType::CommandFailed => EventLevel::Error,
            DeviceEventType::CommandCompleted => EventLevel::Info,
            DeviceEventType::CommandStarted => EventLevel::Debug,
            DeviceEventType::PropertyChange => EventLevel::Debug,
            DeviceEventType::Connection => EventLevel::Info,
            DeviceEventType::DeviceCreated => EventLevel::Info,
            DeviceEventType::DeviceUpdated => EventLevel::Info,
            DeviceEventType::DeviceDeleted => EventLevel::Warning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_strings() {
        let event_type = EventType::System(SystemEventType::UserAuth);
        assert_eq!(event_type.type_string(), "system");
        assert_eq!(event_type.subtype_string(), "user_auth");

        let device_type = EventType::Device(DeviceEventType::Connection);
        assert_eq!(device_type.type_string(), "device");
        assert_eq!(device_type.subtype_string(), "connection");

        let alarm_type = EventType::Device(DeviceEventType::PropertyAlarm);
        assert_eq!(alarm_type.subtype_string(), "property_alarm");
    }

    #[test]
    fn test_event_type_parsing() {
        let parsed = EventType::from_strings("system", "user_auth").unwrap();
        assert_eq!(parsed, EventType::System(SystemEventType::UserAuth));

        let device_alarm = EventType::from_strings("device", "device_alarm").unwrap();
        assert_eq!(device_alarm, EventType::Device(DeviceEventType::DeviceAlarm));

        let property_alarm = EventType::from_strings("device", "property_alarm").unwrap();
        assert_eq!(property_alarm, EventType::Device(DeviceEventType::PropertyAlarm));

        let invalid = EventType::from_strings("invalid", "type");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_display() {
        let event_type = EventType::System(SystemEventType::UserAuth);
        assert_eq!(format!("{}", event_type), "system:user_auth");

        let alarm_type = EventType::Device(DeviceEventType::PropertyAlarm);
        assert_eq!(format!("{}", alarm_type), "device:property_alarm");
    }

    #[test]
    fn test_device_event_type_helpers() {
        // Test alarm detection
        assert!(DeviceEventType::DeviceAlarm.is_alarm());
        assert!(DeviceEventType::PropertyAlarm.is_alarm());
        assert!(!DeviceEventType::PropertyChange.is_alarm());

        // Test normal detection
        assert!(DeviceEventType::DeviceNormal.is_normal());
        assert!(DeviceEventType::PropertyNormal.is_normal());
        assert!(!DeviceEventType::PropertyAlarm.is_normal());

        // Test property event detection
        assert!(DeviceEventType::PropertyChange.is_property_event());
        assert!(DeviceEventType::PropertyAlarm.is_property_event());
        assert!(DeviceEventType::PropertyNormal.is_property_event());
        assert!(!DeviceEventType::Connection.is_property_event());

        // Test command event detection
        assert!(DeviceEventType::CommandStarted.is_command_event());
        assert!(DeviceEventType::CommandCompleted.is_command_event());
        assert!(DeviceEventType::CommandFailed.is_command_event());
        assert!(!DeviceEventType::PropertyChange.is_command_event());
    }

    #[test]
    fn test_default_severity() {
        use crate::domain::event::value_objects::EventLevel;

        assert_eq!(DeviceEventType::DeviceAlarm.default_severity(), EventLevel::Warning);
        assert_eq!(DeviceEventType::PropertyAlarm.default_severity(), EventLevel::Warning);
        assert_eq!(DeviceEventType::CommandFailed.default_severity(), EventLevel::Error);
        assert_eq!(DeviceEventType::CommandCompleted.default_severity(), EventLevel::Info);
        assert_eq!(DeviceEventType::PropertyChange.default_severity(), EventLevel::Debug);
    }

    #[test]
    fn test_backward_compatibility() {
        // Old "property" should map to PropertyChange
        let parsed = EventType::from_strings("device", "property").unwrap();
        assert_eq!(parsed, EventType::Device(DeviceEventType::PropertyChange));

        // Old "command" should map to CommandStarted
        let parsed = EventType::from_strings("device", "command").unwrap();
        assert_eq!(parsed, EventType::Device(DeviceEventType::CommandStarted));
    }
}

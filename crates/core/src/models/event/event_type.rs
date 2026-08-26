use serde::{Deserialize, Serialize};

/// Event type classification value object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventType {
    System(SystemEventType),
    Device(ThingEventType),
    Ai(AiEventType),
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

/// Thing event subtypes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThingEventType {
    // === Connection related events ===
    /// Thing connection status changes (online/offline)
    Connection,

    // === Thing status events ===
    /// Thing alarm triggered
    DeviceAlarm,
    /// Thing alarm cleared/recovered
    DeviceNormal,

    // === Property related events ===
    /// Thing property value changed
    PropertyChange,
    /// Property alarm triggered
    PropertyAlarm,
    /// Property alarm cleared
    PropertyNormal,

    // === Command related events ===
    /// Command execution started
    CommandStarted,
    /// Command execution completed successfully
    CommandCompleted,
    /// Command execution failed
    CommandFailed,

    // === Thing lifecycle events ===
    /// Thing created
    DeviceCreated,
    /// Thing updated
    DeviceUpdated,
    /// Thing deleted
    DeviceDeleted,
}

/// AI subsystem event subtypes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AiEventType {
    AlarmCreated,
    AlarmResolved,
    HeartbeatCompleted,
    ChatCompleted,
    WorkspaceCreated,
    WorkspaceDeleted,
    HeartbeatPersistFailed,
    ReflectionFailed,
    ProposalCreated,
    ProposalResolved,
}

impl AiEventType {
    /// The variant ident (e.g. "AlarmCreated") — used as the stable label for
    /// logs, metrics, and drop notifications.
    pub fn name(&self) -> &'static str {
        match self {
            AiEventType::AlarmCreated => "AlarmCreated",
            AiEventType::AlarmResolved => "AlarmResolved",
            AiEventType::HeartbeatCompleted => "HeartbeatCompleted",
            AiEventType::ChatCompleted => "ChatCompleted",
            AiEventType::WorkspaceCreated => "WorkspaceCreated",
            AiEventType::WorkspaceDeleted => "WorkspaceDeleted",
            AiEventType::HeartbeatPersistFailed => "HeartbeatPersistFailed",
            AiEventType::ReflectionFailed => "ReflectionFailed",
            AiEventType::ProposalCreated => "ProposalCreated",
            AiEventType::ProposalResolved => "ProposalResolved",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AiEventType::AlarmCreated => "Alarm Created",
            AiEventType::AlarmResolved => "Alarm Resolved",
            AiEventType::HeartbeatCompleted => "Heartbeat Completed",
            AiEventType::ChatCompleted => "Chat Completed",
            AiEventType::WorkspaceCreated => "Workspace Created",
            AiEventType::WorkspaceDeleted => "Workspace Deleted",
            AiEventType::HeartbeatPersistFailed => "Heartbeat Persist Failed",
            AiEventType::ReflectionFailed => "Reflection Failed",
            AiEventType::ProposalCreated => "Proposal Created",
            AiEventType::ProposalResolved => "Proposal Resolved",
        }
    }

    pub fn subtype_string(&self) -> &'static str {
        match self {
            AiEventType::AlarmCreated => "alarm_created",
            AiEventType::AlarmResolved => "alarm_resolved",
            AiEventType::HeartbeatCompleted => "heartbeat_completed",
            AiEventType::ChatCompleted => "chat_completed",
            AiEventType::WorkspaceCreated => "workspace_created",
            AiEventType::WorkspaceDeleted => "workspace_deleted",
            AiEventType::HeartbeatPersistFailed => "heartbeat_persist_failed",
            AiEventType::ReflectionFailed => "reflection_failed",
            AiEventType::ProposalCreated => "proposal_created",
            AiEventType::ProposalResolved => "proposal_resolved",
        }
    }
}

impl EventType {
    /// Get string representation for database storage
    pub fn type_string(&self) -> String {
        match self {
            EventType::System(_) => "system".to_string(),
            EventType::Device(_) => "device".to_string(),
            EventType::Ai(_) => "ai".to_string(),
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
                ThingEventType::Connection => "connection".to_string(),
                ThingEventType::DeviceAlarm => "device_alarm".to_string(),
                ThingEventType::DeviceNormal => "device_normal".to_string(),
                ThingEventType::PropertyChange => "property_change".to_string(),
                ThingEventType::PropertyAlarm => "property_alarm".to_string(),
                ThingEventType::PropertyNormal => "property_normal".to_string(),
                ThingEventType::CommandStarted => "command_started".to_string(),
                ThingEventType::CommandCompleted => "command_completed".to_string(),
                ThingEventType::CommandFailed => "command_failed".to_string(),
                ThingEventType::DeviceCreated => "device_created".to_string(),
                ThingEventType::DeviceUpdated => "device_updated".to_string(),
                ThingEventType::DeviceDeleted => "device_deleted".to_string(),
            },
            EventType::Ai(subtype) => subtype.subtype_string().to_string(),
        }
    }

    /// Check if this is a property-related event
    pub fn is_property_event(&self) -> bool {
        match self {
            EventType::Device(event_type) => event_type.is_property_event(),
            _ => false,
        }
    }

    /// Check if this is a command-related event
    pub fn is_command_event(&self) -> bool {
        match self {
            EventType::Device(event_type) => event_type.is_command_event(),
            _ => false,
        }
    }

    /// Check if this is an alarm-related event
    pub fn is_alarm(&self) -> bool {
        match self {
            EventType::Device(event_type) => event_type.is_alarm(),
            _ => false,
        }
    }

    /// Check if this is a normal/recovery event
    pub fn is_normal(&self) -> bool {
        match self {
            EventType::Device(event_type) => event_type.is_normal(),
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
                "connection" => Ok(EventType::Device(ThingEventType::Connection)),
                "device_alarm" => Ok(EventType::Device(ThingEventType::DeviceAlarm)),
                "device_normal" => Ok(EventType::Device(ThingEventType::DeviceNormal)),
                "property_change" => Ok(EventType::Device(ThingEventType::PropertyChange)),
                "property_alarm" => Ok(EventType::Device(ThingEventType::PropertyAlarm)),
                "property_normal" => Ok(EventType::Device(ThingEventType::PropertyNormal)),
                "command_started" => Ok(EventType::Device(ThingEventType::CommandStarted)),
                "command_completed" => Ok(EventType::Device(ThingEventType::CommandCompleted)),
                "command_failed" => Ok(EventType::Device(ThingEventType::CommandFailed)),
                "device_created" => Ok(EventType::Device(ThingEventType::DeviceCreated)),
                "device_updated" => Ok(EventType::Device(ThingEventType::DeviceUpdated)),
                "device_deleted" => Ok(EventType::Device(ThingEventType::DeviceDeleted)),
                // Backward compatibility
                "property" => Ok(EventType::Device(ThingEventType::PropertyChange)),
                "command" => Ok(EventType::Device(ThingEventType::CommandStarted)),
                _ => Err(format!("Unknown device event subtype: {}", subtype_str)),
            },
            "ai" => match subtype_str {
                "alarm_created" => Ok(EventType::Ai(AiEventType::AlarmCreated)),
                "alarm_resolved" => Ok(EventType::Ai(AiEventType::AlarmResolved)),
                "heartbeat_completed" => Ok(EventType::Ai(AiEventType::HeartbeatCompleted)),
                "chat_completed" => Ok(EventType::Ai(AiEventType::ChatCompleted)),
                "workspace_created" => Ok(EventType::Ai(AiEventType::WorkspaceCreated)),
                "workspace_deleted" => Ok(EventType::Ai(AiEventType::WorkspaceDeleted)),
                "heartbeat_persist_failed" => Ok(EventType::Ai(AiEventType::HeartbeatPersistFailed)),
                "reflection_failed" => Ok(EventType::Ai(AiEventType::ReflectionFailed)),
                "proposal_created" => Ok(EventType::Ai(AiEventType::ProposalCreated)),
                "proposal_resolved" => Ok(EventType::Ai(AiEventType::ProposalResolved)),
                _ => Err(format!("Unknown ai event subtype: {}", subtype_str)),
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

impl ThingEventType {
    /// Check if this is an alarm-related event
    pub fn is_alarm(&self) -> bool {
        matches!(self, ThingEventType::DeviceAlarm | ThingEventType::PropertyAlarm)
    }

    /// Check if this is a normal/recovery event
    pub fn is_normal(&self) -> bool {
        matches!(self, ThingEventType::DeviceNormal | ThingEventType::PropertyNormal)
    }

    /// Check if this is a property-related event
    pub fn is_property_event(&self) -> bool {
        matches!(
            self,
            ThingEventType::PropertyChange | ThingEventType::PropertyAlarm | ThingEventType::PropertyNormal
        )
    }

    /// Check if this is a command-related event
    pub fn is_command_event(&self) -> bool {
        matches!(
            self,
            ThingEventType::CommandStarted | ThingEventType::CommandCompleted | ThingEventType::CommandFailed
        )
    }

    /// Get a human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            ThingEventType::Connection => "Connection",
            ThingEventType::DeviceAlarm => "Thing Alarm",
            ThingEventType::DeviceNormal => "Thing Normal",
            ThingEventType::PropertyChange => "Property Change",
            ThingEventType::PropertyAlarm => "Property Alarm",
            ThingEventType::PropertyNormal => "Property Normal",
            ThingEventType::CommandStarted => "Command Started",
            ThingEventType::CommandCompleted => "Command Completed",
            ThingEventType::CommandFailed => "Command Failed",
            ThingEventType::DeviceCreated => "Thing Created",
            ThingEventType::DeviceUpdated => "Thing Updated",
            ThingEventType::DeviceDeleted => "Thing Deleted",
        }
    }

    /// Get the severity level for this event type
    pub fn default_severity(&self) -> super::EventLevel {
        use crate::models::event::EventLevel;

        match self {
            ThingEventType::DeviceAlarm | ThingEventType::PropertyAlarm => EventLevel::Warning,
            ThingEventType::DeviceNormal | ThingEventType::PropertyNormal => EventLevel::Info,
            ThingEventType::CommandFailed => EventLevel::Error,
            ThingEventType::CommandCompleted => EventLevel::Info,
            ThingEventType::CommandStarted => EventLevel::Debug,
            ThingEventType::PropertyChange => EventLevel::Debug,
            ThingEventType::Connection => EventLevel::Info,
            ThingEventType::DeviceCreated => EventLevel::Info,
            ThingEventType::DeviceUpdated => EventLevel::Info,
            ThingEventType::DeviceDeleted => EventLevel::Warning,
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

        let event_type = EventType::Device(ThingEventType::Connection);
        assert_eq!(event_type.type_string(), "device");
        assert_eq!(event_type.subtype_string(), "connection");

        let alarm_type = EventType::Device(ThingEventType::PropertyAlarm);
        assert_eq!(alarm_type.subtype_string(), "property_alarm");
    }

    #[test]
    fn test_event_type_parsing() {
        let parsed = EventType::from_strings("system", "user_auth").unwrap();
        assert_eq!(parsed, EventType::System(SystemEventType::UserAuth));

        let device_alarm = EventType::from_strings("device", "device_alarm").unwrap();
        assert_eq!(device_alarm, EventType::Device(ThingEventType::DeviceAlarm));

        let property_alarm = EventType::from_strings("device", "property_alarm").unwrap();
        assert_eq!(property_alarm, EventType::Device(ThingEventType::PropertyAlarm));

        let invalid = EventType::from_strings("invalid", "type");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_display() {
        let event_type = EventType::System(SystemEventType::UserAuth);
        assert_eq!(format!("{}", event_type), "system:user_auth");

        let alarm_type = EventType::Device(ThingEventType::PropertyAlarm);
        assert_eq!(format!("{}", alarm_type), "device:property_alarm");
    }

    #[test]
    fn test_device_event_type_helpers() {
        // Test alarm detection
        assert!(ThingEventType::DeviceAlarm.is_alarm());
        assert!(ThingEventType::PropertyAlarm.is_alarm());
        assert!(!ThingEventType::PropertyChange.is_alarm());

        // Test normal detection
        assert!(ThingEventType::DeviceNormal.is_normal());
        assert!(ThingEventType::PropertyNormal.is_normal());
        assert!(!ThingEventType::PropertyAlarm.is_normal());

        // Test property event detection
        assert!(ThingEventType::PropertyChange.is_property_event());
        assert!(ThingEventType::PropertyAlarm.is_property_event());
        assert!(ThingEventType::PropertyNormal.is_property_event());
        assert!(!ThingEventType::Connection.is_property_event());

        // Test command event detection
        assert!(ThingEventType::CommandStarted.is_command_event());
        assert!(ThingEventType::CommandCompleted.is_command_event());
        assert!(ThingEventType::CommandFailed.is_command_event());
        assert!(!ThingEventType::PropertyChange.is_command_event());
    }

    #[test]
    fn test_default_severity() {
        use crate::models::event::EventLevel;

        assert_eq!(ThingEventType::DeviceAlarm.default_severity(), EventLevel::Warning);
        assert_eq!(ThingEventType::PropertyAlarm.default_severity(), EventLevel::Warning);
        assert_eq!(ThingEventType::CommandFailed.default_severity(), EventLevel::Error);
        assert_eq!(ThingEventType::CommandCompleted.default_severity(), EventLevel::Info);
        assert_eq!(ThingEventType::PropertyChange.default_severity(), EventLevel::Debug);
    }

    #[test]
    fn test_backward_compatibility() {
        // Old "property" should map to PropertyChange
        let parsed = EventType::from_strings("device", "property").unwrap();
        assert_eq!(parsed, EventType::Device(ThingEventType::PropertyChange));

        // Old "command" should map to CommandStarted
        let parsed = EventType::from_strings("device", "command").unwrap();
        assert_eq!(parsed, EventType::Device(ThingEventType::CommandStarted));
    }

    #[test]
    fn test_ai_event_type_strings() {
        let ai_type = EventType::Ai(AiEventType::AlarmCreated);
        assert_eq!(ai_type.type_string(), "ai");
        assert_eq!(ai_type.subtype_string(), "alarm_created");
    }

    #[test]
    fn test_ai_event_type_parsing() {
        let parsed = EventType::from_strings("ai", "heartbeat_completed").unwrap();
        assert_eq!(parsed, EventType::Ai(AiEventType::HeartbeatCompleted));

        let invalid = EventType::from_strings("ai", "nonexistent");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_ai_event_type_name_is_variant_ident() {
        // The AI crate's drop-notifier/metrics label events by this name; it
        // must match the enum variant ident so logs line up with the source.
        assert_eq!(AiEventType::AlarmCreated.name(), "AlarmCreated");
        assert_eq!(AiEventType::HeartbeatCompleted.name(), "HeartbeatCompleted");
        assert_eq!(AiEventType::ProposalResolved.name(), "ProposalResolved");
    }

    #[test]
    fn test_ai_event_type_helpers() {
        let ai_type = EventType::Ai(AiEventType::ChatCompleted);
        assert!(!ai_type.is_alarm());
        assert!(!ai_type.is_command_event());
        assert!(!ai_type.is_property_event());
        assert!(!ai_type.is_normal());
    }
}

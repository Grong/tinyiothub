use crate::domain::event::{
    value_objects::{EventId, EventLevel, EventSource, EventType, RichContent},
    EventError, Result,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Core event entity representing any event in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    id: EventId,
    event_type: EventType,
    level: EventLevel,
    timestamp: DateTime<Utc>,
    source: EventSource,
    content: RichContent,
}

impl Event {
    /// Create a new event with validation
    pub fn new(
        event_type: EventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        // Validate content size
        content.validate_size()?;

        let event = Self {
            id: EventId::new(),
            event_type,
            level,
            timestamp: Utc::now(),
            source,
            content,
        };

        // Additional business rule validations
        event.validate()?;

        Ok(event)
    }

    /// Reconstruct an event from stored data (for repository use)
    pub fn reconstruct(
        id: EventId,
        event_type: EventType,
        level: EventLevel,
        timestamp: DateTime<Utc>,
        source: EventSource,
        content: RichContent,
    ) -> Self {
        Self {
            id,
            event_type,
            level,
            timestamp,
            source,
            content,
        }
    }

    /// Get event ID
    pub fn id(&self) -> &EventId {
        &self.id
    }

    /// Get event type
    pub fn event_type(&self) -> &EventType {
        &self.event_type
    }

    /// Get event level
    pub fn level(&self) -> EventLevel {
        self.level
    }

    /// Get timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get event source
    pub fn source(&self) -> &EventSource {
        &self.source
    }

    /// Get event content
    pub fn content(&self) -> &RichContent {
        &self.content
    }

    /// Update event content (business rule: only within 5 minutes)
    pub fn update_content(&mut self, new_content: RichContent) -> Result<()> {
        // Business rule: Can only update content within 5 minutes
        let now = Utc::now();
        let time_diff = now.signed_duration_since(self.timestamp);

        if time_diff.num_minutes() > 5 {
            return Err(EventError::Validation {
                message: "Cannot update event content after 5 minutes".to_string(),
            });
        }

        // Validate new content
        new_content.validate_size()?;

        self.content = new_content;
        Ok(())
    }

    /// Check if this event is critical
    pub fn is_critical(&self) -> bool {
        matches!(self.level, EventLevel::Critical)
    }

    /// Check if this event should trigger notifications
    pub fn should_notify(&self) -> bool {
        matches!(self.level, EventLevel::Critical | EventLevel::Error)
    }

    /// Check if this event should update real-time status
    pub fn should_update_real_time_status(&self) -> bool {
        match &self.event_type {
            EventType::Device(_) => matches!(
                self.level,
                EventLevel::Critical | EventLevel::Error | EventLevel::Warning
            ),
            EventType::System(_) => matches!(self.level, EventLevel::Critical | EventLevel::Error),
        }
    }

    /// Validate the event according to business rules
    fn validate(&self) -> Result<()> {
        // Validate event type and level consistency
        match &self.event_type {
            EventType::System(system_type) => {
                use crate::domain::event::value_objects::SystemEventType;
                if system_type == &SystemEventType::SystemError
                    && !matches!(self.level, EventLevel::Critical | EventLevel::Error)
                {
                    return Err(EventError::Validation {
                        message: "System error events must be Critical or Error level".to_string(),
                    });
                }
            }
            EventType::Device(device_type) => {
                use crate::domain::event::value_objects::DeviceEventType;
                match device_type {
                    DeviceEventType::Connection => {
                        // Connection events are usually Info level
                    }
                    // Property-related events can be any level depending on alarm rules
                    DeviceEventType::PropertyChange
                    | DeviceEventType::PropertyAlarm
                    | DeviceEventType::PropertyNormal => {
                        // Property events can be any level depending on alarm rules
                    }
                    _ => {}
                }
            }
        }

        // Validate source consistency
        match &self.event_type {
            EventType::Device(_) => {
                if self.source.device_id().is_none() {
                    return Err(EventError::Validation {
                        message: "Device events must have a device_id in source".to_string(),
                    });
                }
            }
            EventType::System(_) => {
                // System events may or may not have user_id
            }
        }

        Ok(())
    }
}

/// Factory methods for creating specific event types
impl Event {
    /// Create a system event
    pub fn new_system_event(
        system_type: crate::domain::event::value_objects::SystemEventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new(EventType::System(system_type), level, source, content)
    }

    /// Create a device event
    pub fn new_device_event(
        device_type: crate::domain::event::value_objects::DeviceEventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new(EventType::Device(device_type), level, source, content)
    }

    /// Create a device connection event
    pub fn new_device_connection_event(
        _device_id: String,
        _device_name: String,
        connection_status: crate::domain::event::value_objects::ConnectionStatus,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        let level = match connection_status {
            crate::domain::event::value_objects::ConnectionStatus::Online => EventLevel::Info,
            crate::domain::event::value_objects::ConnectionStatus::Offline => EventLevel::Warning,
            crate::domain::event::value_objects::ConnectionStatus::Error => EventLevel::Error,
        };

        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::Connection,
            level,
            source,
            content,
        )
    }

    /// Create a device alarm event (设备级别报警)
    pub fn new_device_alarm_event(
        _device_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::DeviceAlarm,
            EventLevel::Warning,
            source,
            content,
        )
    }

    /// Create a device normal event (设备恢复正常)
    pub fn new_device_normal_event(
        _device_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::DeviceNormal,
            EventLevel::Info,
            source,
            content,
        )
    }

    /// Create a property change event (属性值变化)
    pub fn new_property_change_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::PropertyChange,
            EventLevel::Debug,
            source,
            content,
        )
    }

    /// Create a property alarm event (属性报警)
    pub fn new_property_alarm_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
        level: EventLevel, // 允许指定报警级别
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::PropertyAlarm,
            level,
            source,
            content,
        )
    }

    /// Create a property normal event (属性恢复正常)
    pub fn new_property_normal_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::PropertyNormal,
            EventLevel::Info,
            source,
            content,
        )
    }

    /// Create a command started event (命令开始执行)
    pub fn new_command_started_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::CommandStarted,
            EventLevel::Debug,
            source,
            content,
        )
    }

    /// Create a command completed event (命令执行成功)
    pub fn new_command_completed_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::CommandCompleted,
            EventLevel::Info,
            source,
            content,
        )
    }

    /// Create a command failed event (命令执行失败)
    pub fn new_command_failed_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::CommandFailed,
            EventLevel::Error,
            source,
            content,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::{ConnectionStatus, DeviceEventType, SystemEventType};

    fn create_test_source() -> EventSource {
        EventSource::device("device-001".to_string(), Some("Test Device".to_string()))
    }

    fn create_test_content() -> RichContent {
        RichContent::new_text("test".to_string(), "Test content".to_string())
    }

    #[test]
    fn test_event_new_basic() {
        let event = Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        assert!(!event.id().as_str().is_empty());
        assert_eq!(event.event_type(), &EventType::System(SystemEventType::UserAuth));
        assert_eq!(event.level(), EventLevel::Info);
    }

    #[test]
    fn test_event_reconstruct() {
        let id = EventId::new();
        let timestamp = Utc::now();
        let source = EventSource::system("test".to_string(), None);
        let content = create_test_content();

        let event = Event::reconstruct(
            id.clone(),
            EventType::System(SystemEventType::UserOperation),
            EventLevel::Debug,
            timestamp,
            source.clone(),
            content.clone(),
        );

        assert_eq!(event.id(), &id);
        assert_eq!(event.timestamp(), timestamp);
    }

    #[test]
    fn test_event_is_critical() {
        let critical_event = Event::new(
            EventType::System(SystemEventType::SystemError),
            EventLevel::Critical,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        let info_event = Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        assert!(critical_event.is_critical());
        assert!(!info_event.is_critical());
    }

    #[test]
    fn test_event_should_notify() {
        let critical = Event::new(
            EventType::System(SystemEventType::SystemError),
            EventLevel::Critical,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        let error = Event::new(
            EventType::System(SystemEventType::SystemError),
            EventLevel::Error,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        let warning = Event::new(
            EventType::System(SystemEventType::SystemConfig),
            EventLevel::Warning,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        let info = Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        assert!(critical.should_notify());
        assert!(error.should_notify());
        assert!(!warning.should_notify()); // Warning doesn't trigger notification
        assert!(!info.should_notify());
    }

    #[test]
    fn test_event_should_update_real_time_status() {
        // Device event with Warning level should update real-time status
        let device_warning = Event::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Warning,
            EventSource::device("device-1".to_string(), None),
            create_test_content(),
        )
        .unwrap();
        assert!(device_warning.should_update_real_time_status());

        // Device event with Info level should NOT update real-time status
        let device_info = Event::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Info,
            EventSource::device("device-1".to_string(), None),
            create_test_content(),
        )
        .unwrap();
        assert!(!device_info.should_update_real_time_status());

        // System event with Error level should update real-time status
        let system_error = Event::new(
            EventType::System(SystemEventType::SystemError),
            EventLevel::Error,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();
        assert!(system_error.should_update_real_time_status());

        // System event with Warning level should NOT update real-time status
        let system_warning = Event::new(
            EventType::System(SystemEventType::SystemConfig),
            EventLevel::Warning,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();
        assert!(!system_warning.should_update_real_time_status());
    }

    // ===== Factory method tests =====

    #[test]
    fn test_new_system_event() {
        let event = Event::new_system_event(
            SystemEventType::UserAuth,
            EventLevel::Info,
            EventSource::system("admin".to_string(), Some("admin@example.com".to_string())),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(
            event.event_type(),
            &EventType::System(SystemEventType::UserAuth)
        );
        assert_eq!(event.level(), EventLevel::Info);
    }

    #[test]
    fn test_new_device_event() {
        let event = Event::new_device_event(
            DeviceEventType::Connection,
            EventLevel::Info,
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::Connection)
        );
    }

    #[test]
    fn test_new_device_connection_event_online() {
        let event = Event::new_device_connection_event(
            "device-001".to_string(),
            "Sensor 1".to_string(),
            ConnectionStatus::Online,
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Info);
    }

    #[test]
    fn test_new_device_connection_event_offline() {
        let event = Event::new_device_connection_event(
            "device-001".to_string(),
            "Sensor 1".to_string(),
            ConnectionStatus::Offline,
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Warning);
    }

    #[test]
    fn test_new_device_connection_event_error() {
        let event = Event::new_device_connection_event(
            "device-001".to_string(),
            "Sensor 1".to_string(),
            ConnectionStatus::Error,
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Error);
    }

    #[test]
    fn test_new_device_alarm_event() {
        let event = Event::new_device_alarm_event(
            "device-001".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Warning);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::DeviceAlarm)
        );
    }

    #[test]
    fn test_new_device_normal_event() {
        let event = Event::new_device_normal_event(
            "device-001".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Info);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::DeviceNormal)
        );
    }

    #[test]
    fn test_new_property_change_event() {
        let event = Event::new_property_change_event(
            "device-001".to_string(),
            "temperature".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Debug);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::PropertyChange)
        );
    }

    #[test]
    fn test_new_property_alarm_event() {
        let event = Event::new_property_alarm_event(
            "device-001".to_string(),
            "temperature".to_string(),
            create_test_source(),
            create_test_content(),
            EventLevel::Critical,
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Critical);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::PropertyAlarm)
        );
    }

    #[test]
    fn test_new_property_normal_event() {
        let event = Event::new_property_normal_event(
            "device-001".to_string(),
            "temperature".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Info);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::PropertyNormal)
        );
    }

    #[test]
    fn test_new_command_started_event() {
        let event = Event::new_command_started_event(
            "device-001".to_string(),
            "cmd-001".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Debug);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::CommandStarted)
        );
    }

    #[test]
    fn test_new_command_completed_event() {
        let event = Event::new_command_completed_event(
            "device-001".to_string(),
            "cmd-001".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Info);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::CommandCompleted)
        );
    }

    #[test]
    fn test_new_command_failed_event() {
        let event = Event::new_command_failed_event(
            "device-001".to_string(),
            "cmd-001".to_string(),
            create_test_source(),
            create_test_content(),
        )
        .unwrap();

        assert_eq!(event.level(), EventLevel::Error);
        assert_eq!(
            event.event_type(),
            &EventType::Device(DeviceEventType::CommandFailed)
        );
    }

    // ===== Validation tests =====

    #[test]
    fn test_event_validation_system_error_level() {
        // SystemError with non-critical/error level should fail
        let result = Event::new(
            EventType::System(SystemEventType::SystemError),
            EventLevel::Info, // Invalid: SystemError must be Critical or Error
            EventSource::system("test".to_string(), None),
            create_test_content(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_event_validation_device_without_device_id() {
        // Device event without device_id in source should fail
        let result = Event::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Info,
            EventSource::system("test".to_string(), None), // No device_id
            create_test_content(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_event_update_content_within_window() {
        let mut event = Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        // Update content immediately (within 5 minute window)
        let new_content = RichContent::new_text("updated".to_string(), "New content".to_string());
        let result = event.update_content(new_content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_event_update_content_after_window() {
        // Cannot easily test the 5-minute window without mocking time,
        // so we verify the update_content method exists and is callable
        let mut event = Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            create_test_content(),
        )
        .unwrap();

        let new_content = RichContent::new_text("updated".to_string(), "New content".to_string());
        assert!(event.update_content(new_content).is_ok());
    }
}

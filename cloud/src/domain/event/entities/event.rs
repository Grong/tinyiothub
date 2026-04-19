use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::event::{
    value_objects::{EventId, EventLevel, EventSource, EventType, RichContent},
    EventError, Result,
};

/// Core event entity representing any event in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    id: EventId,
    event_type: EventType,
    level: EventLevel,
    timestamp: DateTime<Utc>,
    source: EventSource,
    content: RichContent,
    /// Workspace ID for SSE routing. Populated at event creation from the device.
    workspace_id: Option<String>,
}

impl Event {
    /// Create a new event with validation
    pub fn new(
        event_type: EventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        // Validate content size
        content.validate_size()?;

        let event = Self { id: EventId::new(), event_type, level, timestamp: Utc::now(), source, content, workspace_id };

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
        workspace_id: Option<String>,
    ) -> Self {
        Self { id, event_type, level, timestamp, source, content, workspace_id }
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

    /// Get workspace ID (for SSE routing)
    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
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
            EventType::Device(_) => {
                matches!(self.level, EventLevel::Critical | EventLevel::Error | EventLevel::Warning)
            }
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
        Self::new(EventType::System(system_type), level, source, content, None)
    }

    /// Create a device event
    pub fn new_device_event(
        device_type: crate::domain::event::value_objects::DeviceEventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new(EventType::Device(device_type), level, source, content, workspace_id)
    }

    /// Create a device connection event
    pub fn new_device_connection_event(
        _device_id: String,
        _device_name: String,
        connection_status: crate::domain::event::value_objects::ConnectionStatus,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
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
            workspace_id,
        )
    }

    /// Create a device alarm event (设备级别报警)
    pub fn new_device_alarm_event(
        _device_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::DeviceAlarm,
            EventLevel::Warning,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a device normal event (设备恢复正常)
    pub fn new_device_normal_event(
        _device_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::DeviceNormal,
            EventLevel::Info,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a property change event (属性值变化)
    pub fn new_property_change_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::PropertyChange,
            EventLevel::Debug,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a property alarm event (属性报警)
    pub fn new_property_alarm_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
        level: EventLevel, // 允许指定报警级别
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::PropertyAlarm,
            level,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a property normal event (属性恢复正常)
    pub fn new_property_normal_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::PropertyNormal,
            EventLevel::Info,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a command started event (命令开始执行)
    pub fn new_command_started_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::CommandStarted,
            EventLevel::Debug,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a command completed event (命令执行成功)
    pub fn new_command_completed_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::CommandCompleted,
            EventLevel::Info,
            source,
            content,
            workspace_id,
        )
    }

    /// Create a command failed event (命令执行失败)
    pub fn new_command_failed_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
        workspace_id: Option<String>,
    ) -> Result<Self> {
        Self::new_device_event(
            crate::domain::event::value_objects::DeviceEventType::CommandFailed,
            EventLevel::Error,
            source,
            content,
            workspace_id,
        )
    }
}

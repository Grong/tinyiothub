use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{EventId, EventLevel, EventSource, EventType, RichContent};
use crate::error::{Error, Result};

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
    pub fn new(event_type: EventType, level: EventLevel, source: EventSource, content: RichContent) -> Result<Self> {
        // Validate content size
        content.validate_size().map_err(|e| Error::ValidationError(e))?;

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
            return Err(Error::ValidationError(
                "Cannot update event content after 5 minutes".to_string(),
            ));
        }

        // Validate new content
        new_content.validate_size().map_err(|e| Error::ValidationError(e))?;

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
                matches!(
                    self.level,
                    EventLevel::Critical | EventLevel::Error | EventLevel::Warning
                )
            }
            EventType::System(_) => matches!(self.level, EventLevel::Critical | EventLevel::Error),
        }
    }

    /// Validate the event according to business rules
    fn validate(&self) -> Result<()> {
        // Validate event type and level consistency
        match &self.event_type {
            EventType::System(system_type) => {
                if system_type == &super::SystemEventType::SystemError
                    && !matches!(self.level, EventLevel::Critical | EventLevel::Error)
                {
                    return Err(Error::ValidationError(
                        "System error events must be Critical or Error level".to_string(),
                    ));
                }
            }
            EventType::Device(device_type) => {
                match device_type {
                    DeviceEventType::Connection => {
                        // Connection events are usually Info level
                    }
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
                    return Err(Error::ValidationError(
                        "Device events must have a device_id in source".to_string(),
                    ));
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
        system_type: super::SystemEventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new(EventType::System(system_type), level, source, content)
    }

    /// Create a device event
    pub fn new_device_event(
        device_type: super::DeviceEventType,
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
        connection_status: super::ConnectionStatus,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        let level = match connection_status {
            super::ConnectionStatus::Online => EventLevel::Info,
            super::ConnectionStatus::Offline => EventLevel::Warning,
            super::ConnectionStatus::Error => EventLevel::Error,
        };

        Self::new_device_event(super::DeviceEventType::Connection, level, source, content)
    }

    /// Create a device alarm event
    pub fn new_device_alarm_event(_device_id: String, source: EventSource, content: RichContent) -> Result<Self> {
        Self::new_device_event(
            super::DeviceEventType::DeviceAlarm,
            EventLevel::Warning,
            source,
            content,
        )
    }

    /// Create a device normal event
    pub fn new_device_normal_event(_device_id: String, source: EventSource, content: RichContent) -> Result<Self> {
        Self::new_device_event(super::DeviceEventType::DeviceNormal, EventLevel::Info, source, content)
    }

    /// Create a property change event
    pub fn new_property_change_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            super::DeviceEventType::PropertyChange,
            EventLevel::Debug,
            source,
            content,
        )
    }

    /// Create a property alarm event
    pub fn new_property_alarm_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
        level: EventLevel,
    ) -> Result<Self> {
        Self::new_device_event(super::DeviceEventType::PropertyAlarm, level, source, content)
    }

    /// Create a property normal event
    pub fn new_property_normal_event(
        _device_id: String,
        _property_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            super::DeviceEventType::PropertyNormal,
            EventLevel::Info,
            source,
            content,
        )
    }

    /// Create a command started event
    pub fn new_command_started_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            super::DeviceEventType::CommandStarted,
            EventLevel::Debug,
            source,
            content,
        )
    }

    /// Create a command completed event
    pub fn new_command_completed_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            super::DeviceEventType::CommandCompleted,
            EventLevel::Info,
            source,
            content,
        )
    }

    /// Create a command failed event
    pub fn new_command_failed_event(
        _device_id: String,
        _command_id: String,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        Self::new_device_event(
            super::DeviceEventType::CommandFailed,
            EventLevel::Error,
            source,
            content,
        )
    }
}

use super::DeviceEventType;

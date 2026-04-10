use chrono::{Duration, Utc};

use crate::domain::event::{
    entities::Event,
    value_objects::{EventLevel, EventType},
    EventError, Result,
};

/// Specification pattern for event business rules
pub trait EventSpecification: Send + Sync {
    fn is_satisfied_by(&self, event: &Event) -> bool;
    fn error_message(&self) -> String;
}

/// Event must have valid content
pub struct EventContentValidSpec;

impl EventSpecification for EventContentValidSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        !event.content().is_empty()
    }

    fn error_message(&self) -> String {
        "Event content cannot be empty".to_string()
    }
}

/// Critical events must have device source
pub struct CriticalEventDeviceSourceSpec;

impl EventSpecification for CriticalEventDeviceSourceSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        if matches!(event.level(), EventLevel::Critical | EventLevel::Error) {
            event.source().is_device()
        } else {
            true // Non-critical events can have any source
        }
    }

    fn error_message(&self) -> String {
        "Critical events must have device source".to_string()
    }
}

/// Event timestamp must be recent (within 1 hour)
pub struct EventTimestampRecentSpec {
    max_age: Duration,
}

impl EventTimestampRecentSpec {
    pub fn new(max_age: Duration) -> Self {
        Self { max_age }
    }

    pub fn default() -> Self {
        Self::new(Duration::hours(1))
    }
}

impl EventSpecification for EventTimestampRecentSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        let now = Utc::now();
        let age = now.signed_duration_since(event.timestamp());
        age <= self.max_age
    }

    fn error_message(&self) -> String {
        format!("Event timestamp must be within {} hours", self.max_age.num_hours())
    }
}

/// System events must have valid system source
pub struct SystemEventValidSourceSpec;

impl EventSpecification for SystemEventValidSourceSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        match event.event_type() {
            EventType::System(_) => event.source().is_system(),
            _ => true, // Non-system events can have any source
        }
    }

    fn error_message(&self) -> String {
        "System events must have system source".to_string()
    }
}

/// Device events must have valid device source
pub struct DeviceEventValidSourceSpec;

impl EventSpecification for DeviceEventValidSourceSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        match event.event_type() {
            EventType::Device(_) => event.source().is_device(),
            _ => true, // Non-device events can have any source
        }
    }

    fn error_message(&self) -> String {
        "Device events must have device source".to_string()
    }
}

/// Event level must match event type severity
pub struct EventLevelMatchesTypeSpec;

impl EventSpecification for EventLevelMatchesTypeSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        use crate::domain::event::value_objects::{DeviceEventType, SystemEventType};

        match (event.event_type(), event.level()) {
            // System events
            (EventType::System(SystemEventType::UserAuth), EventLevel::Info) => true,
            (EventType::System(SystemEventType::UserAuth), EventLevel::Warning) => true,
            (EventType::System(SystemEventType::SystemConfig), EventLevel::Info) => true,
            (EventType::System(SystemEventType::SystemConfig), EventLevel::Warning) => true,
            (EventType::System(SystemEventType::SystemError), EventLevel::Error) => true,
            (EventType::System(SystemEventType::SystemError), EventLevel::Critical) => true,

            // Device events
            (EventType::Device(DeviceEventType::Connection), EventLevel::Error) => true,
            (EventType::Device(DeviceEventType::Connection), EventLevel::Critical) => true,
            (EventType::Device(DeviceEventType::Connection), EventLevel::Info) => true,
            (EventType::Device(DeviceEventType::PropertyChange), EventLevel::Debug) => true,
            (EventType::Device(DeviceEventType::PropertyChange), EventLevel::Info) => true,
            (EventType::Device(DeviceEventType::PropertyAlarm), EventLevel::Warning) => true,
            (EventType::Device(DeviceEventType::PropertyAlarm), EventLevel::Error) => true,
            (EventType::Device(DeviceEventType::PropertyNormal), EventLevel::Info) => true,
            (EventType::Device(DeviceEventType::CommandStarted), EventLevel::Info) => true,
            (EventType::Device(DeviceEventType::CommandCompleted), EventLevel::Info) => true,
            (EventType::Device(DeviceEventType::CommandFailed), EventLevel::Error) => true,

            // Invalid combinations
            _ => false,
        }
    }

    fn error_message(&self) -> String {
        "Event level does not match event type severity".to_string()
    }
}

/// Composite specification for all event validation rules
pub struct EventValidationSpec {
    specs: Vec<Box<dyn EventSpecification>>,
}

impl EventValidationSpec {
    pub fn new() -> Self {
        Self {
            specs: vec![
                Box::new(EventContentValidSpec),
                Box::new(CriticalEventDeviceSourceSpec),
                Box::new(EventTimestampRecentSpec::default()),
                Box::new(SystemEventValidSourceSpec),
                Box::new(DeviceEventValidSourceSpec),
                Box::new(EventLevelMatchesTypeSpec),
            ],
        }
    }

    pub fn validate(&self, event: &Event) -> Result<()> {
        for spec in &self.specs {
            if !spec.is_satisfied_by(event) {
                return Err(EventError::Validation { message: spec.error_message() });
            }
        }
        Ok(())
    }

    pub fn add_specification(&mut self, spec: Box<dyn EventSpecification>) {
        self.specs.push(spec);
    }
}

impl Default for EventValidationSpec {
    fn default() -> Self {
        Self::new()
    }
}

/// Event priority specification
pub struct EventPrioritySpec;

impl EventPrioritySpec {
    /// Get processing priority for event (1 = highest, 5 = lowest)
    pub fn get_priority(event: &Event) -> u8 {
        match event.level() {
            EventLevel::Critical => 1,
            EventLevel::Error => 2,
            EventLevel::Warning => 3,
            EventLevel::Info => 4,
            EventLevel::Debug => 5,
        }
    }

    /// Check if event requires immediate processing
    pub fn requires_immediate_processing(event: &Event) -> bool {
        matches!(event.level(), EventLevel::Critical | EventLevel::Error)
    }

    /// Check if event should be persisted
    pub fn should_persist(event: &Event) -> bool {
        // Don't persist debug events from non-device sources
        if matches!(event.level(), EventLevel::Debug) && !event.source().is_device() {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::{
        entities::Event,
        value_objects::{
            DeviceEventType, EventLevel, EventSource, EventType, RichContent, SystemEventType,
        },
    };

    fn create_valid_system_event() -> Event {
        Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("auth-service".to_string(), None),
            RichContent::new_text("Login".to_string(), "User logged in".to_string()),
            None,
        )
        .unwrap()
    }

    fn create_valid_device_event() -> Event {
        Event::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Error,
            EventSource::device("device-1".to_string(), Some("Device 1".to_string())),
            RichContent::new_text(
                "Connection Lost".to_string(),
                "Device connection lost".to_string(),
            ),
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_event_content_valid_spec() {
        let spec = EventContentValidSpec;
        let event = create_valid_system_event();
        assert!(spec.is_satisfied_by(&event));
    }

    #[test]
    fn test_critical_event_device_source_spec() {
        let spec = CriticalEventDeviceSourceSpec;

        // Valid: Critical event with device source
        let device_event = create_valid_device_event();
        assert!(spec.is_satisfied_by(&device_event));

        // Valid: Non-critical event with system source
        let system_event = create_valid_system_event();
        assert!(spec.is_satisfied_by(&system_event));
    }

    #[test]
    fn test_event_timestamp_recent_spec() {
        let spec = EventTimestampRecentSpec::default();
        let event = create_valid_system_event();
        assert!(spec.is_satisfied_by(&event));
    }

    #[test]
    fn test_system_event_valid_source_spec() {
        let spec = SystemEventValidSourceSpec;
        let event = create_valid_system_event();
        assert!(spec.is_satisfied_by(&event));
    }

    #[test]
    fn test_device_event_valid_source_spec() {
        let spec = DeviceEventValidSourceSpec;
        let event = create_valid_device_event();
        assert!(spec.is_satisfied_by(&event));
    }

    #[test]
    fn test_event_level_matches_type_spec() {
        let spec = EventLevelMatchesTypeSpec;

        // Valid combinations
        let system_event = create_valid_system_event();
        assert!(spec.is_satisfied_by(&system_event));

        let device_event = create_valid_device_event();
        assert!(spec.is_satisfied_by(&device_event));
    }

    #[test]
    fn test_event_validation_spec() {
        let spec = EventValidationSpec::new();

        let valid_event = create_valid_system_event();
        assert!(spec.validate(&valid_event).is_ok());

        let device_event = create_valid_device_event();
        assert!(spec.validate(&device_event).is_ok());
    }

    #[test]
    fn test_event_priority_spec() {
        let critical_event = Event::new(
            EventType::Device(DeviceEventType::DeviceCreated),
            EventLevel::Critical,
            EventSource::device("device-1".to_string(), Some("Device 1".to_string())),
            RichContent::new_text(
                "Critical Error".to_string(),
                "Critical error occurred".to_string(),
            ),
            None,
        )
        .unwrap();

        assert_eq!(EventPrioritySpec::get_priority(&critical_event), 1);
        assert!(EventPrioritySpec::requires_immediate_processing(&critical_event));
        assert!(EventPrioritySpec::should_persist(&critical_event));

        let debug_event = Event::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Debug,
            EventSource::system("auth-service".to_string(), None),
            RichContent::new_text("Debug".to_string(), "Debug message".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(EventPrioritySpec::get_priority(&debug_event), 5);
        assert!(!EventPrioritySpec::requires_immediate_processing(&debug_event));
        assert!(!EventPrioritySpec::should_persist(&debug_event));
    }
}

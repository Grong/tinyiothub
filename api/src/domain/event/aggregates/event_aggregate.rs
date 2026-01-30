use crate::domain::event::{
    entities::Event,
    value_objects::{EventId, EventLevel, EventSource, EventType, RichContent},
    EventError, Result,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Event Aggregate Root
///
/// Encapsulates the Event entity and related business logic.
/// Ensures consistency and enforces business rules for event operations.
pub struct EventAggregate {
    event: Event,
    metadata: HashMap<String, String>,
    version: u64,
}

impl EventAggregate {
    /// Create a new event aggregate
    pub fn new(
        event_type: EventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        let event = Event::new(event_type, level, source, content)?;

        Ok(Self {
            event,
            metadata: HashMap::new(),
            version: 1,
        })
    }

    /// Create aggregate from existing event
    pub fn from_event(event: Event) -> Self {
        Self {
            event,
            metadata: HashMap::new(),
            version: 1,
        }
    }

    /// Get the event entity
    pub fn event(&self) -> &Event {
        &self.event
    }

    /// Get event ID
    pub fn id(&self) -> &EventId {
        self.event.id()
    }

    /// Get event type
    pub fn event_type(&self) -> &EventType {
        self.event.event_type()
    }

    /// Get event level
    pub fn level(&self) -> EventLevel {
        self.event.level()
    }

    /// Get event source
    pub fn source(&self) -> &EventSource {
        self.event.source()
    }

    /// Get event content
    pub fn content(&self) -> &RichContent {
        self.event.content()
    }

    /// Get event timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.event.timestamp()
    }

    /// Add metadata to the event
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.version += 1;
    }

    /// Get metadata
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Get aggregate version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Update event content (business rule: only if not processed)
    pub fn update_content(&mut self, new_content: RichContent) -> Result<()> {
        // Business rule: Can only update content if event is recent (within 5 minutes)
        let now = Utc::now();
        let time_diff = now.signed_duration_since(self.event.timestamp());

        if time_diff.num_minutes() > 5 {
            return Err(EventError::Validation {
                message: "Cannot update event content after 5 minutes".to_string(),
            });
        }

        self.event.update_content(new_content)?;
        self.version += 1;
        Ok(())
    }

    /// Check if event is critical (business rule)
    pub fn is_critical(&self) -> bool {
        matches!(self.event.level(), EventLevel::Error | EventLevel::Critical)
    }

    /// Check if event requires immediate notification (business rule)
    pub fn requires_immediate_notification(&self) -> bool {
        self.is_critical() || self.event.source().is_device_critical()
    }

    /// Get event priority for processing (business rule)
    pub fn processing_priority(&self) -> u8 {
        match self.event.level() {
            EventLevel::Critical => 1,
            EventLevel::Error => 2,
            EventLevel::Warning => 3,
            EventLevel::Info => 4,
            EventLevel::Debug => 5,
        }
    }

    /// Validate event for persistence (business rules)
    pub fn validate_for_persistence(&self) -> Result<()> {
        // Business rule: Event must have valid content
        if self.event.content().is_empty() {
            return Err(EventError::Validation {
                message: "Event content cannot be empty".to_string(),
            });
        }

        // Business rule: Critical events must have device source
        if self.is_critical() && !self.event.source().is_device() {
            return Err(EventError::Validation {
                message: "Critical events must have device source".to_string(),
            });
        }

        Ok(())
    }

    /// Convert to event entity for persistence
    pub fn into_event(self) -> Event {
        self.event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::{DeviceEventType, SystemEventType};

    fn create_test_aggregate() -> EventAggregate {
        EventAggregate::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "Test content".to_string()),
        )
        .unwrap()
    }

    #[test]
    fn test_create_event_aggregate() {
        let aggregate = create_test_aggregate();
        assert_eq!(aggregate.version(), 1);
        assert!(!aggregate.is_critical());
    }

    #[test]
    fn test_add_metadata() {
        let mut aggregate = create_test_aggregate();
        aggregate.add_metadata("key".to_string(), "value".to_string());

        assert_eq!(aggregate.version(), 2);
        assert_eq!(aggregate.metadata().get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_critical_event_detection() {
        let aggregate = EventAggregate::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Critical,
            EventSource::device("device-1".to_string(), Some("Device 1".to_string())),
            RichContent::new_text("Critical".to_string(), "Device connection lost".to_string()),
        )
        .unwrap();

        assert!(aggregate.is_critical());
        assert!(aggregate.requires_immediate_notification());
        assert_eq!(aggregate.processing_priority(), 1);
    }

    #[test]
    fn test_validation_rules() {
        let aggregate = create_test_aggregate();
        assert!(aggregate.validate_for_persistence().is_ok());
    }
}

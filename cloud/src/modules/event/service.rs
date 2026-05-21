// Event domain services, aggregates, and specifications
// Migrated from domain/event/aggregates/event_aggregate.rs
//           domain/event/services/event_service.rs
//           domain/event/specifications/event_specifications.rs

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use crate::modules::event::{
    EventError, Result,
    entities::Event,
    errors::{DomainResult, EventDomainError, EventServiceDomainError},
    value_objects::{
        DeviceEventType, EventId, EventLevel, EventSource, EventType, RichContent, SystemEventType,
    },
};

// ════════════════════════════════════════════════
// Event Aggregate (from aggregates/event_aggregate.rs)
// ════════════════════════════════════════════════

/// Event Aggregate Root — encapsulates Event entity and business logic
pub struct EventAggregate {
    event: Event,
    metadata: HashMap<String, String>,
    version: u64,
}

impl EventAggregate {
    pub fn new(
        event_type: EventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> Result<Self> {
        let event = Event::new(event_type, level, source, content)?;
        Ok(Self { event, metadata: HashMap::new(), version: 1 })
    }

    pub fn from_event(event: Event) -> Self {
        Self { event, metadata: HashMap::new(), version: 1 }
    }

    pub fn event(&self) -> &Event {
        &self.event
    }
    pub fn id(&self) -> &EventId {
        self.event.id()
    }
    pub fn event_type(&self) -> &EventType {
        self.event.event_type()
    }
    pub fn level(&self) -> EventLevel {
        self.event.level()
    }
    pub fn source(&self) -> &EventSource {
        self.event.source()
    }
    pub fn content(&self) -> &RichContent {
        self.event.content()
    }
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.event.timestamp()
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.version += 1;
    }

    pub fn update_content(&mut self, new_content: RichContent) -> Result<()> {
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

    pub fn is_critical(&self) -> bool {
        matches!(self.event.level(), EventLevel::Error | EventLevel::Critical)
    }

    pub fn requires_immediate_notification(&self) -> bool {
        self.is_critical() || self.event.source().is_device_critical()
    }

    pub fn processing_priority(&self) -> u8 {
        match self.event.level() {
            EventLevel::Critical => 1,
            EventLevel::Error => 2,
            EventLevel::Warning => 3,
            EventLevel::Info => 4,
            EventLevel::Debug => 5,
        }
    }

    pub fn validate_for_persistence(&self) -> Result<()> {
        if self.event.content().is_empty() {
            return Err(EventError::Validation {
                message: "Event content cannot be empty".to_string(),
            });
        }
        if self.is_critical() && !self.event.source().is_device() {
            return Err(EventError::Validation {
                message: "Critical events must have device source".to_string(),
            });
        }
        Ok(())
    }

    pub fn into_event(self) -> Event {
        self.event
    }
}

// ════════════════════════════════════════════════
// Specifications (from specifications/event_specifications.rs)
// ════════════════════════════════════════════════

/// Specification pattern for event business rules
pub trait EventSpecification: Send + Sync {
    fn is_satisfied_by(&self, event: &Event) -> bool;
    fn error_message(&self) -> String;
}

pub struct EventContentValidSpec;
impl EventSpecification for EventContentValidSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        !event.content().is_empty()
    }
    fn error_message(&self) -> String {
        "Event content cannot be empty".to_string()
    }
}

pub struct CriticalEventDeviceSourceSpec;
impl EventSpecification for CriticalEventDeviceSourceSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        if matches!(event.level(), EventLevel::Critical | EventLevel::Error) {
            event.source().is_device()
        } else {
            true
        }
    }
    fn error_message(&self) -> String {
        "Critical events must have device source".to_string()
    }
}

pub struct EventTimestampRecentSpec {
    max_age: Duration,
}
impl EventTimestampRecentSpec {
    pub fn new(max_age: Duration) -> Self {
        Self { max_age }
    }
    pub fn default_spec() -> Self {
        Self::new(Duration::hours(1))
    }
}
impl EventSpecification for EventTimestampRecentSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        let age = Utc::now().signed_duration_since(event.timestamp());
        age <= self.max_age
    }
    fn error_message(&self) -> String {
        format!("Event timestamp must be within {} hours", self.max_age.num_hours())
    }
}

pub struct SystemEventValidSourceSpec;
impl EventSpecification for SystemEventValidSourceSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        match event.event_type() {
            EventType::System(_) => event.source().is_system(),
            _ => true,
        }
    }
    fn error_message(&self) -> String {
        "System events must have system source".to_string()
    }
}

pub struct DeviceEventValidSourceSpec;
impl EventSpecification for DeviceEventValidSourceSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        match event.event_type() {
            EventType::Device(_) => event.source().is_device(),
            _ => true,
        }
    }
    fn error_message(&self) -> String {
        "Device events must have device source".to_string()
    }
}

pub struct EventLevelMatchesTypeSpec;
impl EventSpecification for EventLevelMatchesTypeSpec {
    fn is_satisfied_by(&self, event: &Event) -> bool {
        matches!(
            (event.event_type(), event.level()),
            (EventType::System(SystemEventType::UserAuth), EventLevel::Info)
                | (EventType::System(SystemEventType::UserAuth), EventLevel::Warning)
                | (EventType::System(SystemEventType::SystemConfig), EventLevel::Info)
                | (EventType::System(SystemEventType::SystemConfig), EventLevel::Warning)
                | (EventType::System(SystemEventType::SystemError), EventLevel::Error)
                | (EventType::System(SystemEventType::SystemError), EventLevel::Critical)
                | (EventType::Device(DeviceEventType::Connection), EventLevel::Error)
                | (EventType::Device(DeviceEventType::Connection), EventLevel::Critical)
                | (EventType::Device(DeviceEventType::Connection), EventLevel::Info)
                | (EventType::Device(DeviceEventType::PropertyChange), EventLevel::Debug)
                | (EventType::Device(DeviceEventType::PropertyChange), EventLevel::Info)
                | (EventType::Device(DeviceEventType::PropertyAlarm), EventLevel::Warning)
                | (EventType::Device(DeviceEventType::PropertyAlarm), EventLevel::Error)
                | (EventType::Device(DeviceEventType::PropertyNormal), EventLevel::Info)
                | (EventType::Device(DeviceEventType::CommandStarted), EventLevel::Info)
                | (EventType::Device(DeviceEventType::CommandCompleted), EventLevel::Info)
                | (EventType::Device(DeviceEventType::CommandFailed), EventLevel::Error)
        )
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
                Box::new(EventTimestampRecentSpec::default_spec()),
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
    pub fn get_priority(event: &Event) -> u8 {
        match event.level() {
            EventLevel::Critical => 1,
            EventLevel::Error => 2,
            EventLevel::Warning => 3,
            EventLevel::Info => 4,
            EventLevel::Debug => 5,
        }
    }

    pub fn requires_immediate_processing(event: &Event) -> bool {
        matches!(event.level(), EventLevel::Critical | EventLevel::Error)
    }

    pub fn should_persist(event: &Event) -> bool {
        if matches!(event.level(), EventLevel::Debug) && !event.source().is_device() {
            return false;
        }
        true
    }
}

// ════════════════════════════════════════════════
// Event Service (from services/event_service.rs)
// ════════════════════════════════════════════════

// Constants
const EVENT_CATEGORY_SYSTEM: &str = "system";
const EVENT_CATEGORY_DEVICE: &str = "device";
const PATTERN_TYPE_REPEATED_ERRORS: &str = "repeated_errors";
const PATTERN_SEVERITY_HIGH: &str = "high";
const PATTERN_SEVERITY_MEDIUM: &str = "medium";
const EVENT_UPDATE_TIME_LIMIT_MINUTES: i64 = 5;
const EVENT_BATCH_SIZE_LIMIT: usize = 1000;
const REPEATED_ERRORS_THRESHOLD: usize = 3;
const REPEATED_ERRORS_HIGH_SEVERITY_THRESHOLD: usize = 5;

/// Domain service for event processing (pure business logic)
pub struct EventService {
    validation_spec: EventValidationSpec,
}

impl EventService {
    pub fn new() -> Self {
        Self { validation_spec: EventValidationSpec::new() }
    }

    pub fn create_event(
        &self,
        event_type: EventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> DomainResult<EventAggregate> {
        let aggregate = EventAggregate::new(event_type, level, source, content)
            .map_err(|e| EventDomainError::validation(e.to_string()))?;
        self.validation_spec
            .validate(aggregate.event())
            .map_err(|e| EventDomainError::validation(e.to_string()))?;
        aggregate
            .validate_for_persistence()
            .map_err(|e| EventDomainError::validation(e.to_string()))?;
        Ok(aggregate)
    }

    pub fn process_event_for_notifications(
        &self,
        event: &Event,
        notification_rules: &[crate::modules::notification::NotificationAggregate],
    ) -> DomainResult<Vec<String>> {
        let mut matching_rules = Vec::new();
        for rule_aggregate in notification_rules {
            if rule_aggregate.matches_event(&event.event_type().to_string(), &event.level()) {
                matching_rules.push(rule_aggregate.rule().id.clone());
            }
        }
        Ok(matching_rules)
    }

    pub fn get_processing_priority(&self, event: &Event) -> u8 {
        EventPrioritySpec::get_priority(event)
    }

    pub fn requires_immediate_processing(&self, event: &Event) -> bool {
        EventPrioritySpec::requires_immediate_processing(event)
    }

    pub fn should_persist_event(&self, event: &Event) -> bool {
        EventPrioritySpec::should_persist(event)
    }

    pub fn validate_event_update(
        &self,
        current_event: &Event,
        new_content: &RichContent,
    ) -> DomainResult<()> {
        let now = Utc::now();
        let time_diff = now.signed_duration_since(current_event.timestamp());
        if time_diff.num_minutes() > EVENT_UPDATE_TIME_LIMIT_MINUTES {
            return Err(EventDomainError::immutable(format!(
                "Cannot update event content after {} minutes",
                EVENT_UPDATE_TIME_LIMIT_MINUTES
            ))
            .into());
        }
        if new_content.is_empty() {
            return Err(EventDomainError::invalid_content(
                "Event content cannot be empty".to_string(),
            )
            .into());
        }
        Ok(())
    }

    pub fn calculate_severity_score(&self, event: &Event) -> u32 {
        let mut score = 0u32;
        score += match event.level() {
            EventLevel::Critical => 100,
            EventLevel::Error => 75,
            EventLevel::Warning => 50,
            EventLevel::Info => 25,
            EventLevel::Debug => 10,
        };
        if matches!(event.event_type(), EventType::Device(_)) {
            score += 20;
        }
        if event.source().is_device_critical() {
            score += 30;
        }
        score
    }

    pub fn group_events_by_category<'a>(
        &self,
        events: &'a [Event],
    ) -> HashMap<String, Vec<&'a Event>> {
        let mut groups = HashMap::new();
        for event in events {
            let category = match event.event_type() {
                EventType::System(_) => EVENT_CATEGORY_SYSTEM,
                EventType::Device(_) => EVENT_CATEGORY_DEVICE,
            };
            groups.entry(category.to_string()).or_insert_with(Vec::new).push(event);
        }
        groups
    }

    pub fn filter_events_by_criteria<'a>(
        &self,
        events: &'a [Event],
        min_level: Option<EventLevel>,
        event_types: Option<&[EventType]>,
        sources: Option<&[EventSource]>,
        time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    ) -> Vec<&'a Event> {
        events
            .iter()
            .filter(|event| {
                if let Some(min_level) = &min_level
                    && &event.level() < min_level
                {
                    return false;
                }
                if let Some(types) = event_types
                    && !types.contains(event.event_type())
                {
                    return false;
                }
                if let Some(sources) = sources
                    && !sources.contains(event.source())
                {
                    return false;
                }
                if let Some((start, end)) = time_range {
                    let ts = event.timestamp();
                    if ts < start || ts > end {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn detect_event_patterns(&self, events: &[Event]) -> Vec<EventPattern> {
        let mut patterns = Vec::new();
        patterns.extend(self.detect_repeated_errors_pattern(events));
        patterns
    }

    fn detect_repeated_errors_pattern(&self, events: &[Event]) -> Vec<EventPattern> {
        let error_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.level(), EventLevel::Error | EventLevel::Critical))
            .collect();
        let mut source_counts = HashMap::new();
        for event in &error_events {
            *source_counts.entry(event.source().clone()).or_insert(0) += 1;
        }
        let mut patterns = Vec::new();
        for (source, count) in source_counts {
            if count >= REPEATED_ERRORS_THRESHOLD {
                patterns.push(EventPattern {
                    pattern_type: PATTERN_TYPE_REPEATED_ERRORS.to_string(),
                    description: format!("Repeated errors from source: {}", source.source_id()),
                    severity: if count >= REPEATED_ERRORS_HIGH_SEVERITY_THRESHOLD {
                        PATTERN_SEVERITY_HIGH
                    } else {
                        PATTERN_SEVERITY_MEDIUM
                    }
                    .to_string(),
                    event_count: count,
                    sources: vec![source],
                });
            }
        }
        patterns
    }

    pub fn validate_event_batch(&self, events: &[Event]) -> DomainResult<()> {
        if events.len() > EVENT_BATCH_SIZE_LIMIT {
            return Err(EventServiceDomainError::CapacityExceeded {
                current: events.len(),
                max: EVENT_BATCH_SIZE_LIMIT,
            }
            .into());
        }
        for event in events {
            self.validation_spec
                .validate(event)
                .map_err(|e| EventDomainError::validation(e.to_string()))?;
        }
        Ok(())
    }

    /// Process an event (placeholder for event bus integration)
    pub async fn process_event(&self, _event: &Event) -> std::result::Result<(), String> {
        Ok(())
    }
}

impl Default for EventService {
    fn default() -> Self {
        Self::new()
    }
}

/// Event pattern detection result
#[derive(Debug, Clone)]
pub struct EventPattern {
    pub pattern_type: String,
    pub description: String,
    pub severity: String,
    pub event_count: usize,
    pub sources: Vec<EventSource>,
}

// ════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event(level: EventLevel) -> Event {
        Event::new(
            EventType::System(SystemEventType::UserAuth),
            level,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "Test content".to_string()),
        )
        .unwrap()
    }

    fn create_test_aggregate() -> EventAggregate {
        EventAggregate::new(
            EventType::System(SystemEventType::UserAuth),
            EventLevel::Info,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "Test content".to_string()),
        )
        .unwrap()
    }

    // ── Aggregate tests ──

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

    // ── Specification tests ──

    #[test]
    fn test_event_content_valid_spec() {
        let spec = EventContentValidSpec;
        let event = create_test_event(EventLevel::Info);
        assert!(spec.is_satisfied_by(&event));
    }

    #[test]
    fn test_critical_event_device_source_spec() {
        let spec = CriticalEventDeviceSourceSpec;
        let device_event = Event::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Error,
            EventSource::device("device-1".to_string(), Some("Device 1".to_string())),
            RichContent::new_text(
                "Connection Lost".to_string(),
                "Device connection lost".to_string(),
            ),
        )
        .unwrap();
        assert!(spec.is_satisfied_by(&device_event));
        let system_event = create_test_event(EventLevel::Info);
        assert!(spec.is_satisfied_by(&system_event));
    }

    #[test]
    fn test_event_validation_spec() {
        let spec = EventValidationSpec::new();
        let valid_event = create_test_event(EventLevel::Info);
        assert!(spec.validate(&valid_event).is_ok());
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
        )
        .unwrap();
        assert_eq!(EventPrioritySpec::get_priority(&debug_event), 5);
        assert!(!EventPrioritySpec::requires_immediate_processing(&debug_event));
        assert!(!EventPrioritySpec::should_persist(&debug_event));
    }

    // ── Service tests ──

    #[test]
    fn test_create_event() {
        let service = EventService::new();
        let aggregate = service
            .create_event(
                EventType::System(SystemEventType::UserAuth),
                EventLevel::Info,
                EventSource::system("test".to_string(), None),
                RichContent::new_text("Test".to_string(), "Test content".to_string()),
            )
            .unwrap();
        assert_eq!(aggregate.level(), EventLevel::Info);
    }

    #[test]
    fn test_processing_priority() {
        let service = EventService::new();
        let critical_event = create_test_event(EventLevel::Critical);
        let info_event = create_test_event(EventLevel::Info);
        assert_eq!(service.get_processing_priority(&critical_event), 1);
        assert_eq!(service.get_processing_priority(&info_event), 4);
        assert!(service.requires_immediate_processing(&critical_event));
        assert!(!service.requires_immediate_processing(&info_event));
    }

    #[test]
    fn test_severity_score() {
        let service = EventService::new();
        let critical_event = create_test_event(EventLevel::Critical);
        let info_event = create_test_event(EventLevel::Info);
        let critical_score = service.calculate_severity_score(&critical_event);
        let info_score = service.calculate_severity_score(&info_event);
        assert!(critical_score > info_score);
        assert_eq!(critical_score, 100);
        assert_eq!(info_score, 25);
    }

    #[test]
    fn test_group_events_by_category() {
        let service = EventService::new();
        let system_event = create_test_event(EventLevel::Info);
        let device_event = Event::new(
            EventType::Device(DeviceEventType::Connection),
            EventLevel::Error,
            EventSource::device("device-1".to_string(), Some("Device 1".to_string())),
            RichContent::new_text("Error".to_string(), "Connection lost".to_string()),
        )
        .unwrap();
        let events = [system_event, device_event];
        let groups = service.group_events_by_category(&events);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key("system"));
        assert!(groups.contains_key("device"));
    }

    #[test]
    fn test_filter_events_by_criteria() {
        let service = EventService::new();
        let critical_event = create_test_event(EventLevel::Critical);
        let info_event = create_test_event(EventLevel::Info);
        let events = vec![critical_event, info_event];
        let filtered =
            service.filter_events_by_criteria(&events, Some(EventLevel::Error), None, None, None);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_validate_event_batch() {
        let service = EventService::new();
        let events = vec![create_test_event(EventLevel::Info)];
        assert!(service.validate_event_batch(&events).is_ok());
        let large_batch: Vec<_> = (0..1001).map(|_| create_test_event(EventLevel::Info)).collect();
        assert!(service.validate_event_batch(&large_batch).is_err());
    }

    #[test]
    fn test_detect_event_patterns() {
        let service = EventService::new();
        let source = EventSource::device("device-1".to_string(), Some("Device 1".to_string()));
        let events: Vec<_> = (0..4)
            .map(|_| {
                Event::new(
                    EventType::Device(DeviceEventType::DeviceNormal),
                    EventLevel::Error,
                    source.clone(),
                    RichContent::new_text("Error".to_string(), "Test error".to_string()),
                )
                .unwrap()
            })
            .collect();
        let patterns = service.detect_event_patterns(&events);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].pattern_type, PATTERN_TYPE_REPEATED_ERRORS);
        assert_eq!(patterns[0].event_count, 4);
    }
}

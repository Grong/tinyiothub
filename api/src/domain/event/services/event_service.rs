use std::collections::HashMap;

use crate::domain::event::{
    aggregates::{EventAggregate, NotificationAggregate},
    entities::Event,
    errors::{DomainResult, EventDomainError, EventServiceDomainError},
    specifications::{EventPrioritySpec, EventValidationSpec},
    value_objects::{EventLevel, EventSource, EventType, RichContent},
};

/// Domain service for event processing (pure business logic)
///
/// This service encapsulates the core business rules for event handling,
/// validation, and processing without any infrastructure dependencies.
pub struct EventService {
    validation_spec: EventValidationSpec,
}

impl EventService {
    /// Create a new event service
    pub fn new() -> Self {
        Self { validation_spec: EventValidationSpec::new() }
    }

    /// Create a new event with validation
    pub fn create_event(
        &self,
        event_type: EventType,
        level: EventLevel,
        source: EventSource,
        content: RichContent,
    ) -> DomainResult<EventAggregate> {
        // Create event aggregate
        let aggregate = EventAggregate::new(event_type, level, source, content)
            .map_err(|e| EventDomainError::validation(e.to_string()))?;

        // Validate according to business rules
        self.validation_spec
            .validate(aggregate.event())
            .map_err(|e| EventDomainError::validation(e.to_string()))?;

        // Additional business rule validation
        aggregate
            .validate_for_persistence()
            .map_err(|e| EventDomainError::validation(e.to_string()))?;

        Ok(aggregate)
    }

    /// Process event for notifications (business logic)
    pub fn process_event_for_notifications(
        &self,
        event: &Event,
        notification_rules: &[NotificationAggregate],
    ) -> DomainResult<Vec<String>> {
        let mut matching_rules = Vec::new();

        for rule_aggregate in notification_rules {
            if rule_aggregate.matches_event(&event.event_type().to_string(), &event.level()) {
                matching_rules.push(rule_aggregate.rule().id.clone());
            }
        }

        Ok(matching_rules)
    }

    /// Determine event processing priority (business rule)
    pub fn get_processing_priority(&self, event: &Event) -> u8 {
        EventPrioritySpec::get_priority(event)
    }

    /// Check if event requires immediate processing (business rule)
    pub fn requires_immediate_processing(&self, event: &Event) -> bool {
        EventPrioritySpec::requires_immediate_processing(event)
    }

    /// Check if event should be persisted (business rule)
    pub fn should_persist_event(&self, event: &Event) -> bool {
        EventPrioritySpec::should_persist(event)
    }

    /// Validate event update (business rules)
    pub fn validate_event_update(
        &self,
        current_event: &Event,
        new_content: &RichContent,
    ) -> DomainResult<()> {
        // Business rule: Can only update content within 5 minutes
        let now = chrono::Utc::now();
        let time_diff = now.signed_duration_since(current_event.timestamp());

        if time_diff.num_minutes() > 5 {
            return Err(EventDomainError::immutable(
                "Cannot update event content after 5 minutes".to_string(),
            )
            .into());
        }

        // Business rule: Cannot make content empty
        if new_content.is_empty() {
            return Err(EventDomainError::invalid_content(
                "Event content cannot be empty".to_string(),
            )
            .into());
        }

        Ok(())
    }

    /// Calculate event severity score (business logic)
    pub fn calculate_severity_score(&self, event: &Event) -> u32 {
        let mut score = 0;

        // Base score from level
        score += match event.level() {
            EventLevel::Critical => 100,
            EventLevel::Error => 75,
            EventLevel::Warning => 50,
            EventLevel::Info => 25,
            EventLevel::Debug => 10,
        };

        // Additional score for device events
        if matches!(event.event_type(), EventType::Device(_)) {
            score += 20;
        }

        // Additional score for critical device sources
        if event.source().is_device_critical() {
            score += 30;
        }

        score
    }

    /// Group events by category (business logic)
    pub fn group_events_by_category<'a>(
        &self,
        events: &'a [Event],
    ) -> HashMap<String, Vec<&'a Event>> {
        let mut groups = HashMap::new();

        for event in events {
            let category = match event.event_type() {
                EventType::System(_) => "system",
                EventType::Device(_) => "device",
            };

            groups.entry(category.to_string()).or_insert_with(Vec::new).push(event);
        }

        groups
    }

    /// Filter events by business criteria
    pub fn filter_events_by_criteria<'a>(
        &self,
        events: &'a [Event],
        min_level: Option<EventLevel>,
        event_types: Option<&[EventType]>,
        sources: Option<&[EventSource]>,
        time_range: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    ) -> Vec<&'a Event> {
        events
            .iter()
            .filter(|event| {
                // Filter by level
                if let Some(min_level) = &min_level {
                    if &event.level() < min_level {
                        return false;
                    }
                }

                // Filter by event types
                if let Some(types) = event_types {
                    if !types.contains(event.event_type()) {
                        return false;
                    }
                }

                // Filter by sources
                if let Some(sources) = sources {
                    if !sources.contains(event.source()) {
                        return false;
                    }
                }

                // Filter by time range
                if let Some((start, end)) = time_range {
                    let timestamp = event.timestamp();
                    if timestamp < start || timestamp > end {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// Detect event patterns (business intelligence)
    pub fn detect_event_patterns(&self, events: &[Event]) -> Vec<EventPattern> {
        let mut patterns = Vec::new();

        // Pattern 1: Repeated errors from same source
        let error_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.level(), EventLevel::Error | EventLevel::Critical))
            .collect();

        let mut source_counts = HashMap::new();
        for event in &error_events {
            *source_counts.entry(event.source().clone()).or_insert(0) += 1;
        }

        for (source, count) in source_counts {
            if count >= 3 {
                patterns.push(EventPattern {
                    pattern_type: "repeated_errors".to_string(),
                    description: format!("Repeated errors from source: {}", source.source_id()),
                    severity: if count >= 5 { "high" } else { "medium" }.to_string(),
                    event_count: count,
                    sources: vec![source],
                });
            }
        }

        // Pattern 2: Cascading failures (multiple devices failing in sequence)
        // This would be more complex in a real implementation

        patterns
    }

    /// Validate event batch (business rules)
    pub fn validate_event_batch(&self, events: &[Event]) -> DomainResult<()> {
        // Business rule: Batch size limit
        if events.len() > 1000 {
            return Err(EventServiceDomainError::CapacityExceeded {
                current: events.len(),
                max: 1000,
            }
            .into());
        }

        // Validate each event
        for event in events {
            self.validation_spec
                .validate(event)
                .map_err(|e| EventDomainError::validation(e.to_string()))?;
        }

        Ok(())
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

impl Default for EventService {
    fn default() -> Self {
        Self::new()
    }
}

impl EventService {
    /// Process an event (placeholder method for event bus)
    pub async fn process_event(
        &self,
        _event: &crate::domain::event::entities::Event,
    ) -> Result<(), String> {
        // This would typically process the event through the business logic
        // For now, just return Ok as placeholder
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::{DeviceEventType, SystemEventType};

    fn create_test_event(level: EventLevel) -> Event {
        Event::new(
            EventType::System(SystemEventType::UserAuth),
            level,
            EventSource::system("test".to_string(), None),
            RichContent::new_text("Test".to_string(), "Test content".to_string()),
        )
        .unwrap()
    }

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
        assert_eq!(critical_score, 100); // Base score for critical
        assert_eq!(info_score, 25); // Base score for info
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

        // Filter by minimum level
        let filtered =
            service.filter_events_by_criteria(&events, Some(EventLevel::Error), None, None, None);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_validate_event_batch() {
        let service = EventService::new();

        let events = vec![create_test_event(EventLevel::Info)];
        assert!(service.validate_event_batch(&events).is_ok());

        // Test batch size limit
        let large_batch: Vec<_> = (0..1001).map(|_| create_test_event(EventLevel::Info)).collect();
        assert!(service.validate_event_batch(&large_batch).is_err());
    }

    #[test]
    fn test_detect_event_patterns() {
        let service = EventService::new();

        // Create multiple error events from same source
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
        assert_eq!(patterns[0].pattern_type, "repeated_errors");
        assert_eq!(patterns[0].event_count, 4);
    }
}

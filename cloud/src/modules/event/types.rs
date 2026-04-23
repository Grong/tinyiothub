// Event module types
// Consolidated from domain/event/repositories/*.rs and domain/event/services/event_service.rs

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::modules::event::{
    entities::Event,
    value_objects::{EventId, EventLevel, EventSource, EventType},
    Result,
};

// ──────────────────────────────────────────────
// Event Repository DTOs (from event_repository.rs)
// ──────────────────────────────────────────────

/// Criteria for querying events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCriteria {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub event_types: Option<Vec<EventType>>,
    pub levels: Option<Vec<EventLevel>>,
    pub source_types: Option<Vec<String>>,
    pub device_ids: Option<Vec<String>>,
    pub user_ids: Option<Vec<String>>,
    pub search_text: Option<String>,
    pub sort_by: SortBy,
    pub sort_order: SortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortBy {
    Timestamp,
    Level,
    EventType,
    Source,
}

/// Sort order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Parameters for event statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsParams {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub group_by: GroupBy,
    pub device_ids: Option<Vec<String>>,
}

/// Grouping options for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupBy {
    Level,
    EventType,
    Source,
    Hour,
    Day,
    Week,
    Month,
}

/// Event statistics result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStatistics {
    pub total_count: u64,
    pub groups: Vec<StatisticsGroup>,
}

/// Statistics group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsGroup {
    pub key: String,
    pub count: u64,
    pub percentage: f64,
}

/// Export format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Excel,
}

impl Default for EventCriteria {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            event_types: None,
            levels: None,
            source_types: None,
            device_ids: None,
            user_ids: None,
            search_text: None,
            sort_by: SortBy::Timestamp,
            sort_order: SortOrder::Descending,
            limit: None,
            offset: None,
        }
    }
}

impl EventCriteria {
    pub fn builder() -> EventCriteriaBuilder {
        EventCriteriaBuilder::new()
    }

    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    pub fn with_event_types(mut self, types: Vec<EventType>) -> Self {
        self.event_types = Some(types);
        self
    }

    pub fn with_levels(mut self, levels: Vec<EventLevel>) -> Self {
        self.levels = Some(levels);
        self
    }

    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = Some(device_ids);
        self
    }

    pub fn with_sort(mut self, sort_by: SortBy, sort_order: SortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for EventCriteria
pub struct EventCriteriaBuilder {
    criteria: EventCriteria,
}

impl EventCriteriaBuilder {
    pub fn new() -> Self {
        Self { criteria: EventCriteria::default() }
    }

    pub fn start_time(mut self, start: DateTime<Utc>) -> Self {
        self.criteria.start_time = Some(start);
        self
    }

    pub fn end_time(mut self, end: DateTime<Utc>) -> Self {
        self.criteria.end_time = Some(end);
        self
    }

    pub fn event_types(mut self, types: Vec<EventType>) -> Self {
        self.criteria.event_types = Some(types);
        self
    }

    pub fn levels(mut self, levels: Vec<EventLevel>) -> Self {
        self.criteria.levels = Some(levels);
        self
    }

    pub fn device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.criteria.device_ids = Some(device_ids);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn sort_by(mut self, sort_by: SortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: SortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> EventCriteria {
        self.criteria
    }
}

impl Default for EventCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────
// Real-Time Event DTOs (from real_time_event_repository.rs)
// ──────────────────────────────────────────────

/// Filter for real-time events
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RealTimeFilter {
    pub device_ids: Option<Vec<String>>,
    pub event_types: Option<Vec<EventType>>,
    pub source_types: Option<Vec<String>>,
    pub acknowledged: Option<bool>,
    pub min_level: Option<EventLevel>,
}

impl RealTimeFilter {
    pub fn builder() -> RealTimeFilterBuilder {
        RealTimeFilterBuilder::new()
    }

    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = Some(device_ids);
        self
    }

    pub fn with_event_types(mut self, event_types: Vec<EventType>) -> Self {
        self.event_types = Some(event_types);
        self
    }

    pub fn with_acknowledged(mut self, acknowledged: bool) -> Self {
        self.acknowledged = Some(acknowledged);
        self
    }

    pub fn with_min_level(mut self, level: EventLevel) -> Self {
        self.min_level = Some(level);
        self
    }

    pub fn unacknowledged() -> Self {
        Self::default().with_acknowledged(false)
    }

    pub fn critical_and_errors() -> Self {
        Self::default().with_min_level(EventLevel::Error)
    }
}

/// Builder for RealTimeFilter
pub struct RealTimeFilterBuilder {
    filter: RealTimeFilter,
}

impl RealTimeFilterBuilder {
    pub fn new() -> Self {
        Self { filter: RealTimeFilter::default() }
    }

    pub fn device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.filter.device_ids = Some(device_ids);
        self
    }

    pub fn event_types(mut self, event_types: Vec<EventType>) -> Self {
        self.filter.event_types = Some(event_types);
        self
    }

    pub fn source_types(mut self, source_types: Vec<String>) -> Self {
        self.filter.source_types = Some(source_types);
        self
    }

    pub fn acknowledged(mut self, acknowledged: bool) -> Self {
        self.filter.acknowledged = Some(acknowledged);
        self
    }

    pub fn min_level(mut self, level: EventLevel) -> Self {
        self.filter.min_level = Some(level);
        self
    }

    pub fn build(self) -> RealTimeFilter {
        self.filter
    }
}

impl Default for RealTimeFilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time event representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeEvent {
    pub id: EventId,
    pub event_type: EventType,
    pub level: EventLevel,
    pub source: EventSource,
    pub title: String,
    pub content_preview: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

impl RealTimeEvent {
    pub fn is_critical(&self) -> bool {
        matches!(self.level, EventLevel::Critical)
    }

    pub fn needs_attention(&self) -> bool {
        matches!(self.level, EventLevel::Critical | EventLevel::Error)
    }

    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.timestamp
    }

    pub fn is_stale(&self, threshold: chrono::Duration) -> bool {
        self.age() > threshold
    }
}

/// Status summary for real-time events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    pub total_active: u64,
    pub critical_count: u64,
    pub error_count: u64,
    pub warning_count: u64,
    pub unacknowledged_count: u64,
    pub by_device: Vec<DeviceStatusSummary>,
    pub by_type: Vec<TypeStatusSummary>,
}

impl StatusSummary {
    pub fn has_critical_issues(&self) -> bool {
        self.critical_count > 0
    }

    pub fn has_unacknowledged(&self) -> bool {
        self.unacknowledged_count > 0
    }

    pub fn health_status(&self) -> HealthStatus {
        if self.critical_count > 0 {
            HealthStatus::Critical
        } else if self.error_count > 0 {
            HealthStatus::Error
        } else if self.warning_count > 0 {
            HealthStatus::Warning
        } else {
            HealthStatus::Healthy
        }
    }
}

/// Overall system health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Error,
    Critical,
}

/// Device-specific status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusSummary {
    pub device_id: String,
    pub active_count: u64,
    pub highest_level: EventLevel,
    pub latest_timestamp: DateTime<Utc>,
}

/// Type-specific status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeStatusSummary {
    pub event_type: EventType,
    pub active_count: u64,
    pub highest_level: EventLevel,
}

// ──────────────────────────────────────────────
// Event Pattern (from event_service.rs)
// ──────────────────────────────────────────────

/// Event pattern detection result
#[derive(Debug, Clone)]
pub struct EventPattern {
    pub pattern_type: String,
    pub description: String,
    pub severity: String,
    pub event_count: usize,
    pub sources: Vec<EventSource>,
}

// ──────────────────────────────────────────────
// Tests (from event_repository.rs)
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::value_objects::SystemEventType;

    #[test]
    fn test_criteria_builder() {
        let now = Utc::now();
        let criteria = EventCriteria::builder()
            .start_time(now)
            .event_types(vec![EventType::System(SystemEventType::UserAuth)])
            .levels(vec![EventLevel::Error, EventLevel::Critical])
            .device_ids(vec!["device1".to_string(), "device2".to_string()])
            .sort_by(SortBy::Level)
            .sort_order(SortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.start_time, Some(now));
        assert_eq!(criteria.event_types.as_ref().unwrap().len(), 1);
        assert_eq!(criteria.levels.as_ref().unwrap().len(), 2);
        assert_eq!(criteria.device_ids.as_ref().unwrap().len(), 2);
        assert!(matches!(criteria.sort_by, SortBy::Level));
        assert!(matches!(criteria.sort_order, SortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let now = Utc::now();
        let later = now + chrono::Duration::hours(1);

        let criteria = EventCriteria::default()
            .with_time_range(now, later)
            .with_levels(vec![EventLevel::Critical])
            .with_sort(SortBy::Timestamp, SortOrder::Descending)
            .with_pagination(50, 0);

        assert_eq!(criteria.start_time, Some(now));
        assert_eq!(criteria.end_time, Some(later));
        assert_eq!(criteria.levels.as_ref().unwrap().len(), 1);
        assert_eq!(criteria.limit, Some(50));
    }

    #[test]
    fn test_real_time_filter_builder() {
        let filter = RealTimeFilter::builder()
            .device_ids(vec!["device1".to_string(), "device2".to_string()])
            .event_types(vec![EventType::System(SystemEventType::UserAuth)])
            .acknowledged(false)
            .min_level(EventLevel::Error)
            .build();

        assert_eq!(filter.device_ids.as_ref().unwrap().len(), 2);
        assert_eq!(filter.event_types.as_ref().unwrap().len(), 1);
        assert_eq!(filter.acknowledged, Some(false));
        assert_eq!(filter.min_level, Some(EventLevel::Error));
    }

    #[test]
    fn test_real_time_filter_convenience_methods() {
        let unack_filter = RealTimeFilter::unacknowledged();
        assert_eq!(unack_filter.acknowledged, Some(false));

        let critical_filter = RealTimeFilter::critical_and_errors();
        assert_eq!(critical_filter.min_level, Some(EventLevel::Error));
    }

    #[test]
    fn test_health_status() {
        let mut summary = StatusSummary {
            total_active: 10,
            critical_count: 0,
            error_count: 0,
            warning_count: 5,
            unacknowledged_count: 3,
            by_device: vec![],
            by_type: vec![],
        };

        assert_eq!(summary.health_status(), HealthStatus::Warning);
        assert!(!summary.has_critical_issues());
        assert!(summary.has_unacknowledged());

        summary.critical_count = 1;
        assert_eq!(summary.health_status(), HealthStatus::Critical);
        assert!(summary.has_critical_issues());
    }
}

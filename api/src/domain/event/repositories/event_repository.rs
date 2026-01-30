use crate::domain::event::{
    entities::Event,
    value_objects::{EventId, EventLevel, EventType},
    Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Repository interface for event persistence (defined in domain layer)
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Save a new event
    async fn save(&self, event: &Event) -> Result<()>;

    /// Save multiple events in batch (for performance optimization)
    async fn save_batch(&self, events: &[Event]) -> Result<()> {
        // Default implementation: save one by one
        for event in events {
            self.save(event).await?;
        }
        Ok(())
    }

    /// Find an event by its ID
    async fn find_by_id(&self, id: &EventId) -> Result<Option<Event>>;

    /// Find events matching the given criteria
    async fn find_by_criteria(&self, criteria: &EventCriteria) -> Result<Vec<Event>>;

    /// Count events by level
    async fn count_by_level(&self, level: EventLevel) -> Result<u64>;

    /// Count events by type
    async fn count_by_type(&self, event_type: &EventType) -> Result<u64>;

    /// Get event statistics
    async fn get_statistics(&self, params: &StatisticsParams) -> Result<EventStatistics>;

    /// Clean up old events before the given timestamp
    async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64>;

    /// Export events in the specified format
    async fn export_events(
        &self,
        criteria: &EventCriteria,
        format: ExportFormat,
    ) -> Result<Vec<u8>>;
}

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
    /// Create a new criteria builder
    pub fn builder() -> EventCriteriaBuilder {
        EventCriteriaBuilder::new()
    }

    /// Filter by time range
    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Filter by event types
    pub fn with_event_types(mut self, types: Vec<EventType>) -> Self {
        self.event_types = Some(types);
        self
    }

    /// Filter by levels
    pub fn with_levels(mut self, levels: Vec<EventLevel>) -> Self {
        self.levels = Some(levels);
        self
    }

    /// Filter by device IDs
    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = Some(device_ids);
        self
    }

    /// Set sorting
    pub fn with_sort(mut self, sort_by: SortBy, sort_order: SortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    /// Set pagination
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
        Self {
            criteria: EventCriteria::default(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::SystemEventType;

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
}

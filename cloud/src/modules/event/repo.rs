// Event repository traits
// Migrated from domain/event/repositories/

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::modules::event::{
    entities::Event,
    value_objects::{EventId, EventLevel, EventType},
    Result,
};

pub use super::types::{
    DeviceStatusSummary, EventCriteria, EventStatistics, ExportFormat, GroupBy, RealTimeEvent,
    RealTimeFilter, SortBy, SortOrder, StatisticsGroup, StatisticsParams, StatusSummary,
};

/// Repository interface for event persistence
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Save a new event
    async fn save(&self, event: &Event) -> Result<()>;

    /// Save multiple events in batch
    async fn save_batch(&self, events: &[Event]) -> Result<()> {
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

/// Repository interface for real-time event status
#[async_trait]
pub trait RealTimeEventRepository: Send + Sync {
    /// Insert or update real-time event status
    async fn upsert_status(&self, event: &Event) -> Result<()>;

    /// Remove real-time event status
    async fn remove_status(
        &self,
        source: &crate::modules::event::value_objects::EventSource,
        event_type: &EventType,
    ) -> Result<()>;

    /// Find active events matching the filter
    async fn find_active_events(&self, filter: &RealTimeFilter) -> Result<Vec<RealTimeEvent>>;

    /// Get status summary
    async fn get_status_summary(&self, filter: &RealTimeFilter) -> Result<StatusSummary>;

    /// Acknowledge an event
    async fn acknowledge_event(&self, id: &EventId, user_id: &str) -> Result<()>;

    /// Clear all acknowledged events
    async fn clear_acknowledged_events(&self) -> Result<u64>;

    /// Clean up old real-time events
    async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64>;
}

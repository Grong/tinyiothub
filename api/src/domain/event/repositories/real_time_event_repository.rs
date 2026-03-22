use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::event::{
    entities::Event,
    value_objects::{EventId, EventSource, EventType},
    Result,
};

/// Repository interface for real-time event status (defined in domain layer)
#[async_trait]
pub trait RealTimeEventRepository: Send + Sync {
    /// Insert or update real-time event status
    async fn upsert_status(&self, event: &Event) -> Result<()>;

    /// Remove real-time event status
    async fn remove_status(&self, source: &EventSource, event_type: &EventType) -> Result<()>;

    /// Find active events matching the filter
    async fn find_active_events(&self, filter: &RealTimeFilter) -> Result<Vec<RealTimeEvent>>;

    /// Get status summary
    async fn get_status_summary(&self, filter: &RealTimeFilter) -> Result<StatusSummary>;

    /// Acknowledge an event (mark as acknowledged by user)
    async fn acknowledge_event(&self, id: &EventId, user_id: &str) -> Result<()>;

    /// Clear all acknowledged events
    async fn clear_acknowledged_events(&self) -> Result<u64>;

    /// Clean up old real-time events
    async fn cleanup_old_events(&self, before: DateTime<Utc>) -> Result<u64>;
}

/// Filter for real-time events
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RealTimeFilter {
    pub device_ids: Option<Vec<String>>,
    pub event_types: Option<Vec<EventType>>,
    pub source_types: Option<Vec<String>>,
    pub acknowledged: Option<bool>,
    pub min_level: Option<crate::domain::event::value_objects::EventLevel>,
}

/// Real-time event representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeEvent {
    pub id: EventId,
    pub event_type: EventType,
    pub level: crate::domain::event::value_objects::EventLevel,
    pub source: EventSource,
    pub title: String,
    pub content_preview: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<DateTime<Utc>>,
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

/// Device-specific status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusSummary {
    pub device_id: String,
    pub active_count: u64,
    pub highest_level: crate::domain::event::value_objects::EventLevel,
    pub latest_timestamp: DateTime<Utc>,
}

/// Event type status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeStatusSummary {
    pub event_type: EventType,
    pub active_count: u64,
    pub highest_level: crate::domain::event::value_objects::EventLevel,
}

impl RealTimeFilter {
    /// Create a new filter builder
    pub fn builder() -> RealTimeFilterBuilder {
        RealTimeFilterBuilder::new()
    }

    /// Filter by device IDs
    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = Some(device_ids);
        self
    }

    /// Filter by event types
    pub fn with_event_types(mut self, event_types: Vec<EventType>) -> Self {
        self.event_types = Some(event_types);
        self
    }

    /// Filter by acknowledgment status
    pub fn with_acknowledged(mut self, acknowledged: bool) -> Self {
        self.acknowledged = Some(acknowledged);
        self
    }

    /// Filter by minimum level
    pub fn with_min_level(
        mut self,
        level: crate::domain::event::value_objects::EventLevel,
    ) -> Self {
        self.min_level = Some(level);
        self
    }

    /// Get only unacknowledged events
    pub fn unacknowledged() -> Self {
        Self::default().with_acknowledged(false)
    }

    /// Get only critical and error events
    pub fn critical_and_errors() -> Self {
        Self::default().with_min_level(crate::domain::event::value_objects::EventLevel::Error)
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

    pub fn min_level(mut self, level: crate::domain::event::value_objects::EventLevel) -> Self {
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

impl RealTimeEvent {
    /// Check if this event is critical
    pub fn is_critical(&self) -> bool {
        matches!(self.level, crate::domain::event::value_objects::EventLevel::Critical)
    }

    /// Check if this event needs attention (critical or error)
    pub fn needs_attention(&self) -> bool {
        matches!(
            self.level,
            crate::domain::event::value_objects::EventLevel::Critical
                | crate::domain::event::value_objects::EventLevel::Error
        )
    }

    /// Get age of the event
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.timestamp
    }

    /// Check if event is stale (older than threshold)
    pub fn is_stale(&self, threshold: chrono::Duration) -> bool {
        self.age() > threshold
    }
}

impl StatusSummary {
    /// Check if there are any critical issues
    pub fn has_critical_issues(&self) -> bool {
        self.critical_count > 0
    }

    /// Check if there are any unacknowledged events
    pub fn has_unacknowledged(&self) -> bool {
        self.unacknowledged_count > 0
    }

    /// Get the overall system health status
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::value_objects::{EventLevel, SystemEventType};

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

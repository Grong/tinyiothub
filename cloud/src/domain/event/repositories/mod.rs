// Repository Interfaces
// This module contains repository trait definitions (implemented in infrastructure)

pub mod event_repository;
pub mod real_time_event_repository;

pub use event_repository::{
    EventCriteria, EventRepository, EventStatistics, ExportFormat, GroupBy, SortBy, SortOrder,
    StatisticsGroup, StatisticsParams,
};
pub use real_time_event_repository::{
    DeviceStatusSummary, RealTimeEvent, RealTimeEventRepository, RealTimeFilter, StatusSummary,
};

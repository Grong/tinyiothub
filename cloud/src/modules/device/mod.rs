// Device module — migrated from domain/device/

pub mod diagnostics;
pub mod driver;
pub mod handler;
pub mod monitoring;
pub mod performance;
pub mod query;
pub mod query_service_impl;
pub mod service;

// Legacy management-plane code (trace / trace_repository / types /
// device_query) moved to the thing crate (P4-Task15). Re-exported here so
// existing `modules::device::*` import paths keep working until the
// remaining consumers migrate.
pub use tinyiothub_thing::legacy::device_query;
pub use tinyiothub_thing::legacy::trace;
pub use tinyiothub_thing::legacy::trace_repository;
pub use tinyiothub_thing::legacy::types;

// Backward compatibility aliases (domain::device::trace_service → modules::device::trace)
pub use monitoring as monitoring_service;
pub use monitoring::{DeviceMetrics, DeviceMonitoringService, SystemOverview};
pub use performance as performance_service;
pub use performance::{
    DevicePerformanceMetrics, DevicePerformanceService, PerformanceAlert, SystemPerformanceOverview,
};
pub use query as query_service;
pub use query::DeviceQueryService;
pub use service::DeviceService;
// Backward compatibility: device::repository path
pub use tinyiothub_storage::traits::device as repository;
pub use tinyiothub_storage::traits::device::{
    DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder,
};
pub use tinyiothub_thing::legacy::trace as trace_service;
pub use tinyiothub_thing::legacy::trace::{
    DeviceTrace, DeviceTraceService, DeviceTraceStatistics, SystemTraceOverview,
};

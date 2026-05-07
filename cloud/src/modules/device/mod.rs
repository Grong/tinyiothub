// Device module — migrated from domain/device/

pub mod diagnostics;
pub mod driver;
pub mod handler;
pub mod monitoring;
pub mod performance;
pub mod query;
pub mod service;
pub mod trace;
pub mod types;

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
pub use trace as trace_service;
pub use trace::{DeviceTrace, DeviceTraceService, DeviceTraceStatistics, SystemTraceOverview};

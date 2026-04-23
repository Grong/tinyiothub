// Device module — migrated from domain/device/

pub mod handler;
pub mod service;
pub mod monitoring;
pub mod performance;
pub mod query;
pub mod trace;
pub mod driver;
pub mod diagnostics;
pub mod types;

// Backward compatibility aliases (domain::device::trace_service → modules::device::trace)
pub use trace as trace_service;
pub use monitoring as monitoring_service;
pub use performance as performance_service;
pub use query as query_service;

pub use service::DeviceService;
pub use monitoring::{DeviceMonitoringService, DeviceMetrics, SystemOverview};
pub use performance::{DevicePerformanceService, DevicePerformanceMetrics, SystemPerformanceOverview, PerformanceAlert};
pub use query::DeviceQueryService;
pub use trace::{DeviceTraceService, DeviceTrace, DeviceTraceStatistics, SystemTraceOverview};
pub use tinyiothub_storage::traits::device::{DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder};

// Backward compatibility: device::repository path
pub use tinyiothub_storage::traits::device as repository;

// Device module — migrated from domain/device/

pub mod handler;

// The device-connection runtime face (diagnostics / monitoring / performance
// / query / query_service_impl / service) moved to the driver crate
// (P4-Task20). Re-exported here so existing `modules::device::*` import
// paths keep working until the remaining consumers migrate.
pub use tinyiothub_driver::legacy::{
    DeviceMetrics, DeviceMonitoringService, DevicePerformanceMetrics, DevicePerformanceService,
    DeviceQueryService, DeviceService, PerformanceAlert, SystemOverview, SystemPerformanceOverview,
    monitoring_service, performance_service, query_service, query_service_impl, service,
};
// Backward compatibility: device::repository path
pub use tinyiothub_storage::traits::device as repository;
pub use tinyiothub_storage::traits::device::{
    DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder,
};
// Legacy management-plane code (trace / trace_repository / types /
// device_query) moved to the thing crate (P4-Task15).
pub use tinyiothub_thing::legacy::{
    device_query, trace, trace as trace_service,
    trace::{DeviceTrace, DeviceTraceService, DeviceTraceStatistics, SystemTraceOverview},
    trace_repository, types,
};

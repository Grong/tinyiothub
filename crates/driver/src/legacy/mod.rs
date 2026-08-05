//! Device-legacy connection face — the device-connection runtime plane left
//! behind by the thing extraction (P4-Task15) and reclaimed here
//! (P4-Task20).
//!
//! Boundary notes:
//! - `monitoring`/`performance` hold `Arc<dyn tinyiothub_alarm::AlarmRepository>`
//!   for read-only alarm counts (`count_active_alarms_by_device` etc.) —
//!   driver → alarm is a deliberate one-way edge (real-time alarm context on
//!   device data); the alarm crate never names driver types.
//! - `diagnostics` was dead code in cloud (zero callers) and took
//!   `&Arc<AppState>` directly; here it is pure functions over `Device` /
//!   `DeviceTraceStatistics` / `DeviceCache`. The `harmonyos`-gated
//!   `scan_serial_ports` variant depended on `cloud::shared::hardware`
//!   (composition-layer HAL) and was dropped — the stub returns an empty
//!   list on all platforms. Reclaim candidate if the HarmonyOS build ever
//!   needs serial scanning: move `shared::hardware` to a crate.
//! - `DeviceStatusDistribution` / `QuickDevice` (`types`) moved from
//!   `cloud::modules::monitoring::types`; the remaining monitoring module
//!   (system dashboard) stays in cloud for Task 24.
//! - `service` (`DeviceService`) moved here because `monitoring` constructs
//!   it internally; it is the device data access service and belongs to this
//!   plane. Its cloud consumers (`AppState`, `modules::batch`,
//!   `modules::device::handler`) import it from `tinyiothub_driver`.

pub mod diagnostics;
pub mod monitoring;
pub mod performance;
pub mod query;
pub mod query_service_impl;
pub mod service;
pub mod types;

pub use monitoring as monitoring_service;
pub use monitoring::{DeviceMetrics, DeviceMonitoringService, SystemOverview};
pub use performance as performance_service;
pub use performance::{
    DevicePerformanceMetrics, DevicePerformanceService, PerformanceAlert, SystemPerformanceOverview,
};
pub use query as query_service;
pub use query::DeviceQueryService;
pub use query_service_impl::SqliteDeviceQueryService;
pub use service::DeviceService;
pub use types::{DeviceStatusDistribution, QuickDevice};

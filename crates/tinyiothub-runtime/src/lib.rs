//! TinyIoTHub shared runtime infrastructure
//!
//! Modules:
//! - `data_server` — Driver lifecycle, polling loop, command dispatch
//! - `driver`      — Driver wrapper, retry, status, concrete drivers
//! - `event_bus`   — Event bus and handler dispatch
//! - `cron`        — Cron job executor registry and concrete executors

pub mod cron;
pub mod data_server;
pub mod driver;
pub mod event_bus;

// Re-exports for convenience
pub use cron::ExecutorRegistry;
pub use data_server::DataServer;
pub use driver::{DriverWrapper, create_driver, get_all_driver_names, has_driver};
pub use event_bus::{EventBus, publish_event_safe};

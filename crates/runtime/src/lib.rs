#![allow(clippy::duplicated_attributes)]
//! TinyIoTHub shared runtime infrastructure
//!
//! Modules:
//! - `data_server` — Driver lifecycle, polling loop, command dispatch
//! - `driver`      — Driver wrapper, retry, status, concrete drivers
//! - `event_bus`   — Event bus and handler dispatch
//! - `cron_executors` — Db-bound cron executors (device command, event retention)
//!
//! ## 设计不变量
//! - 禁止依赖 web 与任何领域 crate
//! - unsafe 仅限驱动/plugin 动态加载路径（manifest lint 例外已标注）


pub mod cron_executors;
pub mod data_server;
pub mod driver;
pub mod event_bus;
pub mod plugin;

// Re-exports for convenience
pub use cron_executors::{DeviceCommandExecutor, EventRetentionExecutor};
pub use data_server::DataServer;
pub use driver::{
    DriverWrapper, create_driver, driver_registry, get_all_driver_names, has_driver, registry::DriverRegistry,
};
pub use event_bus::{EventBus, publish_event_safe};

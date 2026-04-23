//! TinyIoTHub business logic engines
//!
//! Modules:
//! - `alarm`     — Alarm management (trigger, manager)
//! - `cron`      — Cron job executor registry
//! - `device`    — Device registry and shadow
//! - `event_bus` — Event bus and handler trait
//! - `pipeline`  — Data processing (decode, transform, route)
//! - `rule`      — Rule engine (parse, evaluate, action)

pub mod alarm;
pub mod application;
pub mod cron;
pub mod device;
pub mod driver;
pub mod event_bus;
pub mod pipeline;
pub mod rule;

pub use alarm::{AlarmManager, AlarmTrigger};
pub use application::DataServer;
pub use cron::{
    AgentExecutor, DeviceCommandExecutor, ExecutionResult, ExecutorError, ExecutorRegistry,
    JobExecutor, ShellExecutor,
};
pub use device::{DeviceRegistry, DeviceShadow};
pub use driver::{
    create_driver, get_all_driver_names, has_driver, load_dynamic_driver, unload_dynamic_driver,
    DeviceDriver, DriverWrapper, ResultValue,
};
pub use event_bus::{publish_event_safe, EventBus, EventHandler};
pub use pipeline::{DataRouter, DataTransformer, ProtocolDecoder};
pub use rule::{RuleAction, RuleEvaluator, RuleParser};

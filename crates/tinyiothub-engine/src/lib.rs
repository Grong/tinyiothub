//! TinyIoTHub business logic engines
//!
//! Modules:
//! - `alarm`    — Alarm management (trigger, manager)
//! - `cron`     — Cron job executor registry
//! - `device`   — Device registry and shadow
//! - `pipeline` — Data processing (decode, transform, route)
//! - `rule`     — Rule engine (parse, evaluate, action)

pub mod alarm;
pub mod cron;
pub mod device;
pub mod pipeline;
pub mod rule;

pub use alarm::{AlarmManager, AlarmTrigger};
pub use cron::{
    AgentExecutor, DeviceCommandExecutor, ExecutionResult, ExecutorError, ExecutorRegistry,
    JobExecutor, ShellExecutor,
};
pub use device::{DeviceRegistry, DeviceShadow};
pub use pipeline::{DataRouter, DataTransformer, ProtocolDecoder};
pub use rule::{RuleAction, RuleEvaluator, RuleParser};

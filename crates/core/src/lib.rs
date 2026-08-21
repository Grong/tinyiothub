#![allow(clippy::double_must_use)] // async_trait 展开的 BoxFuture 自带 must_use，属已知误报类
//! TinyIoTHub contract crate — traits + value types (DTO/error/config).
//!
//! ## 设计不变量
//! - 只许 trait + 值类型（DTO/error/config）；禁止业务逻辑函数与 I/O
//! - 禁止 tokio/axum 依赖；sqlx 仅以 feature 门控的错误转换形式存在
//! - 新类型必须论证为何不属于某个领域 crate

pub mod agent_runs;
pub mod config;
pub mod constants;
pub mod cron;
pub mod driver;
pub mod error;
pub mod event;
pub mod heartbeat;
pub mod memory;
pub mod models;
pub mod notification_types;
pub mod policy;
pub mod types;
pub mod version;

pub use error::{Error, Result};

/// Generate a unique ID using UUID v4
pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate current timestamp as "%Y-%m-%d %H:%M:%S" string (UTC)
pub fn now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

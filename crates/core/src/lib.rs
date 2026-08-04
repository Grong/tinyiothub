pub mod agent_hooks;
pub mod config;
pub mod constants;
pub mod cron;
pub mod driver;
pub mod error;
pub mod event;
pub mod memory;
pub mod models;
pub mod notification_types;
pub mod repository;
pub mod rule;
pub mod thing_hooks;
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

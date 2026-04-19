//! TinyIoTHub configuration management
//!
//! This crate wraps configuration types from `tinyiothub-core` and provides
//! higher-level loading, schema, and validation utilities.
//!
//! The canonical `ApplicationSettings` type remains in `cloud/` for now;
//! this crate serves as the migration target.

pub mod loader;
pub mod schema;
pub mod validation;

pub use tinyiothub_core::config::*;
pub use loader::{load_from_env, load_from_file};
pub use schema::{ApplicationConfig, DatabaseConfig, LoggingConfig, ServerConfig};
pub use validation::{validate_db_url, validate_port};

//! Configuration schema definitions.
//!
//! These types mirror the structure expected in `app_settings.toml`.
//! They are intentionally kept minimal here; the canonical definitions
//! remain in `cloud/src/infrastructure/config/settings.rs` until migration.

use serde::{Deserialize, Serialize};

/// Root application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
}

/// Server listener configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// Database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

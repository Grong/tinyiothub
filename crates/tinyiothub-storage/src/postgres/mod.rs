//! PostgreSQL repository implementations (placeholder).
//!
//! This module will contain PostgreSQL-specific implementations of the
//! repository traits defined in `crate::traits`.
//!
//! To enable: add `postgres` feature to `tinyiothub-storage` Cargo.toml.

/// PostgreSQL connection pool configuration.
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub max_connections: u32,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 5432,
            database: "tinyiothub".into(),
            username: "tinyiothub".into(),
            password: "".into(),
            max_connections: 10,
        }
    }
}

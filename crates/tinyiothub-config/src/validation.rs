//! Configuration validation helpers.

use tinyiothub_core::config::ConfigError;

/// Validate that a port is in the valid range.
pub fn validate_port(port: u16) -> Result<(), ConfigError> {
    if port == 0 {
        return Err(ConfigError::ValidationError("port cannot be 0".into()));
    }
    Ok(())
}

/// Validate that a database URL is non-empty.
pub fn validate_db_url(url: &str) -> Result<(), ConfigError> {
    if url.trim().is_empty() {
        return Err(ConfigError::ValidationError("database url cannot be empty".into()));
    }
    Ok(())
}

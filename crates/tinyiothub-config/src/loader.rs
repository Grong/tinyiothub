//! Configuration loading from files and environment variables.

use std::path::Path;

use tinyiothub_core::config::ConfigError;

/// Load configuration from a TOML file.
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<String, ConfigError> {
    std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigError::FileNotFound(format!("{}: {}", path.as_ref().display(), e)))
}

/// Load configuration from an environment variable.
pub fn load_from_env(key: &str) -> Result<String, ConfigError> {
    std::env::var(key).map_err(|_| ConfigError::FileNotFound(format!("env var {} not set", key)))
}

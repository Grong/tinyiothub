use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:tinyiothub.db".to_string(),
            max_connections: 10,
            min_connections: 2,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
        }
    }
}

impl DatabaseConfig {
    /// Create database config from file path
    pub fn from_file_path(path: &str) -> Self {
        Self {
            url: format!("sqlite:{}", path),
            ..Default::default()
        }
    }

    /// Create database config from application settings
    pub fn from_settings(settings: &crate::infrastructure::config::ApplicationSettings) -> Self {
        let db_config = &settings.database;

        Self {
            url: if db_config.url.starts_with("sqlite:") {
                db_config.url.clone()
            } else {
                format!("sqlite:{}", db_config.url)
            },
            max_connections: db_config.max_connections,
            min_connections: db_config.min_connections,
            acquire_timeout_secs: db_config.connect_timeout_secs,
            idle_timeout_secs: 600, // Default idle timeout
        }
    }

    /// Get connection timeout as Duration
    pub fn acquire_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.acquire_timeout_secs)
    }

    /// Get idle timeout as Duration
    pub fn idle_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.idle_timeout_secs)
    }

    /// Check if using SQLite
    pub fn is_sqlite(&self) -> bool {
        self.url.starts_with("sqlite:")
    }

    /// Get database file path for SQLite
    pub fn sqlite_file_path(&self) -> Option<&str> {
        if self.is_sqlite() {
            Some(&self.url[7..]) // Remove "sqlite:" prefix
        } else {
            None
        }
    }
}

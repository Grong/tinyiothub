//! 存储后端插件
//!
//! 支持 PostgreSQL、InfluxDB 等存储后端。

pub mod handlers;
pub mod config;

pub use config::{StorageConfig, PostgresConfig, InfluxdbConfig};
pub use handlers::{StorageHandler, PostgresHandler, InfluxdbHandler};

use crate::domain::plugin::{PluginHandler, AppContext};
use crate::shared::error::Error;
use std::sync::Arc;

pub struct StorageData {
    pub device_id: String,
    pub timestamp: i64,
    pub values: std::collections::HashMap<String, serde_json::Value>,
}

pub fn create_handler(
    config: &toml::Value,
    _context: Arc<AppContext>,
) -> Result<Box<dyn PluginHandler>, Error> {
    let storage_cfg = config.get("storage")
        .ok_or_else(|| Error::ValidationError("Missing [storage] section".to_string()))?;

    match storage_cfg.get("type").and_then(|v| v.as_str()) {
        Some("postgres") => {
            let cfg: PostgresConfig = storage_cfg.try_into()?;
            Ok(Box::new(PostgresHandler::new(cfg)))
        }
        Some("influxdb") => {
            let cfg: InfluxdbConfig = storage_cfg.try_into()?;
            Ok(Box::new(InfluxdbHandler::new(cfg)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown storage type: {:?}",
            storage_cfg.get("type")
        ))),
    }
}

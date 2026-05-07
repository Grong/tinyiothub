//! 存储后端插件
//!
//! 支持 PostgreSQL、InfluxDB 等存储后端。

pub mod config;
pub mod handlers;

use std::sync::Arc;

pub use config::{InfluxdbConfig, PostgresConfig, StorageConfig};
pub use handlers::{InfluxdbHandler, PostgresHandler, StorageHandler};

use crate::{
    modules::plugin::{AppContext, PluginHandler},
    shared::error::Error,
};

pub struct StorageData {
    pub device_id: String,
    pub timestamp: i64,
    pub values: std::collections::HashMap<String, serde_json::Value>,
}

pub fn create_handler(
    config: &toml::Value,
    _context: Arc<AppContext>,
) -> Result<Box<dyn PluginHandler>, Error> {
    let storage_cfg = config
        .get("storage")
        .ok_or_else(|| Error::ValidationError("Missing [storage] section".to_string()))?;

    match storage_cfg.get("type").and_then(|v| v.as_str()) {
        Some("postgres") => {
            let mut json_val: serde_json::Value = storage_cfg
                .clone()
                .try_into()
                .map_err(|e| Error::ValidationError(format!("Invalid Postgres config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: PostgresConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid Postgres config: {}", e)))?;
            // Use block_on since we're in sync context but handler init is async
            let handler =
                tokio::runtime::Handle::current().block_on(PostgresHandler::new(cfg)).map_err(
                    |e| Error::Internal(format!("Failed to create Postgres handler: {}", e)),
                )?;
            Ok(Box::new(handler) as Box<dyn PluginHandler>)
        }
        Some("influxdb") => {
            let mut json_val: serde_json::Value = storage_cfg
                .clone()
                .try_into()
                .map_err(|e| Error::ValidationError(format!("Invalid InfluxDB config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: InfluxdbConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid InfluxDB config: {}", e)))?;
            // InfluxdbHandler::new is sync, so no block_on needed
            let handler = InfluxdbHandler::new(cfg);
            Ok(Box::new(handler) as Box<dyn PluginHandler>)
        }
        _ => {
            Err(Error::Unsupported(format!("Unknown storage type: {:?}", storage_cfg.get("type"))))
        }
    }
}

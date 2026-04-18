//! 协议驱动插件
//!
//! 支持 HTTP Poller、MQTT 订阅、Modbus TCP、SNMP 等协议。

pub mod handlers;
pub mod config;

pub use config::{HttpPollConfig, MqttConfig};
pub use handlers::{ProtocolHandler, HttpPollHandler, MqttSubscribeHandler};

use crate::domain::plugin::PluginHandler;
use crate::shared::error::Error;

/// 创建协议处理器
pub fn create_handler(config: &toml::Value) -> Result<Box<dyn PluginHandler>, Error> {
    let protocol_cfg = config.get("protocol")
        .ok_or_else(|| Error::ValidationError("Missing [protocol] section".to_string()))?;

    match protocol_cfg.get("type").and_then(|v| v.as_str()) {
        Some("http_poll") => {
            let mut json_val: serde_json::Value = protocol_cfg.clone().try_into()
                .map_err(|e| Error::ValidationError(format!("Invalid HTTP poll config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: HttpPollConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid HTTP poll config: {}", e)))?;
            Ok(Box::new(HttpPollHandler::new(cfg, get_mapping(config)?)))
        }
        Some("mqtt_subscribe") => {
            let mut json_val: serde_json::Value = protocol_cfg.clone().try_into()
                .map_err(|e| Error::ValidationError(format!("Invalid MQTT config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: MqttConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid MQTT config: {}", e)))?;
            Ok(Box::new(MqttSubscribeHandler::new(cfg, get_mapping(config)?)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown protocol type: {:?}",
            protocol_cfg.get("type")
        ))),
    }
}

fn get_mapping(config: &toml::Value) -> Result<std::collections::HashMap<String, String>, Error> {
    config
        .get("mapping")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .ok_or_else(|| Error::ValidationError("Missing [mapping] section".to_string()))
}

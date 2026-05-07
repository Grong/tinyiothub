//! 集成插件
//!
//! 支持微信、钉钉、企业微信等外部系统集成。

pub mod config;
pub mod handlers;

use std::sync::Arc;

pub use config::{IntegrationConfig, WeComConfig, WechatConfig};
pub use handlers::{IntegrationHandler, WeComHandler, WechatHandler};

use crate::{
    modules::plugin::{AppContext, PluginHandler},
    shared::error::Error,
};

pub struct IntegrationRequest {
    pub msg_type: String,
    pub content: String,
    pub extras: std::collections::HashMap<String, String>,
}

pub fn create_handler(
    config: &toml::Value,
    _context: Arc<AppContext>,
) -> Result<Box<dyn PluginHandler>, Error> {
    let integration_cfg = config
        .get("integration")
        .ok_or_else(|| Error::ValidationError("Missing [integration] section".to_string()))?;

    match integration_cfg.get("type").and_then(|v| v.as_str()) {
        Some("wechat") => {
            let mut json_val = serde_json::to_value(integration_cfg)
                .map_err(|e| Error::ValidationError(format!("Invalid WeChat config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: WechatConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid WeChat config: {}", e)))?;
            Ok(Box::new(WechatHandler::new(cfg)))
        }
        Some("wecom") => {
            let mut json_val = serde_json::to_value(integration_cfg)
                .map_err(|e| Error::ValidationError(format!("Invalid WeCom config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: WeComConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid WeCom config: {}", e)))?;
            Ok(Box::new(WeComHandler::new(cfg)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown integration type: {:?}",
            integration_cfg.get("type")
        ))),
    }
}

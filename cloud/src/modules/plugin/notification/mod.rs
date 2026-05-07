//! 通知渠道插件
//!
//! 支持飞书、钉钉、Email 等通知渠道。

pub mod config;
pub mod handlers;

use std::sync::Arc;

pub use config::{DingtalkConfig, FeishuConfig};
pub use handlers::{DingtalkHandler, FeishuHandler, NotificationHandler};

use crate::{
    modules::plugin::{AppContext, PluginHandler},
    shared::error::Error,
};

pub struct Notification {
    pub level: String,
    pub title: String,
    pub content: String,
    pub extras: std::collections::HashMap<String, String>,
}

pub fn create_handler(
    config: &toml::Value,
    _context: Arc<AppContext>,
) -> Result<Box<dyn PluginHandler>, Error> {
    let notification_cfg = config
        .get("notification")
        .ok_or_else(|| Error::ValidationError("Missing [notification] section".to_string()))?;

    match notification_cfg.get("type").and_then(|v| v.as_str()) {
        Some("feishu") => {
            let mut json_val: serde_json::Value = notification_cfg
                .clone()
                .try_into()
                .map_err(|e| Error::ValidationError(format!("Invalid Feishu config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: FeishuConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid Feishu config: {}", e)))?;
            Ok(Box::new(FeishuHandler::new(cfg)))
        }
        Some("dingtalk") => {
            let mut json_val: serde_json::Value = notification_cfg
                .clone()
                .try_into()
                .map_err(|e| Error::ValidationError(format!("Invalid Dingtalk config: {}", e)))?;
            if let Some(obj) = json_val.as_object_mut() {
                obj.remove("type");
            }
            let cfg: DingtalkConfig = serde_json::from_value(json_val)
                .map_err(|e| Error::ValidationError(format!("Invalid Dingtalk config: {}", e)))?;
            Ok(Box::new(DingtalkHandler::new(cfg)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown notification type: {:?}",
            notification_cfg.get("type")
        ))),
    }
}

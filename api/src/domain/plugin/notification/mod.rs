//! 通知渠道插件
//!
//! 支持飞书、钉钉、Email 等通知渠道。

pub mod handlers;
pub mod config;

pub use config::{NotificationConfig, FeishuConfig, DingtalkConfig};
pub use handlers::{NotificationHandler, FeishuHandler, DingtalkHandler};

use crate::domain::plugin::{PluginHandler, AppContext};
use crate::shared::error::Error;
use std::sync::Arc;

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
    let notification_cfg = config.get("notification")
        .ok_or_else(|| Error::ValidationError("Missing [notification] section".to_string()))?;

    match notification_cfg.get("type").and_then(|v| v.as_str()) {
        Some("feishu") => {
            let cfg: FeishuConfig = notification_cfg.try_into()?;
            Ok(Box::new(FeishuHandler::new(cfg)))
        }
        Some("dingtalk") => {
            let cfg: DingtalkConfig = notification_cfg.try_into()?;
            Ok(Box::new(DingtalkHandler::new(cfg)))
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown notification type: {:?}",
            notification_cfg.get("type")
        ))),
    }
}

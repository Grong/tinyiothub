//! 飞书通知处理器

use std::any::Any;
use async_trait::async_trait;
use reqwest::Client;
use tracing::{debug, error};

use super::NotificationHandler;
use crate::modules::plugin::notification::Notification;
use crate::shared::error::Error;

use super::super::config::FeishuConfig;
use crate::modules::plugin::{PluginHandler, PluginManifest, PluginType};

pub struct FeishuHandler {
    config: FeishuConfig,
    client: Client,
    manifest: PluginManifest,
}

impl FeishuHandler {
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            manifest: PluginManifest {
                name: "feishu".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Notification,
                description: Some("Feishu notification handler".to_string()),
            },
        }
    }
}

#[async_trait]
impl NotificationHandler for FeishuHandler {
    async fn send(&self, notification: &Notification) -> Result<(), Error> {
        debug!("Sending Feishu notification: {}", notification.title);

        if !self.config.levels.is_empty()
            && !self.config.levels.contains(&notification.level) {
            return Ok(());
        }

        let payload = serde_json::json!({
            "msg_type": "text",
            "content": {
                "text": format!("[{}] {}\n{}", notification.level, notification.title, notification.content)
            }
        });

        let resp = self.client.post(&self.config.webhook_url)
            .json(&payload)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Feishu request failed: {}", e)))?;

        if !resp.status().is_success() {
            error!("Feishu API returned: {}", resp.status());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "FeishuHandler"
    }
}

impl PluginHandler for FeishuHandler {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn plugin_type(&self) -> PluginType {
        self.manifest.plugin_type
    }
}

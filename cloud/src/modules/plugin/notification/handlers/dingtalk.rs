//! 钉钉通知处理器

use std::any::Any;
use async_trait::async_trait;
use reqwest::Client;
use tracing::{debug, error};

use super::NotificationHandler;
use crate::modules::plugin::notification::Notification;
use crate::shared::error::Error;

use super::super::config::DingtalkConfig;
use crate::modules::plugin::{PluginHandler, PluginManifest, PluginType};

pub struct DingtalkHandler {
    config: DingtalkConfig,
    client: Client,
    manifest: PluginManifest,
}

impl DingtalkHandler {
    pub fn new(config: DingtalkConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            manifest: PluginManifest {
                name: "dingtalk".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Notification,
                description: Some("DingTalk notification handler".to_string()),
            },
        }
    }
}

#[async_trait]
impl NotificationHandler for DingtalkHandler {
    async fn send(&self, notification: &Notification) -> Result<(), Error> {
        debug!("Sending Dingtalk notification: {}", notification.title);

        let payload = serde_json::json!({
            "msgtype": "text",
            "text": {
                "content": format!("[{}] {}\n{}", notification.level, notification.title, notification.content)
            }
        });

        let resp = self.client.post(&self.config.webhook_url)
            .json(&payload)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Dingtalk request failed: {}", e)))?;

        if !resp.status().is_success() {
            error!("Dingtalk API returned: {}", resp.status());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "DingtalkHandler"
    }
}

impl PluginHandler for DingtalkHandler {
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

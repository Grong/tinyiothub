//! 飞书通知处理器

use async_trait::async_trait;
use reqwest::Client;
use tracing::debug;

use super::NotificationHandler;
use crate::domain::plugin::notification::Notification;
use crate::shared::error::Error;

use super::super::config::FeishuConfig;

pub struct FeishuHandler {
    config: FeishuConfig,
    client: Client,
}

impl FeishuHandler {
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            config,
            client: Client::new(),
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

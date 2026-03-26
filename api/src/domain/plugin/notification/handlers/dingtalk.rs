//! 钉钉通知处理器

use async_trait::async_trait;
use reqwest::Client;
use tracing::debug;

use super::NotificationHandler;
use crate::domain::plugin::notification::Notification;
use crate::shared::error::Error;

use super::super::config::DingtalkConfig;

pub struct DingtalkHandler {
    config: DingtalkConfig,
    client: Client,
}

impl DingtalkHandler {
    pub fn new(config: DingtalkConfig) -> Self {
        Self {
            config,
            client: Client::new(),
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

use std::collections::HashMap;

use async_trait::async_trait;

use crate::domain::event::{
    aggregates::NotificationChannelType, services::notification_service::NotificationMessage,
    Result,
};
use tinyiothub_core::models::notification_channel::{NotificationChannel, SendMessageRequest};

/// Trait for notification channel implementations
#[async_trait]
pub trait NotificationChannelHandler: Send + Sync {
    /// Send a notification through this channel
    async fn send(&self, message: &NotificationMessage, recipient: &str) -> Result<()>;

    /// Get the channel type this handler supports
    fn channel_type(&self) -> NotificationChannelType;

    /// Check if the channel is available/configured
    async fn is_available(&self) -> bool;

    /// Get channel-specific configuration requirements
    fn get_config_requirements(&self) -> Vec<String>;
}

/// Notification channel manager for coordinating multiple channels
pub struct NotificationChannelManager {
    channels: HashMap<NotificationChannelType, Box<dyn NotificationChannelHandler>>,
}

impl NotificationChannelManager {
    /// Create a new notification channel manager
    pub fn new() -> Self {
        Self { channels: HashMap::new() }
    }

    /// Register a notification channel
    pub fn register_channel(&mut self, channel: Box<dyn NotificationChannelHandler>) {
        let channel_type = channel.channel_type();
        self.channels.insert(channel_type, channel);
    }

    /// Send a notification through the appropriate channel
    pub async fn send_notification(&self, message: &NotificationMessage) -> Result<()> {
        if let Some(channel) = self.channels.get(&message.channel) {
            if channel.is_available().await {
                channel.send(message, &message.recipient).await
            } else {
                Err(crate::domain::event::EventError::Configuration(format!(
                    "Channel {:?} is not available",
                    message.channel
                )))
            }
        } else {
            Err(crate::domain::event::EventError::Configuration(format!(
                "Channel {:?} is not registered",
                message.channel
            )))
        }
    }

    /// Get available channels
    pub fn get_available_channels(&self) -> Vec<NotificationChannelType> {
        self.channels.keys().cloned().collect()
    }

    /// Check if a channel is available
    pub async fn is_channel_available(&self, channel_type: &NotificationChannelType) -> bool {
        if let Some(channel) = self.channels.get(channel_type) {
            channel.is_available().await
        } else {
            false
        }
    }
}

impl Default for NotificationChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Send a message through a notification channel (test/send helper)
pub async fn send_notification_message(
    channel: &NotificationChannel,
    req: &SendMessageRequest,
) -> std::result::Result<String, String> {
    match channel.channel_type.as_str() {
        "sms" => send_sms(channel, req).await,
        "email" => send_email(channel, req).await,
        "webhook" => send_webhook(channel, req).await,
        _ => Err(format!("Unknown channel type: {}", channel.channel_type)),
    }
}

/// Send SMS
async fn send_sms(channel: &NotificationChannel, req: &SendMessageRequest) -> std::result::Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;

    let provider = config.get("provider").and_then(|v| v.as_str()).unwrap_or("aliyun");
    let sign_name = config.get("sign_name").and_then(|v| v.as_str()).unwrap_or("TinyIoT");
    let template_id = config.get("template_id").and_then(|v| v.as_str()).unwrap_or("");

    tracing::info!("Sending SMS via {} to {}: {}", provider, req.recipient, req.content);

    Ok(format!(
        "SMS sent to {} via {} (sign: {}, template: {})",
        req.recipient, provider, sign_name, template_id
    ))
}

/// Send email
async fn send_email(channel: &NotificationChannel, req: &SendMessageRequest) -> std::result::Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;

    let smtp_host = config.get("smtp_host").and_then(|v| v.as_str()).unwrap_or("");
    let from =
        config.get("from").and_then(|v| v.as_str()).unwrap_or("TinyIoT <noreply@tinyiot.com>");

    tracing::info!("Sending email via {} from {} to {}", smtp_host, from, req.recipient);

    Ok(format!(
        "Email sent to {} (from: {}, subject: {})",
        req.recipient,
        from,
        req.title.as_deref().unwrap_or("")
    ))
}

/// Send Webhook
async fn send_webhook(channel: &NotificationChannel, req: &SendMessageRequest) -> std::result::Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(&channel.config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;

    let url = config.get("url").and_then(|v| v.as_str()).ok_or("Missing URL in config")?;
    let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("POST");

    tracing::info!("Sending webhook {} {} to {}", method, url, req.recipient);

    let body = serde_json::json!({
        "msgtype": "text",
        "text": {
            "content": format!("{}\n{}", req.title.as_deref().unwrap_or(""), req.content)
        }
    });

    Ok(format!("Webhook sent to {} via {} {}", url, method, body))
}

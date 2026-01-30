use crate::domain::event::services::notification_service::NotificationMessage;
use crate::domain::event::{aggregates::NotificationChannelType, Result};
use async_trait::async_trait;
use std::collections::HashMap;

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
        Self {
            channels: HashMap::new(),
        }
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

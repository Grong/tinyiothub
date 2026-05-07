// Infrastructure Layer - Notification Channels
// This module contains notification channel implementations

pub mod email_channel;
pub mod sms_channel;
pub mod sse_channel;

pub use email_channel::EmailNotificationChannel;
pub use sms_channel::SmsNotificationChannel;
pub use sse_channel::SseNotificationChannel;

/// Notification channel factory
pub struct NotificationChannelFactory;

impl NotificationChannelFactory {
    /// Create email notification channel
    pub fn create_email_channel() -> EmailNotificationChannel {
        EmailNotificationChannel::new()
    }

    /// Create SMS notification channel
    pub fn create_sms_channel() -> SmsNotificationChannel {
        SmsNotificationChannel::new()
    }

    /// Create SSE notification channel
    pub fn create_sse_channel() -> SseNotificationChannel {
        SseNotificationChannel::new()
    }

    /// Create all available notification channels
    pub fn create_all_channels()
    -> Vec<Box<dyn crate::modules::notification::types::NotificationChannel>> {
        vec![
            Box::new(Self::create_email_channel()),
            Box::new(Self::create_sms_channel()),
            Box::new(Self::create_sse_channel()),
        ]
    }
}

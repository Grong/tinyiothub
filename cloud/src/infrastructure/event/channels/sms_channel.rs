use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::domain::event::{
    aggregates::notification_aggregate::NotificationChannelType,
    services::{NotificationChannel, NotificationMessage},
    EventError, Result,
};

/// SMS configuration for various providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsConfig {
    pub provider: SmsProvider,
    pub api_key: String,
    pub api_secret: Option<String>,
    pub sender_id: String,
    pub base_url: Option<String>,
}

/// Supported SMS providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SmsProvider {
    /// Twilio SMS service
    Twilio,
    /// Alibaba Cloud SMS
    AlibabaCloud,
    /// Tencent Cloud SMS
    TencentCloud,
    /// Generic HTTP API provider
    Generic,
    /// Mock provider for testing
    Mock,
}

/// SMS message template
#[derive(Debug, Clone)]
pub struct SmsTemplate {
    pub template: String,
    pub max_length: usize,
}

/// SMS notification channel handler
pub struct SmsNotificationChannel {
    config: Option<SmsConfig>,
    template: SmsTemplate,
    enabled: bool,
}

impl SmsNotificationChannel {
    /// Create a new SMS notification channel
    pub fn new() -> Self {
        Self { config: None, template: Self::create_default_template(), enabled: false }
    }

    /// Create with configuration
    pub fn with_config(config: SmsConfig) -> Self {
        Self { config: Some(config), template: Self::create_default_template(), enabled: true }
    }

    /// Set SMS configuration
    pub fn set_config(&mut self, config: SmsConfig) {
        info!("SMS notification channel configured with provider: {:?}", config.provider);
        self.config = Some(config);
        self.enabled = true;
    }

    /// Set SMS template
    pub fn set_template(&mut self, template: SmsTemplate) {
        debug!("Updated SMS template with max length: {}", template.max_length);
        self.template = template;
    }

    /// Enable or disable the SMS channel
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        info!("SMS notification channel {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Create default SMS template
    fn create_default_template() -> SmsTemplate {
        SmsTemplate {
            template: "[IoT Gateway] {level}: {title} - {content} ({timestamp})".to_string(),
            max_length: 160, // Standard SMS length
        }
    }

    /// Format SMS content using template
    fn format_sms(&self, message: &NotificationMessage) -> String {
        let level_str = message.level.as_str().to_uppercase();
        let timestamp = message.timestamp.format("%m/%d %H:%M").to_string();

        let mut content = self
            .template
            .template
            .replace("{level}", &level_str)
            .replace("{title}", &message.title)
            .replace("{content}", &message.content)
            .replace("{timestamp}", &timestamp);

        // Truncate if too long
        if content.len() > self.template.max_length {
            let truncate_pos = self.template.max_length.saturating_sub(3);
            content.truncate(truncate_pos);
            content.push_str("...");
        }

        content
    }

    /// Send SMS using configured provider
    async fn send_sms(&self, to: &str, content: &str) -> Result<()> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| EventError::Configuration("SMS not configured".to_string()))?;

        match config.provider {
            SmsProvider::Twilio => self.send_twilio_sms(config, to, content).await,
            SmsProvider::AlibabaCloud => self.send_alibaba_sms(config, to, content).await,
            SmsProvider::TencentCloud => self.send_tencent_sms(config, to, content).await,
            SmsProvider::Generic => self.send_generic_sms(config, to, content).await,
            SmsProvider::Mock => self.send_mock_sms(config, to, content).await,
        }
    }

    /// Send SMS via Twilio (mock implementation)
    async fn send_twilio_sms(&self, _config: &SmsConfig, to: &str, content: &str) -> Result<()> {
        info!("Sending SMS via Twilio to {}: {}", to, content);

        // In a real implementation, this would use the Twilio API
        // Example using reqwest:
        /*
        let client = reqwest::Client::new();
        let auth = format!("{}:{}", config.api_key, config.api_secret.as_ref().unwrap_or(&String::new()));
        let auth_header = format!("Basic {}", base64::encode(auth));

        let params = [
            ("From", &config.sender_id),
            ("To", to),
            ("Body", content),
        ];

        let response = client
            .post("https://api.twilio.com/2010-04-01/Accounts/{account_sid}/Messages.json")
            .header("Authorization", auth_header)
            .form(&params)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(EventError::Notification(format!("Twilio API error: {}", response.status())))
        }
        */

        // Mock successful send
        Ok(())
    }

    /// Send SMS via Alibaba Cloud (mock implementation)
    async fn send_alibaba_sms(&self, _config: &SmsConfig, to: &str, content: &str) -> Result<()> {
        info!("Sending SMS via Alibaba Cloud to {}: {}", to, content);

        // In a real implementation, this would use the Alibaba Cloud SMS SDK
        // Mock successful send
        Ok(())
    }

    /// Send SMS via Tencent Cloud (mock implementation)
    async fn send_tencent_sms(&self, _config: &SmsConfig, to: &str, content: &str) -> Result<()> {
        info!("Sending SMS via Tencent Cloud to {}: {}", to, content);

        // In a real implementation, this would use the Tencent Cloud SMS SDK
        // Mock successful send
        Ok(())
    }

    /// Send SMS via generic HTTP API (mock implementation)
    async fn send_generic_sms(&self, config: &SmsConfig, to: &str, content: &str) -> Result<()> {
        let base_url = config.base_url.as_ref().ok_or_else(|| {
            EventError::Configuration("Generic SMS provider requires base_url".to_string())
        })?;

        info!("Sending SMS via generic API {} to {}: {}", base_url, to, content);

        // In a real implementation, this would make an HTTP request to the generic API
        // Mock successful send
        Ok(())
    }

    /// Send mock SMS for testing
    async fn send_mock_sms(&self, _config: &SmsConfig, to: &str, content: &str) -> Result<()> {
        info!("Mock SMS to {}: {}", to, content);
        Ok(())
    }

    /// Validate phone number format
    fn is_valid_phone_number(&self, phone: &str) -> bool {
        // Simple phone number validation - in production, use a proper phone validation library
        let cleaned = phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect::<String>();

        // Must start with + and have at least 10 digits
        if cleaned.starts_with('+') && cleaned.len() >= 11 {
            return true;
        }

        // Or be a domestic number with at least 10 digits
        if !cleaned.starts_with('+') && cleaned.len() >= 10 {
            return true;
        }

        false
    }

    /// Normalize phone number format
    fn normalize_phone_number(&self, phone: &str) -> String {
        let cleaned = phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect::<String>();

        // Add + prefix if missing and looks like international number
        if !cleaned.starts_with('+') && cleaned.len() >= 10 {
            format!("+{}", cleaned)
        } else {
            cleaned
        }
    }
}

impl Default for SmsNotificationChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsProvider {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            SmsProvider::Twilio => "twilio",
            SmsProvider::AlibabaCloud => "alibaba_cloud",
            SmsProvider::TencentCloud => "tencent_cloud",
            SmsProvider::Generic => "generic",
            SmsProvider::Mock => "mock",
        }
    }

    /// Parse from string
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "twilio" => Some(SmsProvider::Twilio),
            "alibaba_cloud" | "alibaba" => Some(SmsProvider::AlibabaCloud),
            "tencent_cloud" | "tencent" => Some(SmsProvider::TencentCloud),
            "generic" => Some(SmsProvider::Generic),
            "mock" => Some(SmsProvider::Mock),
            _ => None,
        }
    }

    /// Get default base URL for the provider
    pub fn default_base_url(&self) -> Option<&'static str> {
        match self {
            SmsProvider::Twilio => Some("https://api.twilio.com"),
            SmsProvider::AlibabaCloud => Some("https://dysmsapi.aliyuncs.com"),
            SmsProvider::TencentCloud => Some("https://sms.tencentcloudapi.com"),
            SmsProvider::Generic => None,
            SmsProvider::Mock => None,
        }
    }
}

impl std::fmt::Display for SmsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[async_trait::async_trait]
impl NotificationChannel for SmsNotificationChannel {
    fn channel_type(&self) -> NotificationChannelType {
        NotificationChannelType::Sms
    }

    async fn send(&self, message: &NotificationMessage) -> std::result::Result<(), String> {
        // Use the first recipient from the message
        let recipient =
            message.recipients.first().ok_or_else(|| "No recipients specified".to_string())?;

        if !self.enabled {
            return Err("SMS channel is disabled".to_string());
        }

        let normalized_phone = self.normalize_phone_number(recipient);
        if !self.is_valid_phone_number(&normalized_phone) {
            return Err(format!("Invalid phone number: {}", recipient));
        }

        let content = self.format_sms(message);
        self.send_sms(&normalized_phone, &content).await.map_err(|e| e.to_string())?;

        info!("SMS notification sent to {}: {}", normalized_phone, content);
        Ok(())
    }

    async fn is_available(&self) -> bool {
        self.enabled && self.config.is_some()
    }

    fn get_config(&self) -> std::collections::HashMap<String, String> {
        let mut config = std::collections::HashMap::new();
        if let Some(ref cfg) = self.config {
            config.insert("provider".to_string(), format!("{:?}", cfg.provider));
            config.insert("api_key".to_string(), cfg.api_key.clone());
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::services::{
        NotificationChannel, NotificationLevel, NotificationMessage,
    };

    #[test]
    fn test_sms_channel_creation() {
        let channel = SmsNotificationChannel::new();
        assert!(!channel.enabled);
        assert!(channel.config.is_none());
    }

    #[test]
    fn test_sms_channel_with_config() {
        let config = SmsConfig {
            provider: SmsProvider::Mock,
            api_key: "test_key".to_string(),
            api_secret: Some("test_secret".to_string()),
            sender_id: "IoT Gateway".to_string(),
            base_url: None,
        };

        let channel = SmsNotificationChannel::with_config(config);
        assert!(channel.enabled);
        assert!(channel.config.is_some());
    }

    #[tokio::test]
    async fn test_sms_availability() {
        let mut channel = SmsNotificationChannel::new();
        assert!(!channel.is_available().await);

        let config = SmsConfig {
            provider: SmsProvider::Mock,
            api_key: "test_key".to_string(),
            api_secret: None,
            sender_id: "IoT Gateway".to_string(),
            base_url: None,
        };

        channel.set_config(config);
        assert!(channel.is_available().await);
    }

    #[test]
    fn test_phone_number_validation() {
        let channel = SmsNotificationChannel::new();

        assert!(channel.is_valid_phone_number("+1234567890"));
        assert!(channel.is_valid_phone_number("+86138000000000"));
        assert!(channel.is_valid_phone_number("1234567890"));
        assert!(!channel.is_valid_phone_number("123456789")); // Too short
        assert!(!channel.is_valid_phone_number("invalid"));
        assert!(!channel.is_valid_phone_number(""));
    }

    #[test]
    fn test_phone_number_normalization() {
        let channel = SmsNotificationChannel::new();

        assert_eq!(channel.normalize_phone_number("+1-234-567-8900"), "+12345678900");
        assert_eq!(channel.normalize_phone_number("(123) 456-7890"), "+1234567890");
        assert_eq!(channel.normalize_phone_number("1234567890"), "+1234567890");
        assert_eq!(channel.normalize_phone_number("+86 138 0000 0000"), "+8613800000000");
    }

    #[test]
    fn test_sms_formatting() {
        let channel = SmsNotificationChannel::new();

        let message = NotificationMessage::new(
            "Test Alert".to_string(),
            "This is a test notification".to_string(),
            NotificationLevel::Warning,
            vec![NotificationChannelType::Sms],
            vec!["+1234567890".to_string()],
        );

        let content = channel.format_sms(&message);

        assert!(content.contains("Test Alert"));
        assert!(content.contains("WARNING"));
        assert!(content.contains("This is a test notification"));
        assert!(content.len() <= 160); // Should fit in standard SMS
    }

    #[test]
    fn test_sms_truncation() {
        let mut channel = SmsNotificationChannel::new();

        // Set a very short template for testing truncation
        channel.set_template(SmsTemplate {
            template: "{title}: {content}".to_string(),
            max_length: 20,
        });

        let message = NotificationMessage::new(
            "Very Long Title That Should Be Truncated".to_string(),
            "Very long content that should also be truncated".to_string(),
            NotificationLevel::Info,
            vec![NotificationChannelType::Sms],
            vec!["+1234567890".to_string()],
        );

        let content = channel.format_sms(&message);

        assert!(content.len() <= 20);
        assert!(content.ends_with("..."));
    }

    #[test]
    fn test_sms_provider_conversion() {
        assert_eq!(SmsProvider::Twilio.as_str(), "twilio");
        assert_eq!(SmsProvider::parse_str("twilio"), Some(SmsProvider::Twilio));
        assert_eq!(SmsProvider::parse_str("alibaba"), Some(SmsProvider::AlibabaCloud));
        assert_eq!(SmsProvider::parse_str("invalid"), None);
    }

    #[test]
    fn test_provider_default_urls() {
        assert!(SmsProvider::Twilio.default_base_url().is_some());
        assert!(SmsProvider::AlibabaCloud.default_base_url().is_some());
        assert!(SmsProvider::Generic.default_base_url().is_none());
    }

    #[tokio::test]
    async fn test_send_mock_sms() {
        let config = SmsConfig {
            provider: SmsProvider::Mock,
            api_key: "test_key".to_string(),
            api_secret: None,
            sender_id: "IoT Gateway".to_string(),
            base_url: None,
        };

        let channel = SmsNotificationChannel::with_config(config);

        let message = NotificationMessage::new(
            "Test".to_string(),
            "Content".to_string(),
            NotificationLevel::Info,
            vec![NotificationChannelType::Sms],
            vec!["+1234567890".to_string()],
        );

        let result: std::result::Result<(), String> = channel.send(&message).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_without_config() {
        let channel = SmsNotificationChannel::new();

        let message = NotificationMessage::new(
            "Test".to_string(),
            "Content".to_string(),
            NotificationLevel::Info,
            vec![NotificationChannelType::Sms],
            vec!["+1234567890".to_string()],
        );

        let result: std::result::Result<(), String> = channel.send(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_with_invalid_phone() {
        let config = SmsConfig {
            provider: SmsProvider::Mock,
            api_key: "test_key".to_string(),
            api_secret: None,
            sender_id: "IoT Gateway".to_string(),
            base_url: None,
        };

        let channel = SmsNotificationChannel::with_config(config);

        let message = NotificationMessage::new(
            "Test".to_string(),
            "Content".to_string(),
            NotificationLevel::Info,
            vec![NotificationChannelType::Sms],
            vec!["invalid-phone".to_string()],
        );

        let result: std::result::Result<(), String> = channel.send(&message).await;
        assert!(result.is_err());
    }
}

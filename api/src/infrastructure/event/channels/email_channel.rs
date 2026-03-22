use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::domain::event::{
    aggregates::notification_aggregate::NotificationChannelType,
    services::{NotificationChannel, NotificationLevel, NotificationMessage},
    EventError, Result,
};

/// Email configuration for SMTP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_address: String,
    pub from_name: String,
    pub use_tls: bool,
    pub use_starttls: bool,
}

/// Email message template
#[derive(Debug, Clone)]
pub struct EmailTemplate {
    pub subject_template: String,
    pub body_template: String,
    pub is_html: bool,
}

/// Email notification channel handler
pub struct EmailNotificationChannel {
    config: Option<EmailConfig>,
    templates: HashMap<String, EmailTemplate>,
    enabled: bool,
}

impl EmailNotificationChannel {
    /// Create a new email notification channel
    pub fn new() -> Self {
        Self { config: None, templates: Self::create_default_templates(), enabled: false }
    }

    /// Create with configuration
    pub fn with_config(config: EmailConfig) -> Self {
        Self { config: Some(config), templates: Self::create_default_templates(), enabled: true }
    }

    /// Set email configuration
    pub fn set_config(&mut self, config: EmailConfig) {
        info!("Email notification channel configured with SMTP host: {}", config.smtp_host);
        self.config = Some(config);
        self.enabled = true;
    }

    /// Add or update an email template
    pub fn set_template(&mut self, template_name: String, template: EmailTemplate) {
        debug!("Added email template: {}", template_name);
        self.templates.insert(template_name, template);
    }

    /// Enable or disable the email channel
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        info!("Email notification channel {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Create default email templates
    fn create_default_templates() -> HashMap<String, EmailTemplate> {
        let mut templates = HashMap::new();

        // Default notification template
        templates.insert(
            "default".to_string(),
            EmailTemplate {
                subject_template: "[IoT Gateway] {level}: {title}".to_string(),
                body_template: r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>IoT Gateway Notification</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 0; padding: 20px; background-color: #f5f5f5; }
        .container { max-width: 600px; margin: 0 auto; background-color: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .header { border-bottom: 2px solid #e0e0e0; padding-bottom: 10px; margin-bottom: 20px; }
        .level-critical { color: #d32f2f; }
        .level-error { color: #f57c00; }
        .level-warning { color: #fbc02d; }
        .level-info { color: #1976d2; }
        .content { line-height: 1.6; }
        .metadata { background-color: #f8f9fa; padding: 15px; border-radius: 4px; margin-top: 20px; }
        .footer { margin-top: 30px; padding-top: 20px; border-top: 1px solid #e0e0e0; font-size: 12px; color: #666; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1 class="level-{level_class}">IoT Gateway Alert</h1>
            <p><strong>Level:</strong> <span class="level-{level_class}">{level}</span></p>
            <p><strong>Time:</strong> {timestamp}</p>
        </div>
        
        <div class="content">
            <h2>{title}</h2>
            <p>{content}</p>
        </div>
        
        {metadata_section}
        
        <div class="footer">
            <p>This is an automated notification from your IoT Gateway system.</p>
            <p>If you believe this is an error, please contact your system administrator.</p>
        </div>
    </div>
</body>
</html>
                "#.to_string(),
                is_html: true,
            },
        );

        // Critical alert template
        templates.insert(
            "critical".to_string(),
            EmailTemplate {
                subject_template: "🚨 CRITICAL ALERT: {title}".to_string(),
                body_template: r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Critical Alert</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 0; padding: 20px; background-color: #ffebee; }
        .container { max-width: 600px; margin: 0 auto; background-color: white; padding: 20px; border-radius: 8px; border: 3px solid #d32f2f; }
        .alert-header { background-color: #d32f2f; color: white; padding: 15px; margin: -20px -20px 20px -20px; border-radius: 5px 5px 0 0; }
        .content { line-height: 1.6; }
        .action-required { background-color: #fff3e0; padding: 15px; border-left: 4px solid #ff9800; margin: 20px 0; }
    </style>
</head>
<body>
    <div class="container">
        <div class="alert-header">
            <h1>🚨 CRITICAL SYSTEM ALERT</h1>
            <p>Immediate attention required</p>
        </div>
        
        <div class="content">
            <h2>{title}</h2>
            <p>{content}</p>
            
            <div class="action-required">
                <h3>Action Required</h3>
                <p>This is a critical system event that requires immediate attention. Please investigate and resolve as soon as possible.</p>
            </div>
            
            <p><strong>Time:</strong> {timestamp}</p>
            {metadata_section}
        </div>
    </div>
</body>
</html>
                "#.to_string(),
                is_html: true,
            },
        );

        // Plain text template for simple notifications
        templates.insert(
            "plain".to_string(),
            EmailTemplate {
                subject_template: "[IoT Gateway] {level}: {title}".to_string(),
                body_template: r#"
IoT Gateway Notification

Level: {level}
Title: {title}
Time: {timestamp}

Content:
{content}

{metadata_text}

---
This is an automated notification from your IoT Gateway system.
                "#
                .to_string(),
                is_html: false,
            },
        );

        templates
    }

    /// Get the appropriate template for a notification
    fn get_template(&self, message: &NotificationMessage) -> Result<&EmailTemplate> {
        let template_name = match message.level {
            NotificationLevel::Critical => "critical",
            _ => "default",
        };

        self.templates.get(template_name).or_else(|| self.templates.get("default")).ok_or_else(
            || EventError::Configuration("Default email template not found".to_string()),
        )
    }

    /// Format email content using template
    fn format_email(
        &self,
        message: &NotificationMessage,
        template: &EmailTemplate,
    ) -> (String, String) {
        let level_str = message.level.as_str();
        let level_class = level_str.to_lowercase();
        let timestamp = message.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();

        // Format metadata
        let metadata_section = if !message.metadata.is_empty() {
            if template.is_html {
                let mut metadata_html =
                    String::from("<div class=\"metadata\"><h3>Additional Information</h3><ul>");
                for (key, value) in &message.metadata {
                    metadata_html.push_str(&format!(
                        "<li><strong>{}:</strong> {}</li>",
                        key,
                        match value.as_str() {
                            Some(s) => s.to_string(),
                            None => value.to_string(),
                        }
                    ));
                }
                metadata_html.push_str("</ul></div>");
                metadata_html
            } else {
                let mut metadata_text = String::from("\nAdditional Information:\n");
                for (key, value) in &message.metadata {
                    metadata_text.push_str(&format!(
                        "- {}: {}\n",
                        key,
                        match value.as_str() {
                            Some(s) => s.to_string(),
                            None => value.to_string(),
                        }
                    ));
                }
                metadata_text
            }
        } else {
            String::new()
        };

        // Format subject
        let subject = template
            .subject_template
            .replace("{level}", level_str)
            .replace("{title}", &message.title)
            .replace("{timestamp}", &timestamp);

        // Format body
        let body = template
            .body_template
            .replace("{level}", level_str)
            .replace("{level_class}", &level_class)
            .replace("{title}", &message.title)
            .replace("{content}", &message.content)
            .replace("{timestamp}", &timestamp)
            .replace("{metadata_section}", &metadata_section)
            .replace("{metadata_text}", &metadata_section);

        (subject, body)
    }

    /// Send email using configured SMTP (mock implementation)
    async fn send_email(&self, to: &str, subject: &str, body: &str, is_html: bool) -> Result<()> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| EventError::Configuration("Email not configured".to_string()))?;

        // In a real implementation, this would use an SMTP library like lettre
        // For now, we'll just log the email details
        info!(
            "Sending email via SMTP {}:{} from {} to {}: {}",
            config.smtp_host, config.smtp_port, config.from_address, to, subject
        );

        debug!("Email body (HTML: {}): {}", is_html, body);

        // Mock successful send
        // In production, replace this with actual SMTP sending logic:
        /*
        use lettre::{Message, SmtpTransport, Transport};
        use lettre::transport::smtp::authentication::Credentials;

        let email = Message::builder()
            .from(format!("{} <{}>", config.from_name, config.from_address).parse()?)
            .to(to.parse()?)
            .subject(subject)
            .body(body.to_string())?;

        let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

        let mailer = SmtpTransport::relay(&config.smtp_host)?
            .credentials(creds)
            .build();

        mailer.send(&email)?;
        */

        Ok(())
    }

    /// Validate email address format
    fn is_valid_email(&self, email: &str) -> bool {
        // Simple email validation - in production, use a proper email validation library
        if email.len() < 5 {
            return false;
        }

        let at_count = email.matches('@').count();
        if at_count != 1 {
            return false;
        }

        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 {
            return false;
        }

        let local = parts[0];
        let domain = parts[1];

        // Local part must not be empty
        if local.is_empty() {
            return false;
        }

        // Domain must contain at least one dot and not be empty
        if domain.is_empty() || !domain.contains('.') {
            return false;
        }

        // Domain must not start or end with dot
        if domain.starts_with('.') || domain.ends_with('.') {
            return false;
        }

        true
    }
}

impl Default for EmailNotificationChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl NotificationChannel for EmailNotificationChannel {
    fn channel_type(&self) -> NotificationChannelType {
        NotificationChannelType::Email
    }

    async fn send(&self, message: &NotificationMessage) -> std::result::Result<(), String> {
        // Use the first recipient from the message
        let recipient =
            message.recipients.first().ok_or_else(|| "No recipients specified".to_string())?;

        if !self.enabled {
            return Err("Email channel is disabled".to_string());
        }

        // Validate email address
        if !self.is_valid_email(recipient) {
            return Err(format!("Invalid email address: {}", recipient));
        }

        let template = self.get_template(message).map_err(|e| e.to_string())?;
        let (subject, body) = self.format_email(message, template);

        self.send_email(recipient, &subject, &body, template.is_html)
            .await
            .map_err(|e| e.to_string())?;

        info!("Email notification sent to {}: {}", recipient, subject);
        Ok(())
    }

    async fn is_available(&self) -> bool {
        self.enabled && self.config.is_some()
    }

    fn get_config(&self) -> std::collections::HashMap<String, String> {
        let mut config = std::collections::HashMap::new();
        if let Some(ref cfg) = self.config {
            config.insert("smtp_host".to_string(), cfg.smtp_host.clone());
            config.insert("smtp_port".to_string(), cfg.smtp_port.to_string());
            config.insert("from_address".to_string(), cfg.from_address.clone());
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::services::NotificationMessage;

    #[test]
    fn test_email_channel_creation() {
        let channel = EmailNotificationChannel::new();
        assert!(!channel.enabled);
        assert!(channel.config.is_none());
    }

    #[test]
    fn test_email_channel_with_config() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_username: "user@example.com".to_string(),
            smtp_password: "password".to_string(),
            from_address: "noreply@example.com".to_string(),
            from_name: "IoT Gateway".to_string(),
            use_tls: true,
            use_starttls: false,
        };

        let channel = EmailNotificationChannel::with_config(config);
        assert!(channel.enabled);
        assert!(channel.config.is_some());
    }

    #[tokio::test]
    async fn test_email_availability() {
        let mut channel = EmailNotificationChannel::new();
        assert!(!channel.is_available().await);

        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_username: "user@example.com".to_string(),
            smtp_password: "password".to_string(),
            from_address: "noreply@example.com".to_string(),
            from_name: "IoT Gateway".to_string(),
            use_tls: true,
            use_starttls: false,
        };

        channel.set_config(config);
        assert!(channel.is_available().await);
    }

    #[test]
    fn test_email_validation() {
        let channel = EmailNotificationChannel::new();

        assert!(channel.is_valid_email("user@example.com"));
        assert!(channel.is_valid_email("test.user+tag@domain.co.uk"));
        assert!(!channel.is_valid_email("invalid-email"));
        assert!(!channel.is_valid_email("@example.com"));
        assert!(!channel.is_valid_email("user@"));
    }

    #[test]
    fn test_email_formatting() {
        let channel = EmailNotificationChannel::new();

        let message = NotificationMessage::new(
            "Test Alert".to_string(),
            "This is a test notification".to_string(),
            NotificationLevel::Warning,
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        );

        let template = match channel.get_template(&message) {
            Ok(t) => t,
            Err(e) => panic!("Failed to get template: {}", e),
        };
        let (subject, body): (String, String) = channel.format_email(&message, template);

        assert!(subject.contains("Test Alert"));
        assert!(subject.contains("warning"));
        assert!(body.contains("Test Alert"));
        assert!(body.contains("This is a test notification"));
    }

    #[test]
    fn test_critical_template_selection() {
        let channel = EmailNotificationChannel::new();

        let critical_message = NotificationMessage::new(
            "System Failure".to_string(),
            "Critical system error".to_string(),
            NotificationLevel::Critical,
            vec![NotificationChannelType::Email],
            vec!["admin@example.com".to_string()],
        );

        let template = match channel.get_template(&critical_message) {
            Ok(t) => t,
            Err(e) => panic!("Failed to get template: {}", e),
        };
        let (subject, _): (String, String) = channel.format_email(&critical_message, template);

        assert!(subject.contains("🚨 CRITICAL ALERT"));
    }

    #[test]
    fn test_template_management() {
        let mut channel = EmailNotificationChannel::new();

        let custom_template = EmailTemplate {
            subject_template: "Custom: {title}".to_string(),
            body_template: "Custom body: {content}".to_string(),
            is_html: false,
        };

        channel.set_template("custom".to_string(), custom_template);
        assert!(channel.templates.contains_key("custom"));
    }

    #[tokio::test]
    async fn test_send_without_config() {
        let channel = EmailNotificationChannel::new();

        let message = NotificationMessage::new(
            "Test".to_string(),
            "Content".to_string(),
            NotificationLevel::Info,
            vec![NotificationChannelType::Email],
            vec!["test@example.com".to_string()],
        );

        let result: std::result::Result<(), String> = channel.send(&message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_with_invalid_email() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_username: "user@example.com".to_string(),
            smtp_password: "password".to_string(),
            from_address: "noreply@example.com".to_string(),
            from_name: "IoT Gateway".to_string(),
            use_tls: true,
            use_starttls: false,
        };

        let channel = EmailNotificationChannel::with_config(config);

        let message = NotificationMessage::new(
            "Test".to_string(),
            "Content".to_string(),
            NotificationLevel::Info,
            vec![NotificationChannelType::Email],
            vec!["invalid-email".to_string()],
        );

        let result: std::result::Result<(), String> = channel.send(&message).await;
        assert!(result.is_err());
    }
}

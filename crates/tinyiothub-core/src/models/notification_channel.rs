use serde::{Deserialize, Serialize};

/// 通知渠道类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Sms,
    Email,
    Webhook,
}

impl ChannelType {
    pub fn as_str(&self) -> &str {
        match self {
            ChannelType::Sms => "sms",
            ChannelType::Email => "email",
            ChannelType::Webhook => "webhook",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sms" => Some(ChannelType::Sms),
            "email" => Some(ChannelType::Email),
            "webhook" => Some(ChannelType::Webhook),
            _ => None,
        }
    }
}

/// 通知渠道实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub config: String,
    pub is_enabled: bool,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct NotificationChannelQueryParams {
    pub channel_type: Option<String>,
    pub is_enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    pub channel_type: String,
    pub config: String,
    pub description: Option<String>,
}

/// 更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateNotificationChannelRequest {
    pub name: Option<String>,
    pub channel_type: Option<String>,
    pub config: Option<String>,
    pub description: Option<String>,
}

/// 发送消息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendMessageRequest {
    pub recipient: String,     // 手机号/邮箱/ webhook 地址
    pub title: Option<String>, // 标题（邮件/短信）
    pub content: String,       // 消息内容
}

/// 渠道统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelStatistics {
    pub total_channels: i64,
    pub enabled_channels: i64,
    pub sms_channels: i64,
    pub email_channels: i64,
    pub webhook_channels: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_as_str() {
        assert_eq!(ChannelType::Sms.as_str(), "sms");
        assert_eq!(ChannelType::Email.as_str(), "email");
        assert_eq!(ChannelType::Webhook.as_str(), "webhook");
    }

    #[test]
    fn test_channel_type_from_str() {
        assert_eq!(ChannelType::parse_str("sms"), Some(ChannelType::Sms));
        assert_eq!(ChannelType::parse_str("email"), Some(ChannelType::Email));
        assert_eq!(ChannelType::parse_str("webhook"), Some(ChannelType::Webhook));
        assert_eq!(ChannelType::parse_str("unknown"), None);
    }

    #[test]
    fn test_create_channel_request() {
        let req = CreateNotificationChannelRequest {
            name: "Test SMS".to_string(),
            channel_type: "sms".to_string(),
            config: r#"{"provider": "aliyun", "sign_name": "Test"}"#.to_string(),
            description: Some("Test channel".to_string()),
        };

        assert_eq!(req.name, "Test SMS");
        assert_eq!(req.channel_type, "sms");
        assert!(req.description.is_some());
    }

    #[test]
    fn test_update_channel_request() {
        let req = UpdateNotificationChannelRequest {
            name: Some("Updated SMS".to_string()),
            channel_type: None,
            config: None,
            description: None,
        };

        assert_eq!(req.name, Some("Updated SMS".to_string()));
    }

    #[test]
    fn test_send_message_request() {
        let req = SendMessageRequest {
            recipient: "13800138000".to_string(),
            title: Some("Test Title".to_string()),
            content: "Test content".to_string(),
        };

        assert_eq!(req.recipient, "13800138000");
        assert_eq!(req.title, Some("Test Title".to_string()));
    }

    #[test]
    fn test_channel_statistics() {
        let stats = ChannelStatistics {
            total_channels: 10,
            enabled_channels: 8,
            sms_channels: 3,
            email_channels: 3,
            webhook_channels: 4,
        };

        assert_eq!(stats.total_channels, 10);
        assert_eq!(stats.enabled_channels, 8);
    }
}

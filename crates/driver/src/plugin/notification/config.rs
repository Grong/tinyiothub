//! 通知渠道配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationConfig {
    #[serde(rename = "type")]
    pub notification_type: String,
    #[serde(default)]
    pub levels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuConfig {
    pub webhook_url: String,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub levels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DingtalkConfig {
    pub webhook_url: String,
    pub secret: Option<String>,
    #[serde(default)]
    pub levels: Vec<String>,
}

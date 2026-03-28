//! 集成配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationConfig {
    #[serde(rename = "type")]
    pub integration_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WechatConfig {
    pub app_id: String,
    pub app_secret: String,
    pub to_user: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeComConfig {
    pub corp_id: String,
    pub corp_secret: String,
    pub agent_id: String,
    pub party_id: Option<String>,
    pub tag_id: Option<String>,
}

//! 微信集成处理器

use std::any::Any;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::domain::plugin::integration::IntegrationRequest;
use crate::domain::plugin::{PluginHandler, PluginManifest, PluginType};
use crate::shared::error::Error;

use crate::domain::plugin::integration::handlers::IntegrationHandler;
use super::super::config::WechatConfig;

#[derive(Serialize)]
struct WechatSendRequest {
    touser: String,
    msgtype: String,
    agentid: Option<String>,
    text: serde_json::Value,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct WechatAccessTokenResponse {
    access_token: String,
    expires_in: i64,
}

pub struct WechatHandler {
    config: WechatConfig,
    client: Client,
    manifest: PluginManifest,
}

impl WechatHandler {
    pub fn new(config: WechatConfig) -> Self {
        Self {
            manifest: PluginManifest {
                name: "wechat".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Integration,
                description: Some("WeChat integration handler".to_string()),
            },
            config,
            client: Client::new(),
        }
    }

    async fn get_access_token(&self) -> Result<String, Error> {
        let url = format!(
            "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid={}&secret={}",
            self.config.app_id, self.config.app_secret
        );

        let resp = self.client.get(&url)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Failed to get WeChat access token: {}", e)))?;

        let token_resp: WechatAccessTokenResponse = resp.json().await
            .map_err(|e| Error::NetworkError(format!("Failed to parse WeChat response: {}", e)))?;

        Ok(token_resp.access_token)
    }
}

#[async_trait]
impl IntegrationHandler for WechatHandler {
    async fn send(&self, request: &IntegrationRequest) -> Result<(), Error> {
        debug!("Sending WeChat message: {}", request.content);

        let access_token = self.get_access_token().await?;
        let url = format!(
            "https://api.weixin.qq.com/cgi-bin/message/custom/send?access_token={}",
            access_token
        );

        let send_req = WechatSendRequest {
            touser: self.config.to_user.clone(),
            msgtype: request.msg_type.clone(),
            agentid: self.config.agent_id.clone(),
            text: serde_json::json!({ "content": request.content.clone() }),
        };

        let resp = self.client.post(&url)
            .json(&send_req)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Failed to send WeChat message: {}", e)))?;

        if !resp.status().is_success() {
            error!("WeChat API returned: {}", resp.status());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "WechatHandler"
    }
}

impl PluginHandler for WechatHandler {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn plugin_type(&self) -> PluginType {
        self.manifest.plugin_type
    }
}

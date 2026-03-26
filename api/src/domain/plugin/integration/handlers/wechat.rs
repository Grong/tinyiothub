//! 微信集成处理器

use std::sync::Arc;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use super::IntegrationHandler;
use crate::domain::plugin::integration::IntegrationRequest;
use crate::shared::error::Error;

use super::super::config::WechatConfig;

#[derive(Serialize)]
struct WechatSendRequest {
    touser: String,
    msgtype: String,
    agentid: Option<String>,
    text: serde_json::Value,
}

#[derive(Deserialize)]
struct WechatAccessTokenResponse {
    access_token: String,
    expires_in: i64,
}

pub struct WechatHandler {
    config: WechatConfig,
    client: Client,
    access_token: std::sync::Arc<tokio::sync::RwLock<Option<String>>>,
}

impl WechatHandler {
    pub fn new(config: WechatConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            access_token: Arc::new(tokio::sync::RwLock::new(None)),
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

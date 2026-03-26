//! 企业微信集成处理器

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use super::IntegrationHandler;
use crate::domain::plugin::integration::IntegrationRequest;
use crate::shared::error::Error;

use super::super::config::WeComConfig;

#[derive(Serialize)]
struct WeComSendRequest {
    touser: Option<String>,
    toparty: Option<String>,
    totag: Option<String>,
    msgtype: String,
    agentid: String,
    text: serde_json::Value,
}

#[derive(Deserialize)]
struct WeComAccessTokenResponse {
    access_token: String,
    expires_in: i64,
}

pub struct WeComHandler {
    config: WeComConfig,
    client: Client,
}

impl WeComHandler {
    pub fn new(config: WeComConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    async fn get_access_token(&self) -> Result<String, Error> {
        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
            self.config.corp_id, self.config.corp_secret
        );

        let resp = self.client.get(&url)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Failed to get WeCom access token: {}", e)))?;

        let token_resp: WeComAccessTokenResponse = resp.json().await
            .map_err(|e| Error::NetworkError(format!("Failed to parse WeCom response: {}", e)))?;

        Ok(token_resp.access_token)
    }
}

#[async_trait]
impl IntegrationHandler for WeComHandler {
    async fn send(&self, request: &IntegrationRequest) -> Result<(), Error> {
        debug!("Sending WeCom message: {}", request.content);

        let access_token = self.get_access_token().await?;
        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={}",
            access_token
        );

        let send_req = WeComSendRequest {
            touser: if self.config.party_id.is_some() { None } else { Some("@all".to_string()) },
            toparty: self.config.party_id.clone(),
            totag: self.config.tag_id.clone(),
            msgtype: request.msg_type.clone(),
            agentid: self.config.agent_id.clone(),
            text: serde_json::json!({ "content": request.content.clone() }),
        };

        let resp = self.client.post(&url)
            .json(&send_req)
            .send().await
            .map_err(|e| Error::NetworkError(format!("Failed to send WeCom message: {}", e)))?;

        if !resp.status().is_success() {
            error!("WeCom API returned: {}", resp.status());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "WeComHandler"
    }
}

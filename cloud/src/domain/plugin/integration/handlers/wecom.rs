//! 企业微信集成处理器

use std::any::Any;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::domain::plugin::integration::IntegrationRequest;
use crate::domain::plugin::{PluginHandler, PluginManifest, PluginType};
use crate::shared::error::Error;

use crate::domain::plugin::integration::handlers::IntegrationHandler;
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
#[allow(dead_code)]
struct WeComAccessTokenResponse {
    access_token: String,
    expires_in: i64,
}

pub struct WeComHandler {
    config: WeComConfig,
    client: Client,
    manifest: PluginManifest,
}

impl WeComHandler {
    pub fn new(config: WeComConfig) -> Self {
        Self {
            manifest: PluginManifest {
                name: "wecom".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Integration,
                description: Some("Enterprise WeChat integration handler".to_string()),
            },
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

impl PluginHandler for WeComHandler {
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

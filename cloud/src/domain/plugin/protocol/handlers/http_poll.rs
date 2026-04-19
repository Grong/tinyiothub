//! HTTP 轮询协议处理器

use std::any::Any;
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

use super::ProtocolHandler;
use crate::dto::entity::device::Device;
use crate::{
    domain::device::driver::ResultValue,
    shared::error::Error
};

use super::super::config::HttpPollConfig;
use crate::domain::plugin::{PluginHandler, PluginManifest, PluginType};

pub struct HttpPollHandler {
    config: HttpPollConfig,
    mapping: HashMap<String, String>,
    client: Client,
    manifest: PluginManifest,
}

impl HttpPollHandler {
    pub fn new(config: HttpPollConfig, mapping: HashMap<String, String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client build failed");

        Self {
            config,
            mapping,
            client,
            manifest: PluginManifest {
                name: "http_poll".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Protocol,
                description: Some("HTTP Polling protocol handler".to_string()),
            },
        }
    }

    fn build_url(&self) -> String {
        format!(
            "{}{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.endpoint
        )
    }
}

#[async_trait]
impl ProtocolHandler for HttpPollHandler {
    async fn read_data(&self, _device: &Device) -> Result<Vec<ResultValue>, Error> {
        let url = self.build_url();
        debug!("HTTP poll: {} {}", self.config.method, url);

        let mut request = self.client.request(
            reqwest::Method::from_bytes(self.config.method.as_bytes())
                .unwrap_or(reqwest::Method::GET),
            &url,
        );

        if let Some(ref auth) = self.config.auth {
            match auth.auth_type.as_str() {
                "basic" => {
                    if let (Some(u), Some(p)) = (&auth.username, &auth.password) {
                        request = request.basic_auth(u, Some(p));
                    }
                }
                "bearer" => {
                    if let Some(ref token) = auth.token {
                        request = request.bearer_auth(token);
                    }
                }
                _ => {}
            }
        }

        for (k, v) in &self.config.headers {
            request = request.header(k, v);
        }

        let resp = request.send().await
            .map_err(|e| Error::NetworkError(format!("HTTP request failed: {}", e)))?;

        let body = resp.text().await
            .map_err(|e| Error::IOError(format!("Failed to read response: {}", e)))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| Error::ValidationError(format!("Invalid JSON: {}", e)))?;

        let mut results = Vec::new();
        for (field_name, path) in &self.mapping {
            if let Some(value) = self.extract_json_path(&json, path) {
                results.push(self.json_to_result_value(field_name.clone(), value));
            }
        }

        Ok(results)
    }
}

impl HttpPollHandler {
    fn extract_json_path(&self, json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
        let path = path.trim_start_matches("$.吃掉");
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current.clone())
    }

    fn json_to_result_value(&self, name: String, value: serde_json::Value) -> ResultValue {
        use crate::domain::device::driver::ResultValue;
        match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ResultValue::integer(name, i)
                } else if let Some(f) = n.as_f64() {
                    ResultValue::float(name, f)
                } else {
                    ResultValue::string(name, n.to_string())
                }
            }
            serde_json::Value::Bool(b) => ResultValue::boolean(name, b),
            serde_json::Value::String(s) => ResultValue::string(name, s),
            _ => ResultValue::string(name, value.to_string()),
        }
    }
}

impl PluginHandler for HttpPollHandler {
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

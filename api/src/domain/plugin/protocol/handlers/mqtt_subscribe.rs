//! MQTT 订阅协议处理器

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use super::ProtocolHandler;
use crate::{
    domain::device::driver::ResultValue,
    dto::entity::Device,
    shared::error::Error,
};

use super::super::config::MqttConfig;

pub struct MqttSubscribeHandler {
    config: MqttConfig,
    mapping: HashMap<String, String>,
    last_message: Arc<RwLock<Option<String>>>,
}

impl MqttSubscribeHandler {
    pub fn new(config: MqttConfig, mapping: HashMap<String, String>) -> Self {
        Self {
            config,
            mapping,
            last_message: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl ProtocolHandler for MqttSubscribeHandler {
    async fn read_data(&self, _device: &Device) -> Result<Vec<ResultValue>, Error> {
        debug!("MQTT subscribe handler called");

        let last = self.last_message.read().await;
        let body = match last.as_ref() {
            Some(msg) => msg.clone(),
            None => return Ok(vec![]),
        };

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| Error::ValidationError(format!("Invalid MQTT JSON: {}", e)))?;

        let mut results = Vec::new();
        for (field_name, path) in &self.mapping {
            if let Some(value) = self.extract_json_path(&json, path) {
                results.push(self.json_to_result_value(field_name.clone(), value));
            }
        }

        Ok(results)
    }
}

impl MqttSubscribeHandler {
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

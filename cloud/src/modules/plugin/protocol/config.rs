//! 协议驱动配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct HttpPollConfig {
    pub base_url: String,
    pub endpoint: String,
    #[serde(default = "default_get")]
    pub method: String,
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auth: Option<HttpAuth>,
}

fn default_get() -> String { "GET".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct HttpAuth {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttConfig {
    pub broker_url: String,
    pub client_id: Option<String>,
    pub topic: String,
    pub qos: Option<u8>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModbusConfig {
    pub host: String,
    pub port: u16,
    pub slave_id: u8,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 { 5000 }

#[derive(Debug, Clone, Deserialize)]
pub struct SnmpConfig {
    pub host: String,
    pub port: u16,
    pub community: String,
    pub oid: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolType {
    HttpPoll,
    MqttSubscribe,
    ModbusTcp,
    SnmpGet,
}

//! 存储后端配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    pub storage_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresConfig {
    pub connection_string: String,
    #[serde(default = "default_table")]
    pub table_name: String,
}

fn default_table() -> String {
    "device_data".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfluxdbConfig {
    pub url: String,
    pub org: String,
    pub bucket: String,
    pub token: String,
    #[serde(default)]
    pub measurement: Option<String>,
}

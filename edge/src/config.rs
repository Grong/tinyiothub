use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EdgeConfig {
    pub mqtt_broker: String,
    pub mqtt_port: u16,
    pub pairing_interval_secs: u64,
    pub heartbeat_interval_secs: u64,
    pub telemetry_interval_secs: u64,
    pub intelligence_interval_secs: u64,
    pub offline_buffer_max_telemetry: usize,
    pub offline_buffer_disk_min_percent: u8,
    pub offline_buffer_reserved_mb: u64,
    pub local_api_enabled: bool,
    pub local_api_port: u16,
    pub local_api_key: Option<String>,
    pub credentials_file: PathBuf,
    pub config_file: PathBuf,
    pub db_path: PathBuf,
    pub scan_timeout_secs: u64,
    pub mqtt_reconnect_max_backoff_secs: u64,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            mqtt_broker: "mqtt.tinyiothub.com".into(),
            mqtt_port: 1883,
            pairing_interval_secs: 30,
            heartbeat_interval_secs: 30,
            telemetry_interval_secs: 30,
            intelligence_interval_secs: 60,
            offline_buffer_max_telemetry: 100_000,
            offline_buffer_disk_min_percent: 10,
            offline_buffer_reserved_mb: 5,
            local_api_enabled: false,
            local_api_port: 8080,
            local_api_key: None,
            credentials_file: PathBuf::from("/app/data/credentials.json"),
            config_file: PathBuf::from("/app/data/config.yaml"),
            db_path: PathBuf::from("/app/data/edge.db"),
            scan_timeout_secs: 10,
            mqtt_reconnect_max_backoff_secs: 300,
        }
    }
}

impl EdgeConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(v) = std::env::var("EDGE_MQTT_BROKER") {
            config.mqtt_broker = v;
        }
        if let Ok(v) = std::env::var("EDGE_MQTT_PORT") {
            config.mqtt_port = v.parse().unwrap_or(1883);
        }
        if let Ok(v) = std::env::var("EDGE_TELEMETRY_INTERVAL") {
            config.telemetry_interval_secs = v.parse().unwrap_or(30);
        }
        if let Ok(v) = std::env::var("EDGE_INTELLIGENCE_INTERVAL") {
            config.intelligence_interval_secs = v.parse().unwrap_or(60);
        }
        if let Ok(v) = std::env::var("EDGE_LOCAL_API") {
            config.local_api_enabled = v == "1";
        }
        if let Ok(v) = std::env::var("EDGE_LOCAL_API_PORT") {
            config.local_api_port = v.parse().unwrap_or(8080);
        }
        if let Ok(v) = std::env::var("EDGE_LOCAL_API_KEY") {
            config.local_api_key = Some(v);
        }
        if let Ok(v) = std::env::var("EDGE_CREDENTIALS_FILE") {
            config.credentials_file = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("EDGE_CONFIG_FILE") {
            config.config_file = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("EDGE_DB_PATH") {
            config.db_path = PathBuf::from(v);
        }
        config
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayCredentials {
    pub device_id: String,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub workspace_id: String,
}

impl GatewayCredentials {
    pub fn load(path: &PathBuf) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.device_id.is_empty() {
            return Err("device_id is empty");
        }
        if self.client_id.is_empty() {
            return Err("client_id is empty");
        }
        if self.username.is_empty() {
            return Err("username is empty");
        }
        if self.password.is_empty() {
            return Err("password is empty");
        }
        if self.workspace_id.is_empty() {
            return Err("workspace_id is empty");
        }
        Ok(())
    }
}

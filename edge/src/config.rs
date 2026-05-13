use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeConfig {
    pub mqtt_broker: String,
    pub mqtt_port: u16,
    pub pairing_interval_secs: u64,
    pub heartbeat_interval_secs: u64,
    pub credentials_file: PathBuf,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            mqtt_broker: "mqtt.tinyiothub.com".into(),
            mqtt_port: 1883,
            pairing_interval_secs: 30,
            heartbeat_interval_secs: 30,
            credentials_file: PathBuf::from("/app/data/credentials.json"),
        }
    }
}

impl EdgeConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(broker) = std::env::var("EDGE_MQTT_BROKER") {
            config.mqtt_broker = broker;
        }
        if let Ok(port) = std::env::var("EDGE_MQTT_PORT") {
            config.mqtt_port = port.parse().unwrap_or(1883);
        }
        if let Ok(path) = std::env::var("EDGE_CREDENTIALS_FILE") {
            config.credentials_file = PathBuf::from(path);
        }
        config
    }
}

#[derive(Debug, Serialize, Deserialize)]
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

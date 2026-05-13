use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum GatewayMessage {
    ConfigDevice(ConfigDevicePayload),
    Config(serde_json::Value),
    Command(serde_json::Value),
    DriverInstall(DriverInstallPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDevicePayload {
    pub device_id: String,
    pub action: String,
    #[serde(default)]
    pub property: Option<String>,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverInstallPayload {
    pub driver_name: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub sha256: String,
    pub data: String,
}

impl GatewayMessage {
    /// Parse topic+payload with longest-prefix matching.
    /// Longer suffixes (/config/device, /driver/install) are checked before
    /// shorter ones (/config, /command) to avoid false matches.
    pub fn from_topic_payload(topic: &str, payload: &[u8]) -> Result<Self, String> {
        // Check longest prefix first: /config/device before /config
        if topic.ends_with("/config/device") {
            let inner: ConfigDevicePayload = serde_json::from_slice(payload)
                .map_err(|e| format!("ConfigDevice parse error: {}", e))?;
            return Ok(GatewayMessage::ConfigDevice(inner));
        }
        if topic.ends_with("/driver/install") {
            let inner: DriverInstallPayload = serde_json::from_slice(payload)
                .map_err(|e| format!("DriverInstall parse error: {}", e))?;
            return Ok(GatewayMessage::DriverInstall(inner));
        }
        if topic.ends_with("/config") {
            let v: serde_json::Value = serde_json::from_slice(payload)
                .map_err(|e| format!("Config parse error: {}", e))?;
            return Ok(GatewayMessage::Config(v));
        }
        if topic.ends_with("/command") {
            let v: serde_json::Value = serde_json::from_slice(payload)
                .map_err(|e| format!("Command parse error: {}", e))?;
            return Ok(GatewayMessage::Command(v));
        }
        Err(format!("unknown topic: {}", topic))
    }

    pub fn driver_name(&self) -> Option<&str> {
        match self {
            GatewayMessage::DriverInstall(p) => Some(&p.driver_name),
            _ => None,
        }
    }
}

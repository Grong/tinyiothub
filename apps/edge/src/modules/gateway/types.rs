use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum GatewayMessage {
    ConfigThing(ConfigThingPayload),
    Config(serde_json::Value),
    Command(serde_json::Value),
    DriverInstall(DriverInstallPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigThingPayload {
    pub thing_id: String,
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

/// 子设备发现消息（MQTT，网关→平台）— 与 cloud `ThingDiscoverMessage` 同形
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ThingDiscoverMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub things: Vec<DiscoveredThing>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredThing {
    pub name: String,
    pub category: Option<String>,
    pub protocol_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
}

impl GatewayMessage {
    /// Parse topic+payload with longest-prefix matching.
    /// Longer suffixes (/config/thing, /driver/install) are checked before
    /// shorter ones (/config, /command) to avoid false matches.
    pub fn from_topic_payload(topic: &str, payload: &[u8]) -> Result<Self, String> {
        // Check longest prefix first: /config/thing before /config
        if topic.ends_with("/config/thing") {
            let inner: ConfigThingPayload =
                serde_json::from_slice(payload).map_err(|e| format!("ConfigThing parse error: {}", e))?;
            return Ok(GatewayMessage::ConfigThing(inner));
        }
        if topic.ends_with("/driver/install") {
            let inner: DriverInstallPayload =
                serde_json::from_slice(payload).map_err(|e| format!("DriverInstall parse error: {}", e))?;
            return Ok(GatewayMessage::DriverInstall(inner));
        }
        if topic.ends_with("/config") {
            let v: serde_json::Value =
                serde_json::from_slice(payload).map_err(|e| format!("Config parse error: {}", e))?;
            return Ok(GatewayMessage::Config(v));
        }
        if topic.ends_with("/command") {
            let v: serde_json::Value =
                serde_json::from_slice(payload).map_err(|e| format!("Command parse error: {}", e))?;
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

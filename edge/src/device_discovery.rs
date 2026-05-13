use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredDevice {
    pub name: String,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
}

pub struct DeviceScanner {
    configured_devices: Vec<DiscoveredDevice>,
}

impl DeviceScanner {
    pub fn new() -> Self {
        Self {
            configured_devices: Vec::new(),
        }
    }

    pub async fn scan(&self) -> Vec<DiscoveredDevice> {
        self.configured_devices.clone()
    }

    pub fn load_from_config(&mut self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(path)?;
        self.configured_devices = serde_json::from_str(&content)?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDiscoverMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub devices: Vec<DiscoveredDevice>,
}

impl DeviceDiscoverMessage {
    pub fn new(devices: Vec<DiscoveredDevice>) -> Self {
        Self {
            msg_type: "device_discover".into(),
            devices,
        }
    }
}

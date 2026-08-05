use serde::{Deserialize, Serialize};

/// Lightweight device info for listing (avoids sending full Device internals)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub device_type: Option<String>,
    pub status: String,
    pub driver_name: Option<String>,
}

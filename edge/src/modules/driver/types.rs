use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverInfo {
    pub name: String,
    pub version: String,
    pub status: String, // "loaded", "unhealthy", "not_loaded"
    pub device_count: u32,
}

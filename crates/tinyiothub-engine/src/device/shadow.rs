//! Device shadow — last-known state cache for devices.
//!
//! TODO: Migrate logic from `cloud/src/domain/device/`.

use std::collections::HashMap;

/// Shadow document for a single device.
#[derive(Debug, Clone, Default)]
pub struct DeviceShadow {
    pub device_id: String,
    pub reported: HashMap<String, serde_json::Value>,
    pub desired: HashMap<String, serde_json::Value>,
    pub last_updated: String,
}

impl DeviceShadow {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            ..Default::default()
        }
    }
}

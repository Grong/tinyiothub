//! Device registry — tracks online/offline devices and their metadata.
//!
//! TODO: Migrate logic from `cloud/src/domain/device/service.rs`.

/// A registry of all known devices.
#[derive(Debug, Default)]
pub struct DeviceRegistry {
    // TODO: populate from cloud domain
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

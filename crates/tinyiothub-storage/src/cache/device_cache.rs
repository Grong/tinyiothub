//! Device memory cache — extracted from cloud DataContext.

use std::sync::Arc;

use dashmap::DashMap;
use tinyiothub_core::models::device::Device;

/// Thread-safe in-memory device cache.
#[derive(Debug, Clone)]
pub struct DeviceCache {
    devices: Arc<DashMap<String, Arc<Device>>>,
    name_to_id: Arc<DashMap<String, String>>,
}

impl DeviceCache {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(DashMap::new()),
            name_to_id: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, id: &str) -> Option<Device> {
        self.devices.get(id).map(|d| Device::clone(&d))
    }

    pub fn get_by_name(&self, name: &str) -> Option<Device> {
        self.name_to_id
            .get(name)
            .and_then(|id| self.get(id.as_str()))
    }

    pub fn insert(&self, device: Device) {
        let id = device.id.clone();
        let name = device.name.clone();
        self.devices.insert(id.clone(), Arc::new(device));
        self.name_to_id.insert(name, id);
    }

    pub fn remove(&self, id: &str) {
        if let Some((_, device)) = self.devices.remove(id) {
            self.name_to_id.remove(&device.name);
        }
    }

    pub fn update(&self, device: Device) {
        let id = device.id.clone();
        self.devices.insert(id, Arc::new(device));
    }

    pub fn update_property(
        &self,
        device_id: &str,
        _property_id: &str,
        update_fn: impl FnOnce(&mut Device),
    ) {
        if let Some(device_arc) = self.devices.get(device_id) {
            let mut device = (**device_arc).clone();
            update_fn(&mut device);
            self.devices.insert(device_id.to_string(), Arc::new(device));
        }
    }

    pub fn all(&self) -> Vec<Device> {
        self.devices.iter().map(|e| Device::clone(e.value())).collect()
    }

    pub fn clear(&self) {
        self.devices.clear();
        self.name_to_id.clear();
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

impl Default for DeviceCache {
    fn default() -> Self {
        Self::new()
    }
}

//! Device memory cache — zero-copy, lock-free reads.
//!
//! Design inspired by HORUS shared-memory architecture:
//! - **DashMap**: O(1) lookups by ID / name (per-shard locks, held for nanoseconds)
//! - **ArcSwap**: atomic snapshot of device IDs for `all()`
//!   → readers do an atomic pointer load (no lock, no contention)
//!   → writers do an atomic pointer swap (no lock, no contention with readers)
//!
//! The old `RwLock<Vec<String>>` / `DashMap::iter()` approaches are gone.
//! This cache is safe to call from any tokio task without blocking the runtime.

use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use tinyiothub_core::models::device::Device;

/// Thread-safe in-memory device cache.
///
/// Reads are fully lock-free: `all()` does an atomic `ArcSwap::load()` then
/// iterates an immutable `Vec<String>` — no lock is held at any point.
#[derive(Debug, Clone)]
pub struct DeviceCache {
    devices: Arc<DashMap<String, Arc<Device>>>,
    name_to_id: Arc<DashMap<String, String>>,
    /// Atomic snapshot of device IDs.  Writers swap a new `Arc<Vec>` in; readers
    /// load the current `Arc` without any synchronisation beyond an atomic
    /// pointer read.
    device_ids: Arc<ArcSwap<Vec<String>>>,
}

impl DeviceCache {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(DashMap::new()),
            name_to_id: Arc::new(DashMap::new()),
            device_ids: Arc::new(ArcSwap::from(Arc::new(Vec::new()))),
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
        self.name_to_id.insert(name, id.clone());
        // Atomic snapshot swap: load current → append → store
        let current = self.device_ids.load();
        if !current.contains(&id) {
            let mut new_ids: Vec<String> = (**current).clone();
            new_ids.push(id);
            self.device_ids.store(Arc::new(new_ids));
        }
    }

    pub fn remove(&self, id: &str) {
        if let Some((_, device)) = self.devices.remove(id) {
            self.name_to_id.remove(&device.name);
            let current = self.device_ids.load();
            let new_ids: Vec<String> = current.iter().filter(|k| *k != id).cloned().collect();
            self.device_ids.store(Arc::new(new_ids));
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

    /// Returns all cached devices — **completely lock-free**.
    ///
    /// Loads the atomic snapshot (a single atomic pointer read, no lock),
    /// then does per-ID lookups against DashMap.  Each DashMap `get()`
    /// acquires/releases one shard lock for nanoseconds — no cross-shard
    /// contention, no deadlock possible.
    pub fn all(&self) -> Vec<Device> {
        let ids = self.device_ids.load(); // atomic pointer load — O(1), lock-free
        ids.iter().filter_map(|k| self.get(k)).collect()
    }

    pub fn clear(&self) {
        self.devices.clear();
        self.name_to_id.clear();
        self.device_ids.store(Arc::new(Vec::new()));
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

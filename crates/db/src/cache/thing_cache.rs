//! Thing memory cache — zero-copy, lock-free reads.
//!
//! Design inspired by HORUS shared-memory architecture:
//! - **DashMap**: O(1) lookups by ID / name (per-shard locks, held for nanoseconds)
//! - **ArcSwap**: atomic snapshot of device IDs for `all()` → readers do an atomic pointer load (no
//!   lock, no contention) → writers do an atomic pointer swap (no lock, no contention with readers)
//!
//! The old `RwLock<Vec<String>>` / `DashMap::iter()` approaches are gone.
//! This cache is safe to call from any tokio task without blocking the runtime.

use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use tinyiothub_core::models::thing::Thing;

/// Thread-safe in-memory device cache.
///
/// Reads are fully lock-free: `all()` does an atomic `ArcSwap::load()` then
/// iterates an immutable `Vec<String>` — no lock is held at any point.
#[derive(Debug, Clone)]
pub struct ThingCache {
    things: Arc<DashMap<String, Arc<Thing>>>,
    name_to_id: Arc<DashMap<String, String>>,
    /// Atomic snapshot of device IDs.  Writers swap a new `Arc<Vec>` in; readers
    /// load the current `Arc` without any synchronisation beyond an atomic
    /// pointer read.
    thing_ids: Arc<ArcSwap<Vec<String>>>,
}

impl ThingCache {
    pub fn new() -> Self {
        Self {
            things: Arc::new(DashMap::new()),
            name_to_id: Arc::new(DashMap::new()),
            thing_ids: Arc::new(ArcSwap::from(Arc::new(Vec::new()))),
        }
    }

    pub fn get(&self, id: &str) -> Option<Thing> {
        self.things.get(id).map(|d| Thing::clone(&d))
    }

    pub fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.name_to_id.get(name).and_then(|id| self.get(id.as_str()))
    }

    pub fn insert(&self, device: Thing) {
        let id = device.id.clone();
        let name = device.name.clone();
        self.things.insert(id.clone(), Arc::new(device));
        self.name_to_id.insert(name, id.clone());
        // Atomic snapshot swap: load current → append → store
        let current = self.thing_ids.load();
        if !current.contains(&id) {
            let mut new_ids: Vec<String> = (**current).clone();
            new_ids.push(id);
            self.thing_ids.store(Arc::new(new_ids));
        }
    }

    pub fn remove(&self, id: &str) {
        if let Some((_, device)) = self.things.remove(id) {
            self.name_to_id.remove(&device.name);
            let current = self.thing_ids.load();
            let new_ids: Vec<String> = current.iter().filter(|k| *k != id).cloned().collect();
            self.thing_ids.store(Arc::new(new_ids));
        }
    }

    pub fn update(&self, device: Thing) {
        let id = device.id.clone();
        self.things.insert(id, Arc::new(device));
    }

    pub fn update_property(&self, thing_id: &str, _property_id: &str, update_fn: impl FnOnce(&mut Thing)) {
        // Clone the device OUTSIDE the read-lock scope.
        // DashMap::get() holds a per-shard read lock; if we try to
        // DashMap::insert() (which needs a write lock) while that
        // read guard is still alive we deadlock — parking_lot does
        // NOT support lock upgrades.
        let device = self.things.get(thing_id).map(|arc| (**arc).clone());
        if let Some(mut device) = device {
            update_fn(&mut device);
            self.things.insert(thing_id.to_string(), Arc::new(device));
        }
    }

    /// Returns all cached things — **completely lock-free**.
    ///
    /// Loads the atomic snapshot (a single atomic pointer read, no lock),
    /// then does per-ID lookups against DashMap.  Each DashMap `get()`
    /// acquires/releases one shard lock for nanoseconds — no cross-shard
    /// contention, no deadlock possible.
    pub fn all(&self) -> Vec<Thing> {
        let ids = self.thing_ids.load(); // atomic pointer load — O(1), lock-free
        ids.iter().filter_map(|k| self.get(k)).collect()
    }

    pub fn clear(&self) {
        self.things.clear();
        self.name_to_id.clear();
        self.thing_ids.store(Arc::new(Vec::new()));
    }

    pub fn len(&self) -> usize {
        self.things.len()
    }

    pub fn is_empty(&self) -> bool {
        self.things.is_empty()
    }
}

impl Default for ThingCache {
    fn default() -> Self {
        Self::new()
    }
}

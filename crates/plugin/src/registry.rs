//! Plugin registry — tracks loaded plugins and provides lookup.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::ffi::PluginInfo;

/// Handle to a loaded plugin (opaque to callers).
#[derive(Debug, Clone)]
pub struct PluginHandle {
    pub name: String,
    pub info: PluginInfo,
}

/// Registry of all loaded plugins.
#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, Arc<PluginHandle>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, handle: PluginHandle) -> Result<(), String> {
        let mut plugins = self.plugins.write().unwrap();
        if plugins.contains_key(&handle.name) {
            return Err(format!("plugin '{}' already registered", handle.name));
        }
        plugins.insert(handle.name.clone(), Arc::new(handle));
        Ok(())
    }

    pub fn unregister(&self, name: &str) -> Option<Arc<PluginHandle>> {
        self.plugins.write().unwrap().remove(name)
    }

    pub fn get(&self, name: &str) -> Option<Arc<PluginHandle>> {
        self.plugins.read().unwrap().get(name).cloned()
    }

    pub fn list(&self) -> Vec<Arc<PluginHandle>> {
        self.plugins.read().unwrap().values().cloned().collect()
    }
}

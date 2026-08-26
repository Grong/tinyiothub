// crates/tinyiothub-runtime/src/driver/registry.rs

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use libloading::Library;
use parking_lot::RwLock;
use tinyiothub_core::driver::dynamic::{DriverVTable, SYMBOL_DESTROY, SYMBOL_INIT, SYMBOL_VTABLE};
use tinyiothub_core::error::Error;

use super::dynamic_adapter::DynamicEntry;

/// Registry of all drivers: built-in (static) + dynamic (per-workspace).
pub struct DriverRegistry {
    /// Per-workspace dynamic driver registries.
    /// Key = workspace_id.
    dynamic: RwLock<HashMap<String, WorkspaceRegistry>>,
}

/// Drivers loaded for a single workspace.
pub struct WorkspaceRegistry {
    pub workspace_id: String,
    pub drivers: HashMap<String, LoadedDriver>,
}

/// A loaded dynamic driver with reference-counted lifecycle.
pub struct LoadedDriver {
    pub entry: DynamicEntry,
    pub ref_count: AtomicUsize,
}

// SAFETY: The raw pointer in DynamicEntry is only accessed through the vtable
// while the library is kept alive via Arc. The registry ensures thread-safe access.
unsafe impl Send for LoadedDriver {}
unsafe impl Sync for LoadedDriver {}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            dynamic: RwLock::new(HashMap::new()),
        }
    }

    /// Load a dynamic library driver from disk and register it for a workspace.
    pub fn load(&self, path: &PathBuf, workspace_id: &str) -> Result<String, Error> {
        let lib =
            unsafe { Library::new(path).map_err(|e| Error::DriverError(format!("failed to load library: {}", e)))? };
        let lib = Arc::new(lib);

        let init: libloading::Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_void> = unsafe {
            lib.get(SYMBOL_INIT)
                .map_err(|e| Error::DriverError(format!("missing symbol tinyiothub_driver_init: {}", e)))?
        };

        let vtable_fn: libloading::Symbol<unsafe extern "C" fn() -> *const DriverVTable> = unsafe {
            lib.get(SYMBOL_VTABLE)
                .map_err(|e| Error::DriverError(format!("missing symbol tinyiothub_driver_vtable: {}", e)))?
        };

        let ctx = unsafe { init() };
        if ctx.is_null() {
            return Err(Error::DriverError("driver init returned null".into()));
        }

        let vtable_ptr: *const DriverVTable = unsafe { vtable_fn() };
        if vtable_ptr.is_null() {
            return Err(Error::DriverError("driver vtable is null".into()));
        }

        let vtable: &'static DriverVTable = unsafe { &*vtable_ptr };

        if vtable.version != tinyiothub_core::driver::dynamic::DRIVER_ABI_VERSION {
            return Err(Error::DriverError(format!(
                "ABI version mismatch: expected {}, got {}",
                tinyiothub_core::driver::dynamic::DRIVER_ABI_VERSION,
                vtable.version
            )));
        }

        let driver_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.strip_prefix("lib").unwrap_or(s))
            .unwrap_or("unknown")
            .to_string();

        let entry = DynamicEntry {
            name: driver_name.clone(),
            version: "0.0.0".to_string(),
            path: path.clone(),
            library: Arc::clone(&lib),
            vtable,
            ctx,
            loaded_at: Utc::now(),
        };

        let loaded = LoadedDriver {
            entry,
            ref_count: AtomicUsize::new(0),
        };

        let mut dynamic = self.dynamic.write();
        let ws = dynamic
            .entry(workspace_id.to_string())
            .or_insert_with(|| WorkspaceRegistry {
                workspace_id: workspace_id.to_string(),
                drivers: HashMap::new(),
            });

        if ws.drivers.contains_key(&driver_name) {
            return Err(Error::DriverError(format!(
                "driver '{}' already loaded in workspace '{}'",
                driver_name, workspace_id
            )));
        }

        ws.drivers.insert(driver_name.clone(), loaded);
        tracing::info!(
            "loaded dynamic driver '{}' for workspace '{}'",
            driver_name,
            workspace_id
        );

        Ok(driver_name)
    }

    /// Unload a driver from a workspace. Fails if ref_count > 0.
    pub fn unload(&self, driver_name: &str, workspace_id: &str) -> Result<(), Error> {
        let mut dynamic = self.dynamic.write();
        let ws = dynamic
            .get_mut(workspace_id)
            .ok_or_else(|| Error::DriverError(format!("workspace '{}' not found", workspace_id)))?;

        let loaded = ws
            .drivers
            .get(driver_name)
            .ok_or_else(|| Error::DriverError(format!("driver '{}' not found", driver_name)))?;

        let count = loaded.ref_count.load(Ordering::SeqCst);
        if count > 0 {
            return Err(Error::DriverError(format!(
                "driver '{}' is in use (ref_count={})",
                driver_name, count
            )));
        }

        let destroy: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)> = unsafe {
            loaded
                .entry
                .library
                .get(SYMBOL_DESTROY)
                .map_err(|e| Error::DriverError(format!("missing destroy symbol: {}", e)))?
        };
        unsafe { destroy(loaded.entry.ctx) };

        ws.drivers.remove(driver_name);
        if ws.drivers.is_empty() {
            dynamic.remove(workspace_id);
        }

        tracing::info!(
            "unloaded dynamic driver '{}' from workspace '{}'",
            driver_name,
            workspace_id
        );
        Ok(())
    }

    /// Look up a dynamic driver entry by (workspace_id, driver_name).
    pub fn find(&self, workspace_id: &str, driver_name: &str) -> Option<DynamicEntry> {
        let dynamic = self.dynamic.read();
        let ws = dynamic.get(workspace_id)?;
        let loaded = ws.drivers.get(driver_name)?;
        Some(DynamicEntry {
            name: loaded.entry.name.clone(),
            version: loaded.entry.version.clone(),
            path: loaded.entry.path.clone(),
            library: Arc::clone(&loaded.entry.library),
            vtable: loaded.entry.vtable,
            ctx: loaded.entry.ctx,
            loaded_at: loaded.entry.loaded_at,
        })
    }

    /// Increment the ref_count for a driver.
    pub fn acquire(&self, workspace_id: &str, driver_name: &str) -> Result<(), Error> {
        let dynamic = self.dynamic.read();
        let ws = dynamic
            .get(workspace_id)
            .ok_or_else(|| Error::DriverError(format!("workspace '{}' not found", workspace_id)))?;
        let loaded = ws
            .drivers
            .get(driver_name)
            .ok_or_else(|| Error::DriverError(format!("driver '{}' not found", driver_name)))?;
        loaded.ref_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Decrement the ref_count for a driver.
    pub fn release(&self, workspace_id: &str, driver_name: &str) -> Result<(), Error> {
        let dynamic = self.dynamic.read();
        let ws = dynamic
            .get(workspace_id)
            .ok_or_else(|| Error::DriverError(format!("workspace '{}' not found", workspace_id)))?;
        let loaded = ws
            .drivers
            .get(driver_name)
            .ok_or_else(|| Error::DriverError(format!("driver '{}' not found", driver_name)))?;
        loaded.ref_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    /// List all loaded dynamic drivers for a workspace.
    /// Returns Vec of (name, version, loaded_at, ref_count).
    pub fn list_for_workspace(&self, workspace_id: &str) -> Vec<(String, String, DateTime<Utc>, usize)> {
        let dynamic = self.dynamic.read();
        let ws = match dynamic.get(workspace_id) {
            Some(ws) => ws,
            None => return Vec::new(),
        };
        ws.drivers
            .iter()
            .map(|(name, loaded)| {
                let ref_count = loaded.ref_count.load(std::sync::atomic::Ordering::SeqCst);
                (
                    name.clone(),
                    loaded.entry.version.clone(),
                    loaded.entry.loaded_at,
                    ref_count,
                )
            })
            .collect()
    }

    /// List all workspace IDs that have loaded drivers.
    pub fn list_workspaces(&self) -> Vec<String> {
        let dynamic = self.dynamic.read();
        dynamic.keys().cloned().collect()
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Design note: All load-time error paths are handled with descriptive messages
// in DriverRegistry::load (ABI mismatch, null vtable/null init, missing symbols,
// duplicate driver, ref_count blocking unload). Unit tests below cover registry
// lookup/not-found paths. Full E2E coverage of load-time paths requires
// integration tests with real .so files (compiled test driver plugin).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_nonexistent_driver() {
        let registry = DriverRegistry::new();
        assert!(registry.find("ws1", "modbus").is_none());
    }

    #[test]
    fn test_acquire_nonexistent_workspace() {
        let registry = DriverRegistry::new();
        let err = registry.acquire("ws1", "modbus").unwrap_err();
        assert!(err.to_string().contains("workspace 'ws1' not found"));
    }

    #[test]
    fn test_release_nonexistent_driver() {
        let registry = DriverRegistry::new();
        // First create workspace by loading, but since we can't load without .so,
        // we verify the error path via a helper. Instead, we test empty registry.
        let err = registry.release("ws1", "modbus").unwrap_err();
        assert!(err.to_string().contains("workspace 'ws1' not found"));
    }

    #[test]
    fn test_unload_nonexistent_workspace() {
        let registry = DriverRegistry::new();
        let err = registry.unload("modbus", "ws1").unwrap_err();
        assert!(err.to_string().contains("workspace 'ws1' not found"));
    }

    #[test]
    fn test_unload_nonexistent_driver() {
        let registry = DriverRegistry::new();
        // Manually insert a workspace registry without drivers
        {
            let mut dynamic = registry.dynamic.write();
            dynamic.insert(
                "ws1".to_string(),
                WorkspaceRegistry {
                    workspace_id: "ws1".to_string(),
                    drivers: HashMap::new(),
                },
            );
        }
        let err = registry.unload("modbus", "ws1").unwrap_err();
        assert!(err.to_string().contains("driver 'modbus' not found"));
    }

    #[test]
    fn test_list_for_workspace_empty() {
        let registry = DriverRegistry::new();
        assert!(registry.list_for_workspace("ws1").is_empty());
    }

    #[test]
    fn test_list_workspaces_empty() {
        let registry = DriverRegistry::new();
        assert!(registry.list_workspaces().is_empty());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let registry = DriverRegistry::new();
        let err = registry
            .load(&PathBuf::from("/nonexistent/driver.so"), "ws1")
            .unwrap_err();
        assert!(err.to_string().contains("failed to load library"));
    }
}

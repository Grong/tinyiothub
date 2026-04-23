//! Dynamic library loader for plugins.

use std::path::Path;

use libloading::{Library, Symbol};

use crate::ffi::{PluginFfi, PluginInfo};

/// Errors that can occur during plugin loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginLoadError {
    LibraryNotFound(String),
    SymbolNotFound(String),
    IncompatibleVersion { expected: u32, found: u32 },
    InitFailed(i32),
}

impl std::fmt::Display for PluginLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginLoadError::LibraryNotFound(p) => write!(f, "library not found: {}", p),
            PluginLoadError::SymbolNotFound(s) => write!(f, "symbol not found: {}", s),
            PluginLoadError::IncompatibleVersion { expected, found } => {
                write!(f, "incompatible API version: expected {}, found {}", expected, found)
            }
            PluginLoadError::InitFailed(code) => write!(f, "plugin init failed with code {}", code),
        }
    }
}

impl std::error::Error for PluginLoadError {}

/// Loads plugins from shared libraries.
pub struct PluginLoader {
    api_version: u32,
}

impl PluginLoader {
    pub fn new(api_version: u32) -> Self {
        Self { api_version }
    }

    /// Load a plugin from a `.so`, `.dylib`, or `.dll` path.
    ///
    /// # Safety
    ///
    /// The plugin must export valid C ABI symbols. The caller must ensure
    /// the library remains loaded for as long as the returned function
    /// pointers are used.
    pub unsafe fn load(&self, path: &Path) -> Result<PluginFfi, PluginLoadError> {
        let lib = unsafe { Library::new(path) }.map_err(|e| {
            PluginLoadError::LibraryNotFound(format!("{}: {}", path.display(), e))
        })?;

        let info: PluginInfo = {
            let info_fn: Symbol<unsafe extern "C" fn() -> *const PluginInfo> = unsafe {
                lib.get(b"plugin_info\0")
            }
            .map_err(|_| PluginLoadError::SymbolNotFound("plugin_info".into()))?;
            unsafe { std::ptr::read((*info_fn)()) }
        };

        if info.api_version != self.api_version {
            return Err(PluginLoadError::IncompatibleVersion {
                expected: self.api_version,
                found: info.api_version,
            });
        }

        let init_ptr: unsafe extern "C" fn() -> i32 = {
            let init_fn: Symbol<unsafe extern "C" fn() -> i32> = unsafe {
                lib.get(b"plugin_init\0")
            }
            .map_err(|_| PluginLoadError::SymbolNotFound("plugin_init".into()))?;
            *init_fn
        };

        let shutdown_ptr: unsafe extern "C" fn() -> i32 = {
            let shutdown_fn: Symbol<unsafe extern "C" fn() -> i32> = unsafe {
                lib.get(b"plugin_shutdown\0")
            }
            .map_err(|_| PluginLoadError::SymbolNotFound("plugin_shutdown".into()))?;
            *shutdown_fn
        };

        // Leak the library so it stays loaded.
        let _ = Box::leak(Box::new(lib));

        Ok(PluginFfi {
            info,
            init: init_ptr,
            shutdown: shutdown_ptr,
        })
    }
}

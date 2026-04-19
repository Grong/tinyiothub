//! C ABI interface for plugins.
//!
//! Plugins export these symbols via `#[no_mangle]`:
//! - `plugin_info()` -> `*const PluginInfo`
//! - `plugin_init()` -> `i32`
//! - `plugin_shutdown()` -> `i32`

/// Plugin metadata exposed to the host.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: [u8; 64],
    pub version: PluginVersion,
    pub api_version: u32,
}

/// Semantic version for plugins.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl PluginVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

/// FFI vtable for a loaded plugin.
pub struct PluginFfi {
    pub info: PluginInfo,
    pub init: unsafe extern "C" fn() -> i32,
    pub shutdown: unsafe extern "C" fn() -> i32,
}

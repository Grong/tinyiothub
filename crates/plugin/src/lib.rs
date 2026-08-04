//! TinyIoTHub plugin system core.
//!
//! Provides FFI definitions, dynamic loading, registry, and sandbox
//! for third-party device drivers and extensions.
//!
//! Plugins are compiled as `cdylib` and loaded at runtime via `libloading`.

pub mod ffi;
pub mod loader;
pub mod registry;
pub mod sandbox;

pub use ffi::{PluginFfi, PluginInfo, PluginVersion};
pub use loader::{PluginLoadError, PluginLoader};
pub use registry::{PluginHandle, PluginRegistry};
pub use sandbox::{SandboxConfig, SandboxLimits};

pub use drivers::{ModbusDriver, SimulatedDriver, snmp_driver::SnmpDriver};
pub use status::DeviceOverview;
pub use tinyiothub_core::driver::{DeviceDriver, DriverConfig, ResultValue};
pub use tinyiothub_plugin_sdk::{ComponentInfo, ComponentOption, CreateComponentRequest};
pub use wrapper::DriverWrapper;

use std::sync::OnceLock;

use parking_lot::RwLock;
use tinyiothub_core::error::Error;
use tinyiothub_core::models::device::Device;

pub mod drivers;
pub mod dynamic_adapter;
pub mod loader;
pub mod registry;
pub mod retry;
pub mod status;
pub mod validation;
pub mod validator;
pub mod wrapper;

// Register all drivers via macro
tinyiothub_macros::register_drivers! {
    SimulatedDriver,
    ModbusDriver,
    SnmpDriver,
}

/// Global driver registry. Initialized lazily on first access.
static GLOBAL_REGISTRY: OnceLock<RwLock<registry::DriverRegistry>> = OnceLock::new();

fn global_registry() -> &'static RwLock<registry::DriverRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| RwLock::new(registry::DriverRegistry::new()))
}

/// Create a driver instance by name.
/// Checks built-in drivers first, then the workspace-specific dynamic registry.
pub fn create_driver(driver_name: &str, device: &Device) -> Result<DriverWrapper, Error> {
    // 1. Built-in drivers (global, always available)
    if is_driver_supported(driver_name) {
        let base_driver = create_driver_by_name(driver_name, device)?;
        return Ok(DriverWrapper::new(base_driver));
    }

    // 2. Dynamic drivers (per-workspace)
    if let Some(ref workspace_id) = device.workspace_id {
        let reg = global_registry().read();
        if let Some(entry) = reg.find(workspace_id, driver_name) {
            let driver = dynamic_adapter::DynamicDeviceDriver::new(&entry, device.clone())?;
            reg.acquire(workspace_id, driver_name)?;
            return Ok(DriverWrapper::new(Box::new(driver)));
        }
    }

    Err(Error::Unsupported(format!("Unknown driver: {}", driver_name)))
}

/// Get all driver names (builtin only; dynamic names require workspace context)
pub fn get_all_driver_names() -> Vec<String> {
    get_supported_driver_names()
}

/// Check if a driver exists (builtin or in the global registry for any workspace)
pub fn has_driver(name: &str) -> bool {
    if is_driver_supported(name) {
        return true;
    }
    let reg = global_registry().read();
    for ws_id in reg.list_workspaces() {
        if reg.find(&ws_id, name).is_some() {
            return true;
        }
    }
    false
}

/// Access the global driver registry.
pub fn driver_registry() -> &'static RwLock<registry::DriverRegistry> {
    global_registry()
}

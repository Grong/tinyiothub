pub use wrapper::DriverWrapper;
pub use drivers::{snmp_driver::SnmpDriver, ModbusDriver, SimulatedDriver};
pub use status::DeviceOverview;
// Re-export core driver types for backward compatibility
pub use tinyiothub_core::driver::{DeviceDriver, ResultValue, DriverConfig};
// Re-export SDK types for backward compatibility
pub use tinyiothub_plugin_sdk::{ComponentInfo, ComponentOption, CreateComponentRequest};

use tinyiothub_core::models::device::Device;
use tinyiothub_core::error::Error;

pub mod drivers;
pub mod retry;
pub mod status;
pub mod wrapper;

// Register all drivers via macro
tinyiothub_macros::register_drivers! {
    SimulatedDriver,
    ModbusDriver,
    SnmpDriver,
}

/// Create a driver instance by name
pub fn create_driver(
    driver_name: &str,
    device: &Device,
) -> Result<DriverWrapper, Error> {
    if is_driver_supported(driver_name) {
        let base_driver = create_driver_by_name(driver_name, device)?;
        return Ok(DriverWrapper::new(base_driver));
    }

    Err(Error::Unsupported(format!("Unknown driver: {}", driver_name)))
}

/// Get all driver names
pub fn get_all_driver_names() -> Vec<String> {
    get_supported_driver_names()
}

/// Check if a driver exists
pub fn has_driver(name: &str) -> bool {
    is_driver_supported(name)
}

pub use driver::{DeviceDriver, DriverWrapper, ResultValue};
pub use drivers::{snmp_driver::SnmpDriver, ModbusDriver, SimulatedDriver};
pub use status::DeviceOverview;
// 重新导出SDK类型以保持向后兼容
pub use tinyiothub_plugin_sdk::{ComponentInfo, ComponentOption, CreateComponentRequest};

use tinyiothub_core::models::device::Device;
use tinyiothub_core::error::Error;

pub mod driver;
pub mod drivers;
pub mod retry;
pub mod status;

// 使用宏注册所有驱动
tinyiothub_macros::register_drivers! {
    SimulatedDriver,
    ModbusDriver,
    SnmpDriver,
}

/// 创建驱动实例
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

/// 获取所有驱动名称
pub fn get_all_driver_names() -> Vec<String> {
    get_supported_driver_names()
}

/// 检查驱动是否存在
pub fn has_driver(name: &str) -> bool {
    is_driver_supported(name)
}

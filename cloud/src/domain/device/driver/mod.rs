use std::sync::Arc;

pub use driver::{DeviceDriver, DriverWrapper, ResultValue};
pub use drivers::{snmp_driver::SnmpDriver, ModbusDriver, SimulatedDriver};
pub use status::DeviceOverview;
// 重新导出SDK类型以保持向后兼容
pub use tinyiothub_plugin_sdk::{ComponentInfo, ComponentOption, CreateComponentRequest};

use tinyiothub_core::models::Device, shared::error::Error;


pub mod driver;
pub mod drivers;
pub mod dynamic;
pub mod retry;
pub mod status;

// 使用宏注册所有驱动
tinyiothub_macros::register_drivers! {
    SimulatedDriver,
    ModbusDriver,
    SnmpDriver,
}

/// 创建驱动实例（统一入口，支持静态和动态驱动）
pub fn create_driver(
    driver_name: &str,
    device: &Device,
    context: Arc<DataContext>,
) -> Result<DriverWrapper, Error> {
    // 优先使用静态驱动
    if is_driver_supported(driver_name) {
        let base_driver = create_driver_by_name(driver_name, device, context)?;
        return Ok(DriverWrapper::new(base_driver));
    }

    // 尝试使用动态驱动
    let registry = dynamic::registry::get_global_registry();
    if registry.has_driver(driver_name) {
        let base_driver = registry.create_driver(driver_name, device, context)?;
        return Ok(DriverWrapper::new(base_driver));
    }

    Err(Error::Unsupported(format!("Unknown driver: {}", driver_name)))
}

/// 加载动态驱动
pub fn load_dynamic_driver<P: AsRef<std::path::Path>>(path: P) -> Result<String, Error> {
    let registry = dynamic::registry::get_global_registry();
    registry.load_dynamic(path.as_ref().to_path_buf())
}

/// 卸载动态驱动
pub fn unload_dynamic_driver(name: &str) -> Result<(), Error> {
    let registry = dynamic::registry::get_global_registry();
    registry.unload_dynamic(name)
}

/// 获取所有驱动名称（包括静态和动态）
pub fn get_all_driver_names() -> Vec<String> {
    let mut names = get_supported_driver_names();
    let registry = dynamic::registry::get_global_registry();
    names.extend(registry.get_driver_names());
    names.sort();
    names.dedup();
    names
}

/// 检查驱动是否存在（包括静态和动态）
pub fn has_driver(name: &str) -> bool {
    is_driver_supported(name) || dynamic::registry::get_global_registry().has_driver(name)
}

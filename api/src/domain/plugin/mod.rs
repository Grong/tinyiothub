pub mod registry;
pub mod integration;
pub mod protocol;
pub mod notification;
pub mod scheduler;
pub mod storage;
pub mod driver_plugin;

#[cfg(test)] mod tests;

pub use registry::{get_global_registry, PluginRegistry, PluginEntry, PluginManifest, PluginType, PluginHandler, PluginFactory};
pub use crate::application::AppContext;

use std::sync::Arc;
use crate::shared::error::Error;

/// 初始化插件系统
///
/// 在应用启动时调用，负责：
/// 1. 注册内置驱动（SimulatedDriver, ModbusDriver, SnmpDriver）到插件注册表
/// 2. 从 api/plugins/ 目录加载 TOML 格式的插件配置
pub fn init_plugins(context: Arc<AppContext>) -> Result<(), Error> {
    driver_plugin::init_plugins(context)
}

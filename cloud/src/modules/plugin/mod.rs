pub mod driver_plugin;
pub mod integration;
pub mod notification;
pub mod protocol;
pub mod registry;
pub mod scheduler;
pub mod storage;

#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use registry::{
    PluginEntry, PluginFactory, PluginHandler, PluginManifest, PluginRegistry, PluginType,
    get_global_registry,
};

pub use crate::modules::agent::AppContext;
use crate::shared::error::Error;

/// 初始化插件系统
///
/// 在应用启动时调用，负责：
/// 1. 注册内置驱动（SimulatedDriver, ModbusDriver, SnmpDriver）到插件注册表
/// 2. 从 api/plugins/ 目录加载 TOML 格式的插件配置
pub fn init_plugins(context: Arc<AppContext>) -> Result<(), Error> {
    driver_plugin::init_plugins(context)
}

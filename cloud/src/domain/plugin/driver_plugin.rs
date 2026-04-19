//! 驱动插件处理器
//!
//! 将现有驱动系统（DriverWrapper）适配为插件系统（PluginHandler）

use tinyiothub_core::models::device::Device;
use std::{any::Any, sync::Arc};

use crate::{
    domain::device::driver::{create_driver, DriverWrapper},
    domain::plugin::{AppContext, PluginHandler, PluginManifest, PluginType},
    shared::error::Error,
};

/// 驱动插件处理器
///
/// 将 DriverWrapper 适配为 PluginHandler，使其可以纳入统一的插件管理系统
pub struct DriverPluginHandler {
    driver: DriverWrapper,
    manifest: PluginManifest,
}

impl DriverPluginHandler {
    /// 从设备创建设动插件处理器
    pub fn new(
        driver_name: String,
        version: String,
        device: Device,
        context: Arc<AppContext>,
    ) -> Result<Self, Error> {
        let manifest = PluginManifest {
            name: driver_name.clone(),
            version: Some(version),
            plugin_type: PluginType::Protocol,
            description: None,
        };

        let driver =
            create_driver(&driver_name, &device, context.data_context.clone())?;

        Ok(Self { driver, manifest })
    }

    /// 获取内部驱动引用
    pub fn driver(&self) -> &DriverWrapper {
        &self.driver
    }

    /// 获取内部驱动可变引用
    pub fn driver_mut(&mut self) -> &mut DriverWrapper {
        &mut self.driver
    }
}

impl PluginHandler for DriverPluginHandler {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn plugin_type(&self) -> PluginType {
        PluginType::Protocol
    }
}


/// 注册内置驱动到插件注册表
pub fn register_builtin_drivers(registry: &crate::domain::plugin::PluginRegistry) {
    use crate::domain::device::driver::{get_driver_list, get_supported_driver_names};

    // 获取所有内置驱动的信息
    let driver_infos = get_driver_list();
    let driver_names = get_supported_driver_names();

    for (info, name) in driver_infos.into_iter().zip(driver_names.into_iter()) {
        // Clone description for use in both manifest and closure
        let desc_clone = info.description.clone();
        let manifest = PluginManifest {
            name: info.name.clone(),
            version: Some(info.version),
            plugin_type: PluginType::Protocol,
            description: desc_clone.clone(),
        };

        // 创建立厂函数闭包
        let driver_name = info.name.clone();
        let factory = Box::new(move |app_context: Arc<AppContext>| {
            // 创建设备的最小实例用于获取配置
            // 注意：实际设备会通过 MQTT 或其他方式传入，这里只注册类型
            let device = Device {
                id: format!("plugin_{}", driver_name),
                name: driver_name.clone(),
                display_name: desc_clone.clone(),
                ..Default::default()
            };

            let driver = create_driver(&driver_name, &device, app_context.data_context.clone())
                .map_err(|e| crate::shared::error::Error::Internal(e.to_string()))?;

            Ok(Box::new(DriverPluginHandler {
                driver,
                manifest: PluginManifest {
                    name: driver_name.clone(),
                    version: Some("1.0.0".to_string()),
                    plugin_type: PluginType::Protocol,
                    description: None,
                },
            }) as Box<dyn PluginHandler>)
        }) as crate::domain::plugin::PluginFactory;

        registry.register_static(manifest, factory);
        tracing::info!("Registered builtin driver as plugin: {}", name);
    }
}

/// 初始化插件系统
///
/// 此函数在应用启动时调用，负责：
/// 1. 注册内置驱动（SimulatedDriver, ModbusDriver, SnmpDriver）到插件注册表
/// 2. 从 api/plugins/ 目录加载 TOML 格式的插件配置
pub fn init_plugins(_context: Arc<AppContext>) -> Result<(), Error> {
    let registry = crate::domain::plugin::get_global_registry();

    // 1. 注册内置驱动为插件
    register_builtin_drivers(registry);

    // 2. 从 plugins 目录加载 TOML 插件配置
    let plugins_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("plugins");

    if let Err(e) = registry.load_from_dir(&plugins_dir) {
        tracing::warn!("Failed to load some plugins from {:?}: {}", plugins_dir, e);
    }

    tracing::info!("Plugin system initialized with {} plugins", registry.plugin_names().len());
    Ok(())
}

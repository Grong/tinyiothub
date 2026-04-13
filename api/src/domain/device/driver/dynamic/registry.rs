//! 统一驱动注册表
#![allow(deprecated)]

use std::{path::PathBuf, sync::Arc};

use dashmap::DashMap;
use tracing::{debug, info, warn};

use super::loader::DynamicDriverLoader;
use crate::{
    application::data_context::DataContext,
    domain::device::driver::DeviceDriver,
    dto::entity::{component::Component, Device},
    shared::error::Error,
};

/// 驱动工厂函数类型
type DriverFactory = Box<dyn Fn(Device, Arc<DataContext>) -> Box<dyn DeviceDriver> + Send + Sync>;

/// 统一驱动注册表（支持静态和动态驱动）
#[deprecated(since = "0.2.0", note = "Use PluginRegistry from domain::plugin instead")]
pub struct UnifiedDriverRegistry {
    /// 静态驱动工厂（编译时注册）
    static_factories: DashMap<String, DriverFactory>,
    /// 动态驱动加载器（运行时加载）
    dynamic_loaders: DashMap<String, Arc<DynamicDriverLoader>>,
}

impl UnifiedDriverRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self { static_factories: DashMap::new(), dynamic_loaders: DashMap::new() }
    }

    /// 注册静态驱动
    pub fn register_static<F>(&self, name: String, factory: F)
    where
        F: Fn(Device, Arc<DataContext>) -> Box<dyn DeviceDriver> + Send + Sync + 'static,
    {
        debug!("Registering static driver: {}", name);
        self.static_factories.insert(name, Box::new(factory));
    }

    /// 加载动态驱动
    pub fn load_dynamic(&self, path: PathBuf) -> Result<String, Error> {
        let loader = DynamicDriverLoader::load(&path)?;
        let driver_name = loader.driver_name().to_string();

        info!("Loaded dynamic driver: {} from {:?}", driver_name, path);
        self.dynamic_loaders.insert(driver_name.clone(), Arc::new(loader));

        Ok(driver_name)
    }

    /// 卸载动态驱动
    pub fn unload_dynamic(&self, name: &str) -> Result<(), Error> {
        if self.dynamic_loaders.remove(name).is_some() {
            info!("Unloaded dynamic driver: {}", name);
            Ok(())
        } else {
            Err(Error::Unsupported(format!("Driver not found: {}", name)))
        }
    }

    /// 创建驱动实例
    pub fn create_driver(
        &self,
        driver_name: &str,
        device: &Device,
        context: Arc<DataContext>,
    ) -> Result<Box<dyn DeviceDriver>, Error> {
        // 优先使用静态驱动
        if let Some(factory) = self.static_factories.get(driver_name) {
            debug!("Creating static driver: {}", driver_name);
            return Ok(factory(device.clone(), context));
        }

        // 尝试使用动态驱动
        if let Some(loader) = self.dynamic_loaders.get(driver_name) {
            debug!("Creating dynamic driver: {}", driver_name);
            let wrapper =
                super::wrapper::DynamicDriverWrapper::new(Arc::clone(&loader), device.clone())?;
            return Ok(Box::new(wrapper));
        }

        // 驱动不存在
        warn!("Driver not found: {}", driver_name);
        Err(Error::Unsupported(format!("Unknown driver: {}", driver_name)))
    }

    /// 检查驱动是否存在
    pub fn has_driver(&self, name: &str) -> bool {
        self.static_factories.contains_key(name) || self.dynamic_loaders.contains_key(name)
    }

    /// 获取所有驱动名称
    pub fn get_driver_names(&self) -> Vec<String> {
        let mut names = Vec::new();

        // 添加静态驱动
        for entry in self.static_factories.iter() {
            names.push(entry.key().clone());
        }

        // 添加动态驱动
        for entry in self.dynamic_loaders.iter() {
            names.push(entry.key().clone());
        }

        names.sort();
        names
    }

    /// 获取动态驱动信息
    pub fn get_dynamic_driver_info(&self, name: &str) -> Result<Component, Error> {
        let loader = self
            .dynamic_loaders
            .get(name)
            .ok_or_else(|| Error::Unsupported(format!("Dynamic driver not found: {}", name)))?;

        let info_json = loader.get_driver_info_json()?;
        let info: Component = serde_json::from_str(&info_json)
            .map_err(|e| Error::Unsupported(format!("Invalid driver info: {}", e)))?;

        Ok(info)
    }

    /// 获取驱动路径（仅动态驱动）
    pub fn get_driver_path(&self, name: &str) -> Option<String> {
        self.dynamic_loaders.get(name).map(|loader| loader.path().to_string_lossy().to_string())
    }
}

impl Default for UnifiedDriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局驱动注册表实例
static GLOBAL_REGISTRY: once_cell::sync::Lazy<UnifiedDriverRegistry> =
    once_cell::sync::Lazy::new(|| {
        let registry = UnifiedDriverRegistry::new();

        // 注册所有静态驱动
        // TODO: 从 register_drivers! 宏生成的代码中获取驱动列表

        registry
    });

/// 获取全局驱动注册表
pub fn get_global_registry() -> &'static UnifiedDriverRegistry {
    &GLOBAL_REGISTRY
}

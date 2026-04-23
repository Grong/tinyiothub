//! 动态驱动包装器

use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};
use std::sync::Arc;

use tracing::debug;

use super::loader::DynamicDriverLoader;
use crate::driver::{DeviceDriver, ResultValue};
use tinyiothub_core::error::Error;

/// 动态驱动包装器
pub struct DynamicDriverWrapper {
    loader: Arc<DynamicDriverLoader>,
    driver_ptr: *mut std::ffi::c_void,
    device: Device,
}

impl DynamicDriverWrapper {
    /// 创建动态驱动包装器
    pub fn new(loader: Arc<DynamicDriverLoader>, device: Device) -> Result<Self, Error> {
        let device_json = serde_json::to_string(&device)
            .map_err(|e| Error::Unsupported(format!("Failed to serialize device: {}", e)))?;

        let driver_ptr = loader.create_driver(&device_json)?;

        Ok(Self { loader, driver_ptr, device })
    }
}

impl DeviceDriver for DynamicDriverWrapper {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // TODO: 实现通过FFI调用驱动的read_data方法
        debug!("Reading data from dynamic driver: {}", self.loader.driver_name());
        Ok(vec![])
    }

    fn execute_command(&mut self, _command: &DeviceCommand) -> Result<bool, Error> {
        // TODO: 实现通过FFI调用驱动的execute_command方法
        debug!("Executing command on dynamic driver: {}", self.loader.driver_name());
        Ok(true)
    }
}

impl Drop for DynamicDriverWrapper {
    fn drop(&mut self) {
        debug!("Destroying dynamic driver instance: {}", self.loader.driver_name());
        self.loader.destroy_driver(self.driver_ptr);
    }
}

// 动态驱动可以跨线程发送和共享（由加载器保证线程安全）
unsafe impl Send for DynamicDriverWrapper {}
unsafe impl Sync for DynamicDriverWrapper {}

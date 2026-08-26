//! 设备驱动trait定义

use crate::{Device, DeviceCommand, Result, ResultValue};
use std::collections::HashMap;

/// 设备驱动trait（核心接口）
///
/// 所有驱动必须实现此trait
pub trait DeviceDriver: Send + Sync {
    /// 获取设备引用
    fn device(&self) -> &Device;

    /// 获取设备可变引用
    fn device_mut(&mut self) -> &mut Device;

    /// 读取设备数据
    ///
    /// # 返回
    ///
    /// 返回设备的当前数据点列表
    fn read_data(&mut self) -> Result<Vec<ResultValue>>;

    /// 执行设备命令
    ///
    /// # 参数
    ///
    /// * `cmd` - 要执行的命令
    ///
    /// # 返回
    ///
    /// 返回命令是否执行成功
    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool>;

    /// 获取驱动默认配置（可选实现）
    ///
    /// 使用 `#[derive(DeviceDriver)]` 宏的驱动会自动实现此方法
    fn default_config(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

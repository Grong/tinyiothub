//! 鸿蒙系统GPIO控制实现
//!
//! 提供与Linux版本兼容的GPIO接口，但使用鸿蒙系统的底层API

use std::{collections::HashMap, sync::Mutex};

use tracing::debug;

/// GPIO引脚状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpioValue {
    Low = 0,
    High = 1,
}

/// GPIO引脚方向
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpioDirection {
    Input,
    Output,
}

/// GPIO引脚配置
#[derive(Debug, Clone)]
pub struct GpioPin {
    pub chip: u32,
    pub pin: u32,
    pub direction: GpioDirection,
    pub value: GpioValue,
}

/// 鸿蒙系统GPIO管理器
pub struct HarmonyGpioManager {
    pins: Mutex<HashMap<(u32, u32), GpioPin>>,
}

impl HarmonyGpioManager {
    /// 创建新的GPIO管理器
    pub fn new() -> Self {
        Self { pins: Mutex::new(HashMap::new()) }
    }

    /// 导出GPIO引脚
    pub fn export_pin(&self, chip: u32, pin: u32) -> Result<(), std::io::Error> {
        debug!("Exporting GPIO pin {}/{} on HarmonyOS", chip, pin);

        // TODO: 实现鸿蒙系统的GPIO导出逻辑
        // 这里需要调用鸿蒙系统的GPIO API

        let mut pins = self.pins.lock().unwrap();
        pins.insert(
            (chip, pin),
            GpioPin { chip, pin, direction: GpioDirection::Input, value: GpioValue::Low },
        );

        Ok(())
    }

    /// 设置GPIO引脚方向
    pub fn set_direction(
        &self,
        chip: u32,
        pin: u32,
        direction: GpioDirection,
    ) -> Result<(), std::io::Error> {
        debug!("Setting GPIO pin {}/{} direction to {:?} on HarmonyOS", chip, pin, direction);

        // TODO: 实现鸿蒙系统的GPIO方向设置

        let mut pins = self.pins.lock().unwrap();
        if let Some(gpio_pin) = pins.get_mut(&(chip, pin)) {
            gpio_pin.direction = direction;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("GPIO pin {}/{} not found", chip, pin),
            ))
        }
    }

    /// 设置GPIO引脚值
    pub fn set_value(&self, chip: u32, pin: u32, value: GpioValue) -> Result<(), std::io::Error> {
        debug!("Setting GPIO pin {}/{} value to {:?} on HarmonyOS", chip, pin, value);

        // TODO: 实现鸿蒙系统的GPIO值设置

        let mut pins = self.pins.lock().unwrap();
        if let Some(gpio_pin) = pins.get_mut(&(chip, pin)) {
            if gpio_pin.direction == GpioDirection::Output {
                gpio_pin.value = value;
                Ok(())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("GPIO pin {}/{} is not configured as output", chip, pin),
                ))
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("GPIO pin {}/{} not found", chip, pin),
            ))
        }
    }

    /// 读取GPIO引脚值
    pub fn get_value(&self, chip: u32, pin: u32) -> Result<GpioValue, std::io::Error> {
        debug!("Reading GPIO pin {}/{} value on HarmonyOS", chip, pin);

        // TODO: 实现鸿蒙系统的GPIO值读取

        let pins = self.pins.lock().unwrap();
        if let Some(gpio_pin) = pins.get(&(chip, pin)) {
            Ok(gpio_pin.value)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("GPIO pin {}/{} not found", chip, pin),
            ))
        }
    }

    /// 取消导出GPIO引脚
    pub fn unexport_pin(&self, chip: u32, pin: u32) -> Result<(), std::io::Error> {
        debug!("Unexporting GPIO pin {}/{} on HarmonyOS", chip, pin);

        // TODO: 实现鸿蒙系统的GPIO取消导出逻辑

        let mut pins = self.pins.lock().unwrap();
        pins.remove(&(chip, pin));

        Ok(())
    }
}

impl Default for HarmonyGpioManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局GPIO管理器实例
static GPIO_MANAGER: once_cell::sync::Lazy<HarmonyGpioManager> =
    once_cell::sync::Lazy::new(HarmonyGpioManager::new);

/// 获取全局GPIO管理器
pub fn get_gpio_manager() -> &'static HarmonyGpioManager {
    &GPIO_MANAGER
}

/// 兼容性函数：设置GPIO值（与Linux版本兼容）
pub fn set_gpio_value(chip: u32, pin: u32, value: u32) -> Result<(), std::io::Error> {
    let gpio_value = if value == 0 { GpioValue::Low } else { GpioValue::High };
    get_gpio_manager().set_value(chip, pin, gpio_value)
}

/// 兼容性函数：获取GPIO值（与Linux版本兼容）
pub fn get_gpio_value(chip: u32, pin: u32) -> Result<u32, std::io::Error> {
    let value = get_gpio_manager().get_value(chip, pin)?;
    Ok(value as u32)
}

/// 兼容性函数：初始化GPIO引脚
pub fn init_gpio_pin(chip: u32, pin: u32, direction: &str) -> Result<(), std::io::Error> {
    let manager = get_gpio_manager();
    manager.export_pin(chip, pin)?;

    let gpio_direction = match direction {
        "in" => GpioDirection::Input,
        "out" => GpioDirection::Output,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid GPIO direction: {}", direction),
            ))
        }
    };

    manager.set_direction(chip, pin, gpio_direction)?;
    Ok(())
}

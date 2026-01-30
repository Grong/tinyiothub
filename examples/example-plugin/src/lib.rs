//! 示例动态驱动插件

use iot_edge_driver_sdk::*;

/// 示例驱动结构
pub struct ExampleDriver {
    device: Device,
}

impl ExampleDriver {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    pub fn get_driver_info() -> ComponentInfo {
        ComponentInfo {
            name: "ExampleDriver".to_string(),
            version: "1.0.0".to_string(),
            class_name: "ExampleDriver".to_string(),
            device_num: 0,
            description: Some("Example dynamic driver plugin".to_string()),
            options_descriptors: vec![],
            location: None,
        }
    }
}

impl DeviceDriver for ExampleDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>> {
        // 返回示例数据
        Ok(vec![
            ResultValue::integer("temperature".to_string(), 25),
            ResultValue::float("humidity".to_string(), 60.5),
            ResultValue::string("status".to_string(), "running".to_string()),
        ])
    }

    fn execute_command(&mut self, command: &DeviceCommand) -> Result<bool> {
        // 简单的命令处理
        println!("Executing command: {} on device: {}", command.name, self.device.name);
        Ok(true)
    }
}

// 导出驱动（使用FFI宏）
export_driver!(ExampleDriver);

use tinyiothub_core::models::{Device, DeviceCommand};
use std::collections::HashMap;

use crate::{
    domain::device::driver::{DeviceDriver, ResultValue},
    shared::error::Error,
};

#[derive(Debug, Clone, tinyiothub_macros::DeviceDriver)]
#[driver(
    name = "simulator",
    version = "1.0.0",
    description = "Simulated Device Driver for Testing"
)]
#[driver_option(
    label = "Refresh Interval (ms)",
    name = "interval",
    default = "1000",
    option_type = "number",
    required = true
)]
#[driver_option(
    label = "Simulation Mode",
    name = "mode",
    default = "random",
    option_type = "string",
    required = true
)]
#[driver_option(
    label = "Temperature Range",
    name = "temp_range",
    default = "10.0",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Enable Noise",
    name = "enable_noise",
    default = "true",
    option_type = "boolean",
    required = false
)]
pub struct SimulatedDriver {
    pub device: Device,
    pub retry_count: i32,
}

impl SimulatedDriver {
    pub fn new(
        device: Device,
        _context: std::sync::Arc<crate::application::data_context::DataContext>,
    ) -> Self {
        tracing::debug!(
            "SimulatedDriver initialized for device: {}",
            device.display_name.as_deref().unwrap_or(&device.name)
        );

        Self { device, retry_count: 0 }
    }
}

impl DeviceDriver for SimulatedDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    /// 使用宏生成的默认配置
    /// 注意：这个实现是必要的，因为 trait 无法直接调用 Self::get_default_config()
    fn default_config(&self) -> HashMap<String, String> {
        Self::get_default_config()
    }

    /// 为模拟驱动提供快速的重试配置
    fn retry_config(&self) -> crate::domain::device::driver::retry::RetryConfig {
        use std::time::Duration;

        use crate::domain::device::driver::retry::{BackoffStrategy, RetryConfig};

        RetryConfig {
            max_attempts: 1, // 模拟驱动不需要重试
            base_interval: Duration::from_millis(10),
            max_interval: Duration::from_millis(100),
            backoff_strategy: BackoffStrategy::Fixed,
            timeout: Duration::from_millis(500), // 短超时时间，模拟驱动应该很快
        }
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        let start_time = std::time::Instant::now();
        tracing::debug!(
            "SimulatedDriver::read_data called for device: {}",
            self.device.display_name.as_deref().unwrap_or(&self.device.name)
        );

        // 使用统一的配置管理获取配置参数
        let simulation_mode = self.get_config_string("mode", "random");
        let temp_range = self.get_config_number("temp_range", 10.0);
        let enable_noise = self.get_config_boolean("enable_noise", true);

        tracing::debug!(
            "Using config - mode: {}, temp_range: {}, enable_noise: {}",
            simulation_mode,
            temp_range,
            enable_noise
        );

        // 纯模拟驱动，不需要实际的硬件连接
        // 根据设备的实际属性生成模拟数据
        let mut results = Vec::new();

        // 如果设备有属性列表，为每个属性生成模拟值
        if let Some(ref properties) = self.device.properties {
            tracing::debug!("Processing {} properties", properties.len());

            for (index, property) in properties.iter().enumerate() {
                tracing::trace!(
                    "Processing property {}/{}: {}",
                    index + 1,
                    properties.len(),
                    property.name
                );

                let result_value = match property.name.as_str() {
                    // 温度相关属性 - 使用配置的温度范围
                    "alarm_high_temp" => {
                        let base_temp = 75.0;
                        let temp = if simulation_mode == "fixed" {
                            base_temp
                        } else {
                            let noise =
                                if enable_noise { rand::random::<f64>() * temp_range } else { 0.0 };
                            base_temp + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), temp, 2)
                    }
                    "alarm_low_temp" => {
                        let base_temp = 5.0;
                        let temp = if simulation_mode == "fixed" {
                            base_temp
                        } else {
                            let noise =
                                if enable_noise { rand::random::<f64>() * temp_range } else { 0.0 };
                            base_temp + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), temp, 2)
                    }
                    "current_temp" | "temperature" => {
                        let base_temp = 25.0; // 基础温度
                        let temp = if simulation_mode == "fixed" {
                            base_temp
                        } else if simulation_mode == "sine" {
                            // 正弦波模拟
                            let time_factor = (std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as f64)
                                / 60.0; // 每分钟一个周期
                            base_temp + (time_factor.sin() * temp_range / 2.0)
                        } else {
                            // 随机模式
                            let noise = if enable_noise {
                                (rand::random::<f64>() - 0.5) * temp_range
                            } else {
                                0.0
                            };
                            base_temp + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), temp, 2)
                    }

                    // 湿度相关属性 - 使用类似的配置逻辑
                    "alarm_high_humidity" => {
                        let base_humidity = 85.0;
                        let humidity = if simulation_mode == "fixed" {
                            base_humidity
                        } else {
                            let noise =
                                if enable_noise { rand::random::<f64>() * 10.0 } else { 0.0 };
                            base_humidity + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), humidity, 1)
                    }
                    "alarm_low_humidity" => {
                        let base_humidity = 25.0;
                        let humidity = if simulation_mode == "fixed" {
                            base_humidity
                        } else {
                            let noise =
                                if enable_noise { rand::random::<f64>() * 10.0 } else { 0.0 };
                            base_humidity + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), humidity, 1)
                    }
                    "current_humidity" | "humidity" => {
                        let base_humidity = 60.0;
                        let humidity = if simulation_mode == "fixed" {
                            base_humidity
                        } else if simulation_mode == "sine" {
                            let time_factor = (std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as f64)
                                / 120.0; // 每2分钟一个周期
                            base_humidity + (time_factor.sin() * 20.0)
                        } else {
                            let noise = if enable_noise {
                                (rand::random::<f64>() - 0.5) * 40.0
                            } else {
                                0.0
                            };
                            base_humidity + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), humidity, 1)
                    }

                    // 状态相关属性
                    "device_status" => {
                        let status = if simulation_mode == "fixed" {
                            "运行中"
                        } else {
                            let statuses = ["运行中", "待机", "维护中"];
                            statuses[rand::random::<usize>() % statuses.len()]
                        };
                        ResultValue::string(property.name.clone(), status.to_string())
                    }
                    "power_status" => {
                        let power_on = if simulation_mode == "fixed" {
                            true
                        } else {
                            rand::random::<f64>() > 0.1 // 90%概率开启
                        };
                        ResultValue::boolean(property.name.clone(), power_on)
                    }

                    // 版本和配置属性
                    "firmware_version" => {
                        let version = if simulation_mode == "fixed" {
                            "v1.2.3"
                        } else {
                            let versions = ["v1.2.3", "v1.2.4", "v1.3.0"];
                            versions[rand::random::<usize>() % versions.len()]
                        };
                        ResultValue::string(property.name.clone(), version.to_string())
                    }
                    "sampling_interval" => {
                        // 使用配置的刷新间隔
                        let interval = self.get_config_number("interval", 1000.0) as i64;
                        ResultValue::integer(property.name.clone(), interval)
                    }

                    // 时间相关属性
                    "last_calibration" => {
                        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        ResultValue::string(property.name.clone(), now)
                    }

                    // 摄像头相关属性
                    "resolution" => {
                        ResultValue::string(property.name.clone(), "1920x1080".to_string())
                    }
                    "frame_rate" => ResultValue::integer(property.name.clone(), 30),
                    "brightness" => ResultValue::integer(property.name.clone(), 50),
                    "contrast" => ResultValue::integer(property.name.clone(), 50),
                    "recording_status" => ResultValue::boolean(property.name.clone(), true),

                    // 机器人相关属性
                    "joint_angle_1" | "joint_angle_2" | "joint_angle_3" | "joint_angle_4"
                    | "joint_angle_5" | "joint_angle_6" => {
                        let angle = if simulation_mode == "fixed" {
                            0.0
                        } else {
                            -180.0 + (rand::random::<f64>() * 360.0)
                        };
                        ResultValue::float_with_precision(property.name.clone(), angle, 1)
                    }
                    "robot_status" => {
                        ResultValue::string(property.name.clone(), "待机".to_string())
                    }
                    "current_task" => {
                        ResultValue::string(property.name.clone(), "无任务".to_string())
                    }

                    // 默认处理：根据数据类型生成合适的值
                    _ => {
                        match property.data_type.as_deref() {
                            Some("number") | Some("float") => {
                                let value = if simulation_mode == "fixed" {
                                    50.0
                                } else {
                                    rand::random::<f64>() * 100.0
                                };
                                ResultValue::float_with_precision(property.name.clone(), value, 2)
                            }
                            Some("integer") | Some("int") => {
                                let value = if simulation_mode == "fixed" {
                                    50
                                } else {
                                    rand::random::<i64>() % 100
                                };
                                ResultValue::integer(property.name.clone(), value)
                            }
                            Some("boolean") | Some("bool") => {
                                let value = if simulation_mode == "fixed" {
                                    true
                                } else {
                                    rand::random::<bool>()
                                };
                                ResultValue::boolean(property.name.clone(), value)
                            }
                            _ => {
                                // 默认字符串类型
                                let mode_suffix = if simulation_mode != "random" {
                                    format!("_{}", simulation_mode)
                                } else {
                                    String::new()
                                };
                                ResultValue::string(
                                    property.name.clone(),
                                    format!("模拟值{}", mode_suffix),
                                )
                            }
                        }
                    }
                };

                results.push(result_value);
            }
        } else {
            tracing::debug!("No properties found, generating default values");
            // 如果没有属性列表，生成一些默认的模拟数据
            results.push(ResultValue::integer("counter".to_string(), 42));
            results.push(ResultValue::float("temperature".to_string(), 23.5));
            results.push(ResultValue::boolean("active".to_string(), true));
            results.push(ResultValue::string("mode".to_string(), simulation_mode));
        }

        let elapsed = start_time.elapsed();
        tracing::debug!(
            "SimulatedDriver generated {} values for device: {} in {:?}",
            results.len(),
            self.device.display_name.as_deref().unwrap_or(&self.device.name),
            elapsed
        );

        if elapsed > std::time::Duration::from_millis(10) {
            tracing::warn!(
                "SimulatedDriver::read_data took longer than expected: {:?} for device: {}",
                elapsed,
                self.device.display_name.as_deref().unwrap_or(&self.device.name)
            );
        }
        Ok(results)
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!("Executing Simulated command: {} on device {}", cmd.name, self.device.name);

        // 模拟设备总是成功执行命令
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::application::data_context::DataContext;

    fn create_test_device() -> Device {
        Device {
            id: "test-device-1".to_string(),
            name: "测试设备".to_string(),
            display_name: Some("测试设备显示名".to_string()),
            driver_name: Some("SimulatedDriver".to_string()),
            driver_options: Some(r#"{"mode": "sine", "temp_range": "15.0"}"#.to_string()),
            protocol_type: Some("simulation".to_string()),
            properties: None,
            commands: None,
            created_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            updated_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_macro_generated_default_config() {
        // 测试宏生成的默认配置
        let default_config = SimulatedDriver::get_default_config();

        assert_eq!(default_config.get("interval"), Some(&"1000".to_string()));
        assert_eq!(default_config.get("mode"), Some(&"random".to_string()));
        assert_eq!(default_config.get("temp_range"), Some(&"10.0".to_string()));
        assert_eq!(default_config.get("enable_noise"), Some(&"true".to_string()));
        assert_eq!(default_config.len(), 4);
    }

    #[tokio::test]
    async fn test_driver_default_config_method() {
        let device = create_test_device();
        let context = Arc::new(DataContext::new_mock().await);
        let driver = SimulatedDriver::new(device, context);

        // 测试 DeviceDriver trait 的 default_config 方法
        let config = driver.default_config();

        assert_eq!(config.get("interval"), Some(&"1000".to_string()));
        assert_eq!(config.get("mode"), Some(&"random".to_string()));
        assert_eq!(config.get("temp_range"), Some(&"10.0".to_string()));
        assert_eq!(config.get("enable_noise"), Some(&"true".to_string()));
    }

    #[tokio::test]
    async fn test_config_initialization_with_device_options() {
        let device = create_test_device();
        let context = Arc::new(DataContext::new_mock().await);
        let driver = SimulatedDriver::new(device, context);

        // 测试配置初始化（合并默认配置和设备配置）
        let config = driver.init_config();

        // 设备配置应该覆盖默认配置
        assert_eq!(config.get_string("mode", "unknown"), "sine");
        assert_eq!(config.get_number("temp_range", 0.0), 15.0);

        // 未在设备配置中指定的参数应该使用默认值
        assert_eq!(config.get_integer("interval", 0), 1000);
        assert!(config.get_boolean("enable_noise", false));
    }

    #[tokio::test]
    async fn test_config_convenience_methods() {
        let device = create_test_device();
        let context = Arc::new(DataContext::new_mock().await);
        let driver = SimulatedDriver::new(device, context);

        // 测试便捷的配置获取方法
        assert_eq!(driver.get_config_integer("interval", 0), 1000);
        assert_eq!(driver.get_config_string("mode", "unknown"), "sine");
        assert_eq!(driver.get_config_number("temp_range", 0.0), 15.0);
        assert!(driver.get_config_boolean("enable_noise", false));

        // 测试不存在的配置参数
        assert_eq!(driver.get_config_string("non_existent", "default"), "default");
        assert_eq!(driver.get_config_integer("non_existent", 42), 42);
    }

    #[test]
    fn test_driver_info_generation() {
        // 测试宏生成的驱动信息
        let driver_info = SimulatedDriver::get_driver_info();

        assert_eq!(driver_info.name, "simulator");
        assert_eq!(driver_info.version, "1.0.0");
        assert!(driver_info.class_name.contains("SimulatedDriver"));
        assert_eq!(
            driver_info.description,
            Some("Simulated Device Driver for Testing".to_string())
        );

        // 检查选项描述符
        let options_json: Vec<tinyiothub_core::models::component::ComponentOption> =
            serde_json::from_str(&driver_info.options_descriptors).unwrap();

        assert_eq!(options_json.len(), 4);

        // 验证每个选项
        let interval_option = options_json.iter().find(|opt| opt.name == "interval").unwrap();
        assert_eq!(interval_option.label, "Refresh Interval (ms)");
        assert_eq!(interval_option.default_value, "1000");
        assert_eq!(interval_option.option_type, "number");
        assert!(interval_option.required);

        let mode_option = options_json.iter().find(|opt| opt.name == "mode").unwrap();
        assert_eq!(mode_option.label, "Simulation Mode");
        assert_eq!(mode_option.default_value, "random");
        assert_eq!(mode_option.option_type, "string");
        assert!(mode_option.required);
    }

    #[tokio::test]
    async fn test_read_data_with_config() {
        let device = create_test_device();
        let context = Arc::new(DataContext::new_mock().await);
        let mut driver = SimulatedDriver::new(device, context);

        // 测试数据读取
        let result = driver.read_data();
        assert!(result.is_ok());

        let values = result.unwrap();
        assert!(!values.is_empty());

        // 验证返回的数据结构
        for value in &values {
            assert!(!value.name.is_empty());
            assert!(!value.value_type.is_empty());
        }
    }
}

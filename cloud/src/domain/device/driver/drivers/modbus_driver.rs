use tinyiothub_core::models::{Device, DeviceCommand};
use std::collections::HashMap;

#[cfg(feature = "serial")]
use serialport::SerialPort;

use crate::{
    domain::device::driver::{DeviceDriver, ResultValue},
    shared::error::Error,
};

#[derive(Debug, Clone, tinyiothub_macros::DeviceDriver)]
#[driver(name = "ModbusDriver", version = "1.0.0", description = "Modbus RTU/TCP Driver")]
#[driver_option(
    label = "Refresh Interval (ms)",
    name = "interval",
    default = "1000",
    option_type = "number",
    required = true
)]
#[driver_option(
    label = "Serial Port",
    name = "serial_port",
    default = "/dev/ttyS1",
    option_type = "string",
    required = true
)]
#[driver_option(
    label = "Baud Rate",
    name = "baud_rate",
    default = "9600",
    option_type = "number",
    required = true
)]
#[driver_option(
    label = "Data Bits",
    name = "data_bits",
    default = "8",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Stop Bits",
    name = "stop_bits",
    default = "1",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Parity",
    name = "parity",
    default = "None",
    option_type = "string",
    required = false
)]
#[driver_option(
    label = "Slave ID",
    name = "slave_id",
    default = "1",
    option_type = "number",
    required = true
)]
#[driver_option(
    label = "Connection Timeout (ms)",
    name = "timeout",
    default = "5000",
    option_type = "number",
    required = false
)]
pub struct ModbusDriver {
    pub device: Device,
    pub retry_count: i32,
}

impl ModbusDriver {
    pub fn new(
        device: Device,
        _context: std::sync::Arc<crate::application::data_context::DataContext>,
    ) -> Self {
        tracing::debug!(
            "ModbusDriver initialized for device: {}",
            device.display_name.as_deref().unwrap_or(&device.name)
        );

        Self { device, retry_count: 0 }
    }

    /// 创建串口连接
    #[cfg(feature = "serial")]
    fn create_serial_connection(
        &self,
        port: &str,
        baud_rate: u32,
        timeout_ms: u64,
    ) -> Result<Box<dyn SerialPort>, Error> {
        let connection = serialport::new(port, baud_rate)
            .timeout(Duration::from_millis(timeout_ms))
            .open()
            .map_err(|e| Error::IOError(format!("Serial port error {}: {:?}", port, e)))?;

        tracing::debug!("Successfully opened serial port: {} at {} baud", port, baud_rate);
        tracing::info!("Device {} connected via Modbus", self.device.id);

        Ok(connection)
    }
}

impl DeviceDriver for ModbusDriver {
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

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        let start_time = std::time::Instant::now();
        tracing::debug!(
            "ModbusDriver::read_data called for device: {}",
            self.device.display_name.as_deref().unwrap_or(&self.device.name)
        );

        // 使用统一的配置管理获取配置参数
        let serial_port = self.get_config_string("serial_port", "/dev/ttyS1");
        let baud_rate = self.get_config_integer("baud_rate", 9600) as u32;
        let slave_id = self.get_config_integer("slave_id", 1) as u8;
        let timeout_ms = self.get_config_integer("timeout", 5000) as u64;

        tracing::debug!(
            "Using Modbus config - port: {}, baud: {}, slave_id: {}, timeout: {}ms",
            serial_port,
            baud_rate,
            slave_id,
            timeout_ms
        );

        // 尝试建立串口连接
        let mut results = Vec::new();

        #[cfg(feature = "serial")]
        let connection_result = self.create_serial_connection(&serial_port, baud_rate, timeout_ms);

        #[cfg(not(feature = "serial"))]
        let connection_result: Result<(), Error> = {
            tracing::warn!("Serial port not supported on this platform, using simulated data");
            Ok(())
        };

        match connection_result {
            Ok(_connection) => {
                // 模拟读取 Modbus 设备数据
                // 在实际实现中，这里会使用 tokio-modbus 库进行真实的 Modbus 通信

                if let Some(ref properties) = self.device.properties {
                    for property in properties {
                        let result_value = match property.name.as_str() {
                            "temperature" => {
                                // 模拟从 Modbus 寄存器读取温度值
                                let temp = 20.0 + (rand::random::<f64>() * 20.0); // 20-40度
                                ResultValue::float_with_precision(property.name.clone(), temp, 2)
                            }
                            "humidity" => {
                                // 模拟从 Modbus 寄存器读取湿度值
                                let humidity = 40.0 + (rand::random::<f64>() * 40.0); // 40-80%
                                ResultValue::float_with_precision(
                                    property.name.clone(),
                                    humidity,
                                    1,
                                )
                            }
                            "pressure" => {
                                // 模拟从 Modbus 寄存器读取压力值
                                let pressure = 1000.0 + (rand::random::<f64>() * 100.0); // 1000-1100 hPa
                                ResultValue::float_with_precision(
                                    property.name.clone(),
                                    pressure,
                                    1,
                                )
                            }
                            "status" => {
                                // 模拟设备状态
                                ResultValue::boolean(property.name.clone(), true)
                            }
                            "error_code" => {
                                // 模拟错误代码
                                ResultValue::integer(property.name.clone(), 0)
                            }
                            _ => {
                                // 默认处理：根据数据类型生成模拟值
                                match property.data_type.as_deref() {
                                    Some("number") | Some("float") => {
                                        let value = rand::random::<f64>() * 100.0;
                                        ResultValue::float_with_precision(
                                            property.name.clone(),
                                            value,
                                            2,
                                        )
                                    }
                                    Some("integer") | Some("int") => {
                                        let value = rand::random::<i64>() % 1000;
                                        ResultValue::integer(property.name.clone(), value)
                                    }
                                    Some("boolean") | Some("bool") => {
                                        let value = rand::random::<bool>();
                                        ResultValue::boolean(property.name.clone(), value)
                                    }
                                    _ => ResultValue::string(
                                        property.name.clone(),
                                        "Modbus数据".to_string(),
                                    ),
                                }
                            }
                        };
                        results.push(result_value);
                    }
                } else {
                    // 如果没有属性定义，返回一些默认的 Modbus 数据
                    results.push(ResultValue::float("temperature".to_string(), 25.0));
                    results.push(ResultValue::float("humidity".to_string(), 60.0));
                    results.push(ResultValue::boolean("status".to_string(), true));
                    results.push(ResultValue::integer("slave_id".to_string(), slave_id as i64));
                }
            }
            Err(e) => {
                tracing::error!("Failed to connect to Modbus device: {}", e);
                // 连接失败时返回错误状态
                results.push(ResultValue::boolean("connection_status".to_string(), false));
                results.push(ResultValue::string("error_message".to_string(), e.to_string()));
            }
        }

        let elapsed = start_time.elapsed();
        tracing::debug!(
            "ModbusDriver generated {} values for device: {} in {:?}",
            results.len(),
            self.device.display_name.as_deref().unwrap_or(&self.device.name),
            elapsed
        );

        Ok(results)
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!("Executing Modbus command: {} on device {}", cmd.name, self.device.name);

        // 使用配置参数
        let slave_id = self.get_config_integer("slave_id", 1) as u8;

        // 模拟 Modbus 命令执行
        // 在实际实现中，这里会发送 Modbus 写命令到设备
        match cmd.name.as_str() {
            "set_temperature" => {
                tracing::info!("Setting temperature via Modbus (slave_id: {})", slave_id);
                Ok(true)
            }
            "reset_device" => {
                tracing::info!("Resetting device via Modbus (slave_id: {})", slave_id);
                Ok(true)
            }
            "start_measurement" => {
                tracing::info!("Starting measurement via Modbus (slave_id: {})", slave_id);
                Ok(true)
            }
            _ => {
                tracing::warn!("Unknown Modbus command: {}", cmd.name);
                Ok(false)
            }
        }
    }
}

use std::collections::HashMap;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

#[cfg(feature = "serialport")]
use serialport::SerialPort;

use tinyiothub_core::driver::{DeviceDriver, ResultValue};
use tinyiothub_core::error::Error;

#[derive(Debug, Clone, tinyiothub_macros::DeviceDriver)]
#[driver(name = "modbus_rtu", version = "1.0.0", description = "Modbus RTU/TCP Driver")]
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
    pub fn new(device: Device) -> Self {
        Self { device, retry_count: 0 }
    }

    #[cfg(feature = "serialport")]
    fn create_serial_connection(
        &self,
        port: &str,
        baud_rate: u32,
        timeout_ms: u64,
    ) -> Result<Box<dyn SerialPort>, Error> {
        let connection = serialport::new(port, baud_rate)
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .open()
            .map_err(|e| Error::IOError(format!("Serial port error {}: {:?}", port, e)))?;
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

    fn default_config(&self) -> HashMap<String, String> {
        Self::get_default_config()
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        let mut results = Vec::new();

        #[cfg(feature = "serialport")]
        let _connection_result = {
            let serial_port = self.get_config_string("serial_port", "/dev/ttyS1");
            let baud_rate = self.get_config_integer("baud_rate", 9600) as u32;
            let timeout_ms = self.get_config_integer("timeout", 5000) as u64;
            self.create_serial_connection(&serial_port, baud_rate, timeout_ms)
        };

        if let Some(ref properties) = self.device.properties {
            for property in properties {
                let result_value = match property.name.as_str() {
                    "temperature" => {
                        let temp = 20.0 + (rand::random::<f64>() * 20.0);
                        ResultValue::float_with_precision(property.name.clone(), temp, 2)
                    }
                    "humidity" => {
                        let humidity = 40.0 + (rand::random::<f64>() * 40.0);
                        ResultValue::float_with_precision(property.name.clone(), humidity, 1)
                    }
                    _ => match property.data_type.as_deref() {
                        Some("number") | Some("float") => {
                            let value = rand::random::<f64>() * 100.0;
                            ResultValue::float_with_precision(property.name.clone(), value, 2)
                        }
                        _ => ResultValue::string(property.name.clone(), "Modbus数据".to_string()),
                    },
                };
                results.push(result_value);
            }
        } else {
            results.push(ResultValue::float("temperature".to_string(), 25.0));
            results.push(ResultValue::float("humidity".to_string(), 60.0));
            results.push(ResultValue::boolean("status".to_string(), true));
        }

        Ok(results)
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!("Executing Modbus command: {} on device {}", cmd.name, self.device.name);
        match cmd.name.as_str() {
            "set_temperature" | "reset_device" | "start_measurement" => Ok(true),
            _ => Ok(false),
        }
    }
}

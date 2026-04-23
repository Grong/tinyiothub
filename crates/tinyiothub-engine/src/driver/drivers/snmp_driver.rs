use std::collections::HashMap;

use tinyiothub_core::models::{
    component::{Component, ComponentOption, CreateComponentRequest},
    device::Device,
    device_command::DeviceCommand,
    device_property::DeviceProperty,
};

#[cfg(feature = "serialport")]
use serialport::SerialPort;

use crate::driver::{DeviceDriver, ResultValue};
use tinyiothub_core::error::Error;

#[derive(Debug, Clone)]
pub struct SnmpDriver {
    pub device: Device,
    pub retry_count: i32,
}

impl SnmpDriver {
    pub fn new(device: Device) -> Self {
        Self { device, retry_count: 0 }
    }

    pub fn get_driver_info() -> Component {
        let opts = vec![
            ComponentOption::new(
                "Refresh Interval (ms)".to_string(),
                "interval".to_string(),
                "1000".to_string(),
                "number".to_string(),
                true,
            ),
            ComponentOption::new(
                "Serial Port".to_string(),
                "serial".to_string(),
                "/dev/ttyS1".to_string(),
                "string".to_string(),
                true,
            ),
            ComponentOption::new(
                "Baud Rate".to_string(),
                "baud_rate".to_string(),
                "9600".to_string(),
                "number".to_string(),
                true,
            ),
            ComponentOption::new(
                "Device Address".to_string(),
                "slave_id".to_string(),
                "1".to_string(),
                "number".to_string(),
                true,
            ),
        ];

        Component::new(CreateComponentRequest {
            name: "SnmpDriver".to_string(),
            version: "1.0.0".to_string(),
            class_name: "tinyiothub::domain::device::driver::drivers::SnmpDriver".to_string(),
            device_num: Some(0),
            description: Some("Snmp Device Driver".to_string()),
            options_descriptors: opts,
            location: None,
        })
    }

    #[allow(dead_code)]
    fn get_slave_id(&self) -> u8 {
        let opts = self.device.driver_options.clone().unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(&opts) {
            if let Some(slave_id_str) = parsed.get("slave_id") {
                return slave_id_str.parse::<u8>().unwrap_or(1);
            }
        }
        1
    }

    #[cfg(feature = "serialport")]
    fn get_connect(&self) -> Result<Box<dyn SerialPort>, Error> {
        let opts = self.device.driver_options.clone().unwrap_or_default();
        let parsed: HashMap<String, String> = serde_json::from_str(&opts)
            .map_err(|e| Error::IOError(format!("Failed to parse driver options: {}", e)))?;

        let default_serial = "/dev/ttyS1".to_string();
        let tty_path = parsed.get("serial").unwrap_or(&default_serial);
        let baud_rate: u32 =
            parsed.get("baud_rate").unwrap_or(&"9600".to_string()).parse().unwrap_or(9600);

        let conn = serialport::new(tty_path, baud_rate)
            .timeout(Duration::from_millis(3000))
            .open()
            .map_err(|e| Error::IOError(format!("Serial port error {}: {:?}", tty_path, e)))?;

        Ok(conn)
    }

    #[allow(dead_code)]
    fn handle_read_command(
        &self,
        prop: &DeviceProperty,
        _data: Vec<u8>,
    ) -> Result<ResultValue, Error> {
        let prop_name = prop.name.clone();

        // Default implementation - return placeholder values
        match prop_name.as_str() {
            "status" => Ok(ResultValue {
                name: prop_name,
                value: Some("1".to_string()),
                value_type: "int".to_string(),
            }),
            "temperature" => Ok(ResultValue {
                name: prop_name,
                value: Some("25.0".to_string()),
                value_type: "float".to_string(),
            }),
            _ => Ok(ResultValue {
                name: prop_name,
                value: Some("0".to_string()),
                value_type: "int".to_string(),
            }),
        }
    }
}

impl DeviceDriver for SnmpDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // 模拟读取 SNMP 设备数据
        let mut results = Vec::new();

        results.push(ResultValue::string("system_name".to_string(), "SNMP Device".to_string()));
        results.push(ResultValue::integer("uptime".to_string(), 86400));
        results.push(ResultValue::float("cpu_usage".to_string(), 45.2));
        results.push(ResultValue::float("memory_usage".to_string(), 67.8));

        Ok(results)
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!("Executing SNMP command: {} on device {}", cmd.name, self.device.name);

        match cmd.name.as_str() {
            "get_system_info" => {
                tracing::info!("Getting SNMP system info");
                Ok(true)
            }
            "restart" => {
                tracing::info!("Restarting SNMP device");
                Ok(true)
            }
            _ => {
                tracing::warn!("Unknown SNMP command: {}", cmd.name);
                Ok(false)
            }
        }
    }
}

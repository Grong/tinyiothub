use tinyiothub_core::driver::{DeviceDriver, ResultValue};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

#[derive(Debug, Clone, tinyiothub_macros::DeviceDriver)]
#[driver(
    name = "snmp",
    version = "1.0.0",
    description = "SNMP Device Driver (stub — simulated data only)"
)]
#[driver_option(
    label = "Refresh Interval (ms)",
    name = "interval",
    default = "1000",
    option_type = "number",
    required = true
)]
#[driver_option(
    label = "SNMP Version",
    name = "version",
    default = "v2c",
    option_type = "string",
    required = true
)]
#[driver_option(
    label = "Community",
    name = "community",
    default = "public",
    option_type = "string",
    required = true
)]
#[driver_option(
    label = "Host",
    name = "host",
    default = "127.0.0.1",
    option_type = "string",
    required = true
)]
#[driver_option(
    label = "Port",
    name = "port",
    default = "161",
    option_type = "number",
    required = true
)]
pub struct SnmpDriver {
    pub device: Device,
}

impl SnmpDriver {
    pub fn new(device: Device) -> Self {
        Self { device }
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
        let results = vec![
            ResultValue::string("system_name".to_string(), "SNMP Device".to_string()),
            ResultValue::integer("uptime".to_string(), 86400),
            ResultValue::float("cpu_usage".to_string(), 45.2),
            ResultValue::float("memory_usage".to_string(), 67.8),
        ];
        Ok(results)
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!("Executing SNMP command: {} on device {}", cmd.name, self.device.name);
        match cmd.name.as_str() {
            "get_system_info" | "restart" => Ok(true),
            _ => Ok(false),
        }
    }
}

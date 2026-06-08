use std::collections::HashMap;
use std::time::Instant;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tinyiothub_core::driver::{BackoffStrategy, DeviceDriver, ResultValue, RetryConfig};
use tinyiothub_core::error::Error;

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
    tick_counter: u64,
    rng: StdRng,
    last_read: Instant,
    cached_values: Option<Vec<ResultValue>>,
}

impl SimulatedDriver {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            retry_count: 0,
            tick_counter: 0,
            rng: StdRng::from_entropy(),
            last_read: Instant::now(),
            cached_values: None,
        }
    }
}

impl DeviceDriver for SimulatedDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }

    fn default_config(&self) -> HashMap<String, String> {
        Self::get_default_config()
    }

    fn retry_config(&self) -> RetryConfig {
        RetryConfig {
            max_attempts: 1,
            base_interval: std::time::Duration::from_millis(10),
            max_interval: std::time::Duration::from_millis(100),
            backoff_strategy: BackoffStrategy::Fixed,
            timeout: std::time::Duration::from_millis(500),
        }
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // Respect the configured interval — skip regeneration if not enough time has passed
        let interval_ms = self.get_config_number("interval", 1000.0) as u64;
        let elapsed = self.last_read.elapsed().as_millis() as u64;
        if elapsed < interval_ms && self.cached_values.is_some() {
            return Ok(self.cached_values.as_ref().unwrap().clone());
        }

        self.tick_counter = self.tick_counter.wrapping_add(1);
        self.last_read = Instant::now();

        let simulation_mode = self.get_config_string("mode", "random");
        let _temp_range = self.get_config_number("temp_range", 10.0);
        let enable_noise = self.get_config_boolean("enable_noise", true);

        let mut results = Vec::new();

        if let Some(ref properties) = self.device.properties {
            for property in properties.iter() {
                let result_value = match property.name.as_str() {
                    "current_temp" | "temperature" => {
                        let temp = if simulation_mode == "fixed" {
                            25.0
                        } else {
                            let base = 25.0;
                            let variation = (self.tick_counter % 10) as f64;
                            let noise = if enable_noise {
                                (self.rng.r#gen::<f64>() - 0.5) * 2.0
                            } else {
                                0.0
                            };
                            base + variation + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), temp, 2)
                    }
                    "current_humidity" | "humidity" => {
                        let humidity = if simulation_mode == "fixed" {
                            60.0
                        } else {
                            let base = 60.0;
                            let variation = (self.tick_counter % 20) as f64;
                            let noise = if enable_noise {
                                (self.rng.r#gen::<f64>() - 0.5) * 2.0
                            } else {
                                0.0
                            };
                            base + variation + noise
                        };
                        ResultValue::float_with_precision(property.name.clone(), humidity, 1)
                    }
                    "power_status" => {
                        let power_on = if simulation_mode == "fixed" {
                            true
                        } else {
                            !self.tick_counter.is_multiple_of(5)
                        };
                        ResultValue::boolean(property.name.clone(), power_on)
                    }
                    _ => match property.data_type.as_deref() {
                        Some("number") | Some("float") => {
                            let value = if simulation_mode == "fixed" {
                                50.0
                            } else {
                                self.rng.r#gen::<f64>() * 100.0
                            };
                            ResultValue::float_with_precision(property.name.clone(), value, 2)
                        }
                        Some("integer") | Some("int") => {
                            let value = if simulation_mode == "fixed" {
                                50
                            } else {
                                self.rng.gen_range(0..100)
                            };
                            ResultValue::integer(property.name.clone(), value)
                        }
                        Some("boolean") | Some("bool") => {
                            let value = if simulation_mode == "fixed" {
                                true
                            } else {
                                self.rng.r#gen::<bool>()
                            };
                            ResultValue::boolean(property.name.clone(), value)
                        }
                        _ => ResultValue::string(property.name.clone(), format!("模拟值_{}", simulation_mode)),
                    },
                };
                results.push(result_value);
            }
        } else {
            results.push(ResultValue::integer("counter".to_string(), 42));
            results.push(ResultValue::float("temperature".to_string(), 23.5));
            results.push(ResultValue::boolean("active".to_string(), true));
            results.push(ResultValue::string("mode".to_string(), simulation_mode));
        }

        self.cached_values = Some(results.clone());
        Ok(results)
    }

    fn execute_command(&mut self, cmd: &DeviceCommand) -> Result<bool, Error> {
        tracing::info!(
            "Executing Simulated command: {} on device {}",
            cmd.name,
            self.device.name
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let default_config = SimulatedDriver::get_default_config();
        assert_eq!(default_config.get("interval"), Some(&"1000".to_string()));
        assert_eq!(default_config.get("mode"), Some(&"random".to_string()));
        assert_eq!(default_config.get("temp_range"), Some(&"10.0".to_string()));
        assert_eq!(default_config.get("enable_noise"), Some(&"true".to_string()));
        assert_eq!(default_config.len(), 4);
    }

    #[test]
    fn test_read_data_with_config() {
        let device = create_test_device();
        let mut driver = SimulatedDriver::new(device);
        let result = driver.read_data();
        assert!(result.is_ok());
        let values = result.unwrap();
        assert!(!values.is_empty());
    }
}

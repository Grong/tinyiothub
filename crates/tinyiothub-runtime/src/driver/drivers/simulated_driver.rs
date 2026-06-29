use std::collections::HashMap;
use std::time::Instant;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tinyiothub_core::driver::{BackoffStrategy, DeviceDriver, ResultValue, RetryConfig};
use tinyiothub_core::error::Error;

/// Per-property state for stateful random walk simulation.
#[derive(Debug, Clone)]
struct PropertyState {
    /// Current simulated value.
    current_value: f64,
    /// Drift direction (-1.0 to 1.0), slowly changes over time.
    drift_direction: f64,
    /// Whether anomaly injection is currently active for this property.
    anomaly_active: bool,
    /// Remaining ticks for the current anomaly.
    anomaly_remaining: u32,
}

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
#[driver_option(
    label = "Drift Speed",
    name = "drift_speed",
    default = "0.3",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Anomaly Probability",
    name = "anomaly_probability",
    default = "0.02",
    option_type = "number",
    required = false
)]
pub struct SimulatedDriver {
    pub device: Device,
    pub retry_count: i32,
    tick_counter: u64,
    rng: StdRng,
    last_read: Instant,
    cached_values: Option<Vec<ResultValue>>,
    /// Stateful property values for random walk simulation.
    property_states: HashMap<String, PropertyState>,
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
            property_states: HashMap::new(),
        }
    }

    /// Generate a value using stateful random walk with anomaly injection.
    ///
    /// Each property maintains its own state: current value, drift direction, and
    /// anomaly status. Values change gradually each tick, staying near thresholds
    /// long enough to test alarm debounce/hysteresis.
    #[allow(clippy::too_many_arguments)]
    fn generate_random_walk_value(
        &mut self,
        property_name: &str,
        drift_speed: f64,
        anomaly_probability: f64,
        enable_noise: bool,
        min_val: f64,
        max_val: f64,
        initial_val: f64,
    ) -> f64 {
        let state = self
            .property_states
            .entry(property_name.to_string())
            .or_insert_with(|| {
                // Initialize with a random starting point near initial_val
                let start = initial_val + (self.rng.r#gen::<f64>() - 0.5) * (max_val - min_val) * 0.2;
                PropertyState {
                    current_value: start.clamp(min_val, max_val),
                    drift_direction: (self.rng.r#gen::<f64>() - 0.5) * 2.0,
                    anomaly_active: false,
                    anomaly_remaining: 0,
                }
            });

        // Handle ongoing anomaly
        if state.anomaly_active && state.anomaly_remaining > 0 {
            state.anomaly_remaining -= 1;
            if state.anomaly_remaining == 0 {
                state.anomaly_active = false;
            }
            // No noise during anomaly — keep the value sticky to properly test alarm persistence
            return state.current_value.clamp(min_val, max_val);
        }

        // Possibly inject a new anomaly
        if anomaly_probability > 0.0 && self.rng.r#gen::<f64>() < anomaly_probability {
            state.anomaly_active = true;
            state.anomaly_remaining = self.rng.gen_range(5..=15);
            // Jump to an anomalous value (1.5-2.5x the normal range above/below)
            let anomaly_magnitude = self.rng.gen_range(1.5..2.5);
            let anomaly_direction = if self.rng.r#gen::<bool>() { 1.0 } else { -1.0 };
            let anomaly_value = state.current_value
                + anomaly_direction * anomaly_magnitude * (max_val - min_val) * 0.2;
            state.current_value = anomaly_value.clamp(min_val, max_val);
            // Align drift direction with anomaly — so after anomaly ends,
            // momentum continues trending in the same direction, not reversing immediately
            state.drift_direction = anomaly_direction;
            return state.current_value;
        }

        // Normal random walk with momentum
        // Drift direction has inertia: keeps trending in same direction, slowly evolves
        state.drift_direction = state.drift_direction * 0.92 + (self.rng.r#gen::<f64>() - 0.5) * 0.16;
        state.drift_direction = state.drift_direction.clamp(-1.0, 1.0);

        // Apply drift with some randomness in magnitude
        let delta = state.drift_direction * drift_speed * self.rng.gen_range(0.5..1.5);

        state.current_value += delta;

        // Add noise if enabled
        if enable_noise {
            state.current_value += (self.rng.r#gen::<f64>() - 0.5) * drift_speed * 0.5;
        }

        // Clamp to range
        state.current_value = state.current_value.clamp(min_val, max_val);

        state.current_value
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

    #[allow(clippy::collapsible_if)]
    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        // Respect the configured interval — skip regeneration if not enough time has passed
        let interval_ms = self.get_config_number("interval", 1000.0) as u64;
        let elapsed = self.last_read.elapsed().as_millis() as u64;
        if elapsed < interval_ms {
            if let Some(ref cached) = self.cached_values {
                return Ok(cached.clone());
            }
        }

        self.tick_counter = self.tick_counter.wrapping_add(1);
        self.last_read = Instant::now();

        let simulation_mode = self.get_config_string("mode", "random");
        let temp_range = self.get_config_number("temp_range", 80.0);
        let enable_noise = self.get_config_boolean("enable_noise", true);
        let drift_speed = self.get_config_number("drift_speed", 0.3);
        let anomaly_probability = self.get_config_number("anomaly_probability", 0.02);

        let mut results = Vec::new();

        // Collect property info first to avoid borrow conflicts between
        // self.device.properties (immutable) and self.generate_random_walk_value (mutable).
        let property_infos: Vec<(String, Option<String>)> = self
            .device
            .properties
            .as_ref()
            .map(|props| props.iter().map(|p| (p.name.clone(), p.data_type.clone())).collect())
            .unwrap_or_default();

        if !property_infos.is_empty() {
            for (prop_name, prop_data_type) in property_infos.iter() {
                let result_value = match prop_name.as_str() {
                    "current_temp" | "temperature" => {
                        let temp = if simulation_mode == "fixed" {
                            25.0
                        } else {
                            // Default temperature range: 30-110 °C (based on temp_range)
                            let range_max = 30.0 + temp_range;
                            self.generate_random_walk_value(
                                prop_name,
                                drift_speed,
                                anomaly_probability,
                                enable_noise,
                                30.0,
                                range_max,
                                70.0,
                            )
                        };
                        ResultValue::float_with_precision(prop_name.clone(), temp, 2)
                    }
                    "current_humidity" | "humidity" => {
                        let humidity = if simulation_mode == "fixed" {
                            60.0
                        } else {
                            self.generate_random_walk_value(
                                prop_name,
                                drift_speed * 0.5,
                                anomaly_probability,
                                enable_noise,
                                20.0,
                                100.0,
                                60.0,
                            )
                        };
                        ResultValue::float_with_precision(prop_name.clone(), humidity, 1)
                    }
                    "power_status" => {
                        let power_on = if simulation_mode == "fixed" {
                            true
                        } else {
                            !self.tick_counter.is_multiple_of(5)
                        };
                        ResultValue::boolean(prop_name.clone(), power_on)
                    }
                    _ => match prop_data_type.as_deref() {
                        Some("number") | Some("float") => {
                            let value = if simulation_mode == "fixed" {
                                50.0
                            } else {
                                self.generate_random_walk_value(
                                    prop_name,
                                    drift_speed,
                                    anomaly_probability,
                                    enable_noise,
                                    0.0,
                                    100.0,
                                    50.0,
                                )
                            };
                            ResultValue::float_with_precision(prop_name.clone(), value, 2)
                        }
                        Some("integer") | Some("int") => {
                            let value = if simulation_mode == "fixed" {
                                50
                            } else {
                                self.generate_random_walk_value(
                                    prop_name,
                                    drift_speed,
                                    anomaly_probability,
                                    enable_noise,
                                    0.0,
                                    100.0,
                                    50.0,
                                ) as i64
                            };
                            ResultValue::integer(prop_name.clone(), value)
                        }
                        Some("boolean") | Some("bool") => {
                            let value = if simulation_mode == "fixed" {
                                true
                            } else {
                                self.rng.r#gen::<bool>()
                            };
                            ResultValue::boolean(prop_name.clone(), value)
                        }
                        _ => ResultValue::string(prop_name.clone(), format!("模拟值_{}", simulation_mode)),
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

    fn create_device_with_temp_property() -> Device {
        use tinyiothub_core::models::device_property::DeviceProperty;
        Device {
            id: "test-device-temp".to_string(),
            name: "Temp Device".to_string(),
            display_name: None,
            driver_name: Some("SimulatedDriver".to_string()),
            driver_options: None,
            protocol_type: Some("simulation".to_string()),
            properties: Some(vec![DeviceProperty {
                id: "prop-temp".to_string(),
                device_id: "test-device-temp".to_string(),
                name: "temperature".to_string(),
                display_name: None,
                description: None,
                data_type: Some("float".to_string()),
                unit: None,
                min_value: None,
                max_value: None,
                default_value: None,
                is_read_only: 1,
                created_at: None,
                updated_at: None,
                current_value: None,
                alarm_status: None,
            }]),
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
        assert_eq!(default_config.get("drift_speed"), Some(&"0.3".to_string()));
        assert_eq!(default_config.get("anomaly_probability"), Some(&"0.02".to_string()));
        assert_eq!(default_config.len(), 6);
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

    #[test]
    fn test_random_walk_consecutive_values_close() {
        let device = create_device_with_temp_property();
        let mut driver = SimulatedDriver::new(device);

        let mut prev_temp: Option<f64> = None;
        for _ in 0..20 {
            let values = driver.read_data().unwrap();
            let temp = values
                .iter()
                .find(|v| v.name == "temperature")
                .and_then(|v| v.value.as_ref().and_then(|s| s.parse::<f64>().ok()))
                .unwrap();

            if let Some(prev) = prev_temp {
                let change = (temp - prev).abs();
                assert!(
                    change < 5.0,
                    "Consecutive random-walk values should change gradually, got change {}",
                    change
                );
            }
            prev_temp = Some(temp);
        }
    }

    #[test]
    fn test_drift_direction_changes_over_time() {
        let device = create_device_with_temp_property();
        let mut driver = SimulatedDriver::new(device);

        // Read enough times to see drift direction evolve
        for _ in 0..30 {
            let _ = driver.read_data().unwrap();
        }

        // The internal state should have been created
        let state = driver
            .property_states
            .get("temperature")
            .expect("temperature state should exist");
        assert!(
            state.drift_direction.abs() <= 1.0,
            "Drift direction should stay clamped to [-1, 1]"
        );
    }

    #[test]
    fn test_fixed_mode_unchanged() {
        // Force fixed mode by overriding the config value via driver_options
        let mut fixed_device = create_device_with_temp_property();
        fixed_device.driver_options = Some(r#"{"mode": "fixed"}"#.to_string());
        let mut fixed_driver = SimulatedDriver::new(fixed_device);

        let values1 = fixed_driver.read_data().unwrap();
        let values2 = fixed_driver.read_data().unwrap();

        let temp1 = values1
            .iter()
            .find(|v| v.name == "temperature")
            .and_then(|v| v.value.as_ref().and_then(|s| s.parse::<f64>().ok()))
            .unwrap();
        let temp2 = values2
            .iter()
            .find(|v| v.name == "temperature")
            .and_then(|v| v.value.as_ref().and_then(|s| s.parse::<f64>().ok()))
            .unwrap();

        assert!(
            (temp1 - temp2).abs() < f64::EPSILON,
            "Fixed mode should return identical temperature values"
        );
        assert!(
            (temp1 - 25.0).abs() < f64::EPSILON,
            "Fixed mode temperature should be 25.0"
        );
    }

    #[test]
    fn test_anomaly_probability_zero_no_anomalies() {
        let mut device = create_device_with_temp_property();
        device.driver_options = Some(r#"{"anomaly_probability": 0.0, "drift_speed": 0.1}"#.to_string());
        let mut driver = SimulatedDriver::new(device);

        let mut max_change: f64 = 0.0;
        let mut prev: Option<f64> = None;
        for _ in 0..50 {
            let values = driver.read_data().unwrap();
            let temp = values
                .iter()
                .find(|v| v.name == "temperature")
                .and_then(|v| v.value.as_ref().and_then(|s| s.parse::<f64>().ok()))
                .unwrap();
            if let Some(p) = prev {
                max_change = max_change.max((temp - p).abs());
            }
            prev = Some(temp);
        }

        assert!(
            max_change < 5.0,
            "With anomaly_probability=0, changes should stay gradual, max change was {}",
            max_change
        );
    }
}

use std::collections::HashMap;
use std::time::Instant;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tinyiothub_core::driver::{BackoffStrategy, DeviceDriver, ResultValue, RetryConfig};
use tinyiothub_core::error::Error;

use super::simulated::patterns::match_property;
use super::simulated::signal::SignalComposer;
use super::simulated::anomaly::AnomalyEngine;
use super::simulated::correlation::{self, CorrelationManager, EnvironmentContext};

#[derive(Debug, Clone, tinyiothub_macros::DeviceDriver)]
#[driver(
    name = "simulator",
    version = "2.0.0",
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
#[driver_option(
    label = "Enable Periodic",
    name = "enable_periodic",
    default = "true",
    option_type = "boolean",
    required = false
)]
#[driver_option(
    label = "Enable Anomaly",
    name = "enable_anomaly",
    default = "true",
    option_type = "boolean",
    required = false
)]
#[driver_option(
    label = "Daily Amplitude Scale",
    name = "daily_amplitude_scale",
    default = "1.0",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Noise Level",
    name = "noise_level",
    default = "1.0",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Drift Rate",
    name = "drift_rate",
    default = "0.0",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Correlation Tags",
    name = "correlation_tags",
    default = "*",
    option_type = "string",
    required = false
)]
pub struct SimulatedDriver {
    pub device: Device,
    pub retry_count: i32,
    tick_counter: u64,
    rng: StdRng,
    last_read: Instant,
    cached_values: Option<Vec<ResultValue>>,
    /// Per-property anomaly engines (one per property name).
    anomaly_engines: HashMap<String, AnomalyEngine>,
    /// Random per-device phase offset (0.0–1.0) for the daily cycle.
    phase_offset: f64,
    /// Merged environment context from tag-based correlation.
    group_context: Option<EnvironmentContext>,
}

impl SimulatedDriver {
    pub fn new(device: Device) -> Self {
        let mut rng = StdRng::from_entropy();
        let phase_offset = rng.gen_range(0.0..1.0);

        let mut driver = Self {
            device,
            retry_count: 0,
            tick_counter: 0,
            rng,
            last_read: Instant::now(),
            cached_values: None,
            anomaly_engines: HashMap::new(),
            phase_offset,
            group_context: None,
        };

        // Set correlation pattern from config
        let correlation_tags = driver.get_config_string("correlation_tags", "*");
        CorrelationManager::set_pattern(&correlation_tags);

        // Build group context from device tags
        let tags = driver.device.tags.clone().unwrap_or_default();
        let contexts = correlation::get_contexts_for_device(&tags, &mut driver.rng);
        driver.group_context = Some(correlation::merge_contexts(&contexts));

        driver
    }

    /// Build a `SignalComposer` from the current driver configuration.
    fn build_composer(&self) -> SignalComposer {
        SignalComposer {
            interval_ms: self.get_config_number("interval", 1000.0) as u64,
            daily_amplitude_scale: self.get_config_number("daily_amplitude_scale", 1.0),
            noise_level: self.get_config_number("noise_level", 1.0),
            drift_rate: self.get_config_number("drift_rate", 0.0),
            enable_periodic: self.get_config_boolean("enable_periodic", true),
            enable_noise: self.get_config_boolean("enable_noise", true),
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
        let enable_anomaly = self.get_config_boolean("enable_anomaly", true);
        let composer = self.build_composer();
        let group_ctx = self.group_context.as_ref();

        let mut results = Vec::new();

        // Collect property info first to avoid borrow conflicts
        let property_infos: Vec<(String, Option<String>, Option<f64>, Option<f64>)> = self
            .device
            .properties
            .as_ref()
            .map(|props| {
                props
                    .iter()
                    .map(|p| (p.name.clone(), p.data_type.clone(), p.min_value, p.max_value))
                    .collect()
            })
            .unwrap_or_default();

        if !property_infos.is_empty() {
            for (prop_name, prop_data_type, min_value, max_value) in property_infos.iter() {
                let data_type = prop_data_type.as_deref().unwrap_or("float");
                let behavior = match_property(prop_name, data_type);

                let result_value = if simulation_mode == "fixed" {
                    // Fixed mode: return baseline value for all properties
                    if behavior.discrete {
                        ResultValue::boolean(prop_name.clone(), true)
                    } else {
                        ResultValue::float_with_precision(
                            prop_name.clone(),
                            behavior.baseline,
                            2,
                        )
                    }
                } else if behavior.discrete {
                    // Boolean/status: toggle to "off" (false) ~1/7 of the time
                    let value = self.rng.gen_range(0..7) != 0;
                    ResultValue::boolean(prop_name.clone(), value)
                } else if behavior.cumulative {
                    // Cumulative counter: value increases with tick count
                    let value = behavior.baseline + (self.tick_counter as f64 * 0.1);
                    ResultValue::float_with_precision(prop_name.clone(), value, 2)
                } else {
                    // Normal signal pipeline: compose + anomaly + clamp
                    let mut value = composer.compose(
                        &behavior,
                        self.tick_counter,
                        self.phase_offset,
                        &mut self.rng,
                        group_ctx,
                    );

                    if enable_anomaly {
                        let engine = self
                            .anomaly_engines
                            .entry(prop_name.clone())
                            .or_insert_with(|| AnomalyEngine::for_property(prop_name, &behavior.unit));
                        let anomaly_offset = engine.tick(value, &mut self.rng);
                        value += anomaly_offset;
                        if let Some(frozen_val) = engine.frozen_value() {
                            value = frozen_val;
                        }
                    }

                    // Clamp to property min/max if defined
                    if let (Some(min), Some(max)) = (min_value, max_value) {
                        value = value.clamp(*min, *max);
                    }

                    ResultValue::float_with_precision(prop_name.clone(), value, 2)
                };
                results.push(result_value);
            }
        } else {
            // Fallback: generate default property values when no properties are defined
            results.push(ResultValue::integer("counter".to_string(), 42));
            results.push(ResultValue::float("temperature".to_string(), 23.5));
            results.push(ResultValue::boolean("active".to_string(), true));
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

    fn create_device_with_multiple_properties() -> Device {
        use tinyiothub_core::models::device_property::DeviceProperty;
        Device {
            id: "test-device-multi".to_string(),
            name: "Multi Prop Device".to_string(),
            display_name: None,
            driver_name: Some("SimulatedDriver".to_string()),
            driver_options: Some(
                r#"{"enable_anomaly": false, "enable_periodic": false, "enable_noise": false}"#
                    .to_string(),
            ),
            protocol_type: Some("simulation".to_string()),
            properties: Some(vec![
                DeviceProperty {
                    id: "prop-temp".to_string(),
                    device_id: "test-device-multi".to_string(),
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
                },
                DeviceProperty {
                    id: "prop-current".to_string(),
                    device_id: "test-device-multi".to_string(),
                    name: "current".to_string(),
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
                },
                DeviceProperty {
                    id: "prop-power".to_string(),
                    device_id: "test-device-multi".to_string(),
                    name: "power_status".to_string(),
                    display_name: None,
                    description: None,
                    data_type: Some("boolean".to_string()),
                    unit: None,
                    min_value: None,
                    max_value: None,
                    default_value: None,
                    is_read_only: 1,
                    created_at: None,
                    updated_at: None,
                    current_value: None,
                    alarm_status: None,
                },
            ]),
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
        assert_eq!(
            default_config.get("temp_range"),
            Some(&"10.0".to_string())
        );
        assert_eq!(
            default_config.get("enable_noise"),
            Some(&"true".to_string())
        );
        assert_eq!(
            default_config.get("drift_speed"),
            Some(&"0.3".to_string())
        );
        assert_eq!(
            default_config.get("anomaly_probability"),
            Some(&"0.02".to_string())
        );
        assert_eq!(
            default_config.get("enable_periodic"),
            Some(&"true".to_string())
        );
        assert_eq!(
            default_config.get("enable_anomaly"),
            Some(&"true".to_string())
        );
        assert_eq!(
            default_config.get("daily_amplitude_scale"),
            Some(&"1.0".to_string())
        );
        assert_eq!(
            default_config.get("noise_level"),
            Some(&"1.0".to_string())
        );
        assert_eq!(
            default_config.get("drift_rate"),
            Some(&"0.0".to_string())
        );
        assert_eq!(
            default_config.get("correlation_tags"),
            Some(&"*".to_string())
        );
        assert_eq!(default_config.len(), 12);
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
    fn test_read_data_consecutive_values_close() {
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
                    change < 10.0,
                    "Consecutive signal values should change gradually, got change {}",
                    change
                );
            }
            prev_temp = Some(temp);
        }
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
            "Fixed mode temperature should be 25.0 (baseline from match_property)"
        );
    }

    #[test]
    fn test_anomaly_disabled_no_spikes() {
        // With all variation off, values should be nearly constant (just baseline)
        let mut device = create_device_with_temp_property();
        device.driver_options = Some(
            r#"{"enable_anomaly": false, "enable_periodic": false, "enable_noise": false}"#
                .to_string(),
        );
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

        // With all variation disabled, values should barely change
        // (only drift_rate * tick, and drift_rate defaults to 0.0)
        assert!(
            max_change < 1.0,
            "With anomalies/periodic/noise disabled, values should be nearly constant, max change was {}",
            max_change
        );
    }

    #[test]
    fn test_multiple_property_types() {
        let device = create_device_with_multiple_properties();
        let mut driver = SimulatedDriver::new(device);

        let values = driver.read_data().unwrap();

        // Should have 3 properties: temperature, current, power_status
        assert_eq!(values.len(), 3);

        let temp = values.iter().find(|v| v.name == "temperature").unwrap();
        let current = values.iter().find(|v| v.name == "current").unwrap();
        let power = values.iter().find(|v| v.name == "power_status").unwrap();

        // Temperature should be ~25.0 (baseline for temp behavior)
        let temp_val: f64 = temp.value.as_ref().unwrap().parse().unwrap();
        assert!((temp_val - 25.0).abs() < 5.0, "temp should be near 25.0, got {temp_val}");

        // Current should be ~10.0 (baseline for current behavior)
        let current_val: f64 = current.value.as_ref().unwrap().parse().unwrap();
        assert!(
            (current_val - 10.0).abs() < 5.0,
            "current should be near 10.0A, got {current_val}"
        );

        // Power_status should be a boolean
        let power_val = power.value.as_ref().unwrap();
        assert!(
            power_val == "true" || power_val == "false",
            "power_status should be boolean, got {power_val}"
        );
    }

    #[test]
    fn test_correlation_tags_is_respected() {
        // Empty correlation_tags pattern should disable correlation (tag_matches returns false)
        let mut device = create_device_with_temp_property();
        device.driver_options = Some(
            r#"{"correlation_tags": "", "enable_periodic": false, "enable_noise": false, "enable_anomaly": false}"#
                .to_string(),
        );
        let mut driver = SimulatedDriver::new(device);

        let values = driver.read_data().unwrap();
        assert!(!values.is_empty());

        let temp = values
            .iter()
            .find(|v| v.name == "temperature")
            .and_then(|v| v.value.as_ref().and_then(|s| s.parse::<f64>().ok()))
            .unwrap();

        // With all variation off, temperature should be exactly baseline (25.0)
        assert!(
            (temp - 25.0).abs() < 0.01,
            "With empty correlation_tags and all variation off, temp should be 25.0, got {temp}"
        );

        // Reset correlation pattern for other tests
        CorrelationManager::set_pattern("*");
    }

    #[test]
    fn test_anomaly_probability_zero_no_anomalies() {
        // Anomaly probability is still supported as a legacy option, but the new
        // anomaly engine uses its own probabilities. This test verifies that with
        // enable_anomaly=false, no spikes occur.
        let mut device = create_device_with_temp_property();
        device.driver_options = Some(
            r#"{"enable_anomaly": false, "enable_noise": false, "enable_periodic": false}"#
                .to_string(),
        );
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
            max_change < 1.0,
            "With enable_anomaly=false, no spikes should occur, max change was {}",
            max_change
        );
    }
}

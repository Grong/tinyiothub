use std::collections::HashMap;
use std::time::Instant;
use tinyiothub_core::models::{thing::Thing, thing_command::ThingCommand};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tinyiothub_core::driver::{BackoffStrategy, CancelToken, ResultValue, RetryConfig, ThingDriver};
use tinyiothub_core::error::Error;
use tinyiothub_core::media::{DataKind, MediaPayload, SourceItem};

use super::simulated::anomaly::AnomalyEngine;
use super::simulated::correlation::{self, CorrelationManager, EnvironmentContext};
use super::simulated::patterns::match_property;
use super::simulated::signal::SignalComposer;
use super::simulated::world;

#[derive(Debug, Clone, tinyiothub_macros::ThingDriver)]
#[driver(
    name = "simulator",
    version = "2.0.0",
    description = "Simulated Thing Driver for Testing"
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
#[driver_option(
    label = "Story Mode (workspace-wide; auto/on/off)",
    name = "story",
    default = "auto",
    option_type = "string",
    required = false
)]
#[driver_option(
    label = "Clock Speed (workspace-wide; virtual seconds per real second; 0 = auto)",
    name = "clock_speed",
    default = "0",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Virtual Clock Start (workspace-wide; HH:MM or RFC3339)",
    name = "clock_start",
    default = "",
    option_type = "string",
    required = false
)]
#[driver_option(
    label = "Random Seed (0 = derive from thing id)",
    name = "seed",
    default = "0",
    option_type = "number",
    required = false
)]
#[driver_option(
    label = "Physical Role Override (environment, cold_room, lighting, power_lighting, water_zone, ...)",
    name = "role",
    default = "",
    option_type = "string",
    required = false
)]
#[driver_option(
    label = "Plant Zone Override (workshop, warehouse, corridor, outdoor, entrance)",
    name = "zone",
    default = "",
    option_type = "string",
    required = false
)]
pub struct SimulatedDriver {
    pub device: Thing,
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
    /// Workspace world key (None = legacy per-device telemetry).
    world_key: Option<String>,
    /// Force the next read to bypass the interval cache (after a command).
    force_refresh: bool,
    /// Monotonic sequence for snapshot/source reads.
    source_seq: u64,
}

impl SimulatedDriver {
    pub fn new(device: Thing) -> Self {
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
            world_key: None,
            force_refresh: false,
            source_seq: 0,
        };

        // Set correlation pattern from config
        let correlation_tags = driver.get_config_string("correlation_tags", "*");
        CorrelationManager::set_pattern(&correlation_tags);

        // Build group context from device tags
        let tags = driver.device.tags.clone().unwrap_or_default();
        let contexts = correlation::get_contexts_for_device(&tags, &mut driver.rng);
        driver.group_context = Some(correlation::merge_contexts(&contexts));

        let mode = driver.get_config_string("mode", "random");
        let has_properties = driver
            .device
            .properties
            .as_ref()
            .is_some_and(|properties| !properties.is_empty());
        if mode != "fixed" && driver.device.workspace_id.is_some() {
            if has_properties {
                driver.world_key = world::register(&driver.device);
            } else {
                tracing::warn!(
                    thing_id = %driver.device.id,
                    "simulator thing has no properties; keeping legacy telemetry"
                );
            }
        }

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

impl ThingDriver for SimulatedDriver {
    fn thing(&self) -> &Thing {
        &self.device
    }

    fn thing_mut(&mut self) -> &mut Thing {
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

    fn describe(&mut self) -> Result<tinyiothub_core::driver::declaration::DeviceDeclaration, Error> {
        Ok(crate::driver::describe::declaration_from_thing(
            &self.device,
            "simulator",
            "2.0.0",
        ))
    }

    #[allow(clippy::collapsible_if)]
    fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        // Respect the configured interval — skip regeneration if not enough time has passed
        let interval_ms = self.get_config_number("interval", 1000.0) as u64;
        let elapsed = self.last_read.elapsed().as_millis() as u64;
        if !self.force_refresh && elapsed < interval_ms {
            if let Some(ref cached) = self.cached_values {
                return Ok(cached.clone());
            }
        }

        if let Some(key) = self.world_key.as_deref() {
            if let Some(values) = world::read(key, &self.device.id) {
                self.tick_counter = self.tick_counter.wrapping_add(1);
                self.last_read = Instant::now();
                self.force_refresh = false;
                self.cached_values = Some(values.clone());
                return Ok(values);
            }
        }

        self.tick_counter = self.tick_counter.wrapping_add(1);
        self.force_refresh = false;
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
                // 属性间检查点：stop 后长属性列表尽快协作退出（E4c）。
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let data_type = prop_data_type.as_deref().unwrap_or("float");
                let behavior = match_property(prop_name, data_type);

                let result_value = if simulation_mode == "fixed" {
                    // Fixed mode: return baseline value for all properties
                    if behavior.discrete {
                        ResultValue::boolean(prop_name.clone(), true)
                    } else {
                        ResultValue::float_with_precision(prop_name.clone(), behavior.baseline, 2)
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

    fn read_sources(&mut self, sources: &[String], cancel: &CancelToken) -> Result<Vec<SourceItem>, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        if !is_camera(&self.device) {
            return Err(Error::Unsupported(
                "simulated driver only exposes snapshot sources for cameras".into(),
            ));
        }
        let (label, bytes) = camera_image(&self.device);
        let mut items = Vec::with_capacity(sources.len());
        for source in sources {
            self.source_seq = self.source_seq.wrapping_add(1);
            items.push(SourceItem {
                v: 1,
                source: source.clone(),
                data_kind: DataKind::Image,
                seq: self.source_seq,
                time: chrono::Utc::now(),
                text: Some(format!("模拟监控画面：{label}")),
                data: serde_json::json!({"camera": label, "snapshot": true}),
                media: Some(MediaPayload {
                    mime: "image/jpeg".into(),
                    bytes: bytes.to_vec(),
                }),
                error: None,
            });
        }
        Ok(items)
    }

    fn execute_command(&mut self, cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(key) = self.world_key.as_deref()
            && world::command(key, &self.device.id, &cmd.name, cmd.parameters.as_deref())
        {
            self.force_refresh = true;
            self.cached_values = None;
            tracing::info!(
                "Executing simulated command: {} on thing {} (world state updated)",
                cmd.name,
                self.device.name
            );
            return Ok(true);
        }
        tracing::info!(
            "Executing Simulated command: {} on device {}",
            cmd.name,
            self.device.name
        );
        Ok(true)
    }
}

fn is_camera(device: &Thing) -> bool {
    let text = format!(
        "{} {} {}",
        device.id,
        device.name,
        device.display_name.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    device.category.as_deref().is_some_and(|category| {
        category.contains("摄像")
            || category.contains("视频")
            || category.to_ascii_lowercase().contains("camera")
            || category.to_ascii_lowercase().contains("video")
    }) || text.contains("camera")
        || text.contains("video")
        || text.contains("摄像")
        || text.contains("视频")
}

fn camera_image(device: &Thing) -> (&'static str, &'static [u8]) {
    const ENTRANCE: &[u8] = include_bytes!("simulated/assets/camera/entrance.jpg");
    const WORKSHOP: &[u8] = include_bytes!("simulated/assets/camera/workshop.jpg");
    const WAREHOUSE: &[u8] = include_bytes!("simulated/assets/camera/warehouse.jpg");

    let text = format!(
        "{} {} {}",
        device.id,
        device.name,
        device.display_name.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    if text.contains("entrance") || text.contains("入口") {
        ("工厂入口", ENTRANCE)
    } else if text.contains("warehouse") || text.contains("仓库") {
        ("B栋仓库", WAREHOUSE)
    } else {
        ("车间产线", WORKSHOP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> Thing {
        Thing {
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

    fn create_device_with_temp_property() -> Thing {
        use tinyiothub_core::models::thing_property::ThingProperty;
        Thing {
            id: "test-device-temp".to_string(),
            name: "Temp Thing".to_string(),
            display_name: None,
            driver_name: Some("SimulatedDriver".to_string()),
            driver_options: None,
            protocol_type: Some("simulation".to_string()),
            properties: Some(vec![ThingProperty {
                id: "prop-temp".to_string(),
                thing_id: "test-device-temp".to_string(),
                name: "temperature".to_string(),
                display_name: None,
                description: None,
                data_type: Some("float".to_string()),
                unit: None,
                min_value: None,
                max_value: None,
                default_value: None,
                is_read_only: 1,
                source: None,
                expression: None,
                refs: None,
                created_at: None,
                updated_at: None,
                current_value: None,
                alarm_status: None,
                derived_status: None,
            }]),
            commands: None,
            created_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            updated_at: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            ..Default::default()
        }
    }

    fn create_device_with_multiple_properties() -> Thing {
        use tinyiothub_core::models::thing_property::ThingProperty;
        Thing {
            id: "test-device-multi".to_string(),
            name: "Multi Prop Thing".to_string(),
            display_name: None,
            driver_name: Some("SimulatedDriver".to_string()),
            driver_options: Some(
                r#"{"enable_anomaly": false, "enable_periodic": false, "enable_noise": false}"#.to_string(),
            ),
            protocol_type: Some("simulation".to_string()),
            properties: Some(vec![
                ThingProperty {
                    id: "prop-temp".to_string(),
                    thing_id: "test-device-multi".to_string(),
                    name: "temperature".to_string(),
                    display_name: None,
                    description: None,
                    data_type: Some("float".to_string()),
                    unit: None,
                    min_value: None,
                    max_value: None,
                    default_value: None,
                    is_read_only: 1,
                    source: None,
                    expression: None,
                    refs: None,
                    created_at: None,
                    updated_at: None,
                    current_value: None,
                    alarm_status: None,
                    derived_status: None,
                },
                ThingProperty {
                    id: "prop-current".to_string(),
                    thing_id: "test-device-multi".to_string(),
                    name: "current".to_string(),
                    display_name: None,
                    description: None,
                    data_type: Some("float".to_string()),
                    unit: None,
                    min_value: None,
                    max_value: None,
                    default_value: None,
                    is_read_only: 1,
                    source: None,
                    expression: None,
                    refs: None,
                    created_at: None,
                    updated_at: None,
                    current_value: None,
                    alarm_status: None,
                    derived_status: None,
                },
                ThingProperty {
                    id: "prop-power".to_string(),
                    thing_id: "test-device-multi".to_string(),
                    name: "power_status".to_string(),
                    display_name: None,
                    description: None,
                    data_type: Some("boolean".to_string()),
                    unit: None,
                    min_value: None,
                    max_value: None,
                    default_value: None,
                    is_read_only: 1,
                    source: None,
                    expression: None,
                    refs: None,
                    created_at: None,
                    updated_at: None,
                    current_value: None,
                    alarm_status: None,
                    derived_status: None,
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
        assert_eq!(default_config.get("temp_range"), Some(&"10.0".to_string()));
        assert_eq!(default_config.get("enable_noise"), Some(&"true".to_string()));
        assert_eq!(default_config.get("drift_speed"), Some(&"0.3".to_string()));
        assert_eq!(default_config.get("anomaly_probability"), Some(&"0.02".to_string()));
        assert_eq!(default_config.get("enable_periodic"), Some(&"true".to_string()));
        assert_eq!(default_config.get("enable_anomaly"), Some(&"true".to_string()));
        assert_eq!(default_config.get("daily_amplitude_scale"), Some(&"1.0".to_string()));
        assert_eq!(default_config.get("noise_level"), Some(&"1.0".to_string()));
        assert_eq!(default_config.get("drift_rate"), Some(&"0.0".to_string()));
        assert_eq!(default_config.get("correlation_tags"), Some(&"*".to_string()));
        assert_eq!(default_config.get("story"), Some(&"auto".to_string()));
        assert_eq!(default_config.get("clock_speed"), Some(&"0".to_string()));
        assert_eq!(default_config.get("clock_start"), Some(&"".to_string()));
        assert_eq!(default_config.get("seed"), Some(&"0".to_string()));
        assert_eq!(default_config.get("role"), Some(&"".to_string()));
        assert_eq!(default_config.get("zone"), Some(&"".to_string()));
        assert_eq!(default_config.len(), 18);
    }

    #[test]
    fn test_read_data_with_config() {
        let device = create_test_device();
        let mut driver = SimulatedDriver::new(device);
        let result = driver.read_data(&CancelToken::new());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_cancelled_token_short_circuits_read_and_command() {
        let mut driver = SimulatedDriver::new(create_test_device());
        let cancel = CancelToken::new();
        cancel.cancel();

        assert!(matches!(driver.read_data(&cancel), Err(Error::Cancelled)));

        let cmd = ThingCommand {
            id: "cmd-1".to_string(),
            thing_id: "test-device-1".to_string(),
            name: "noop".to_string(),
            display_name: None,
            description: None,
            parameters: None,
            created_at: String::new(),
        };
        assert!(matches!(driver.execute_command(&cmd, &cancel), Err(Error::Cancelled)));
    }

    #[test]
    fn camera_snapshot_returns_a_jpeg_source() {
        let mut device = create_test_device();
        device.id = "device-cam-01".into();
        device.name = "camera_entrance".into();
        device.display_name = Some("工厂入口摄像头".into());
        device.category = Some("网络摄像头".into());
        let mut driver = SimulatedDriver::new(device);
        let items = driver
            .read_sources(&["snapshot".to_string()], &CancelToken::new())
            .expect("camera source read");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].data_kind, DataKind::Image);
        assert_eq!(items[0].source, "snapshot");
        let media = items[0].media.as_ref().expect("snapshot media");
        assert_eq!(media.mime, "image/jpeg");
        assert_eq!(&media.bytes[..2], &[0xFF, 0xD8], "JPEG SOI marker");
        assert!(media.bytes.len() > 10_000);
        assert!(items[0].text.as_deref().unwrap_or_default().contains("模拟监控画面"));
    }

    #[test]
    fn non_camera_source_read_is_unsupported() {
        let mut driver = SimulatedDriver::new(create_test_device());
        let error = driver
            .read_sources(&["snapshot".to_string()], &CancelToken::new())
            .expect_err("non-camera must not serve snapshots");
        assert!(matches!(error, Error::Unsupported(_)));
    }

    #[test]
    fn test_read_data_consecutive_values_close() {
        let device = create_device_with_temp_property();
        let mut driver = SimulatedDriver::new(device);

        let mut prev_temp: Option<f64> = None;
        for _ in 0..20 {
            let values = driver.read_data(&CancelToken::new()).unwrap();
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

        let values1 = fixed_driver.read_data(&CancelToken::new()).unwrap();
        let values2 = fixed_driver.read_data(&CancelToken::new()).unwrap();

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
        device.driver_options =
            Some(r#"{"enable_anomaly": false, "enable_periodic": false, "enable_noise": false}"#.to_string());
        let mut driver = SimulatedDriver::new(device);

        let mut max_change: f64 = 0.0;
        let mut prev: Option<f64> = None;
        for _ in 0..50 {
            let values = driver.read_data(&CancelToken::new()).unwrap();
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

        let values = driver.read_data(&CancelToken::new()).unwrap();

        // Should have 3 properties: temperature, current, power_status
        assert_eq!(values.len(), 3);

        let temp = values.iter().find(|v| v.name == "temperature").unwrap();
        let current = values.iter().find(|v| v.name == "current").unwrap();
        let power = values.iter().find(|v| v.name == "power_status").unwrap();

        // Temperature should be ~25.0 (baseline for temp behavior)
        let temp_val: f64 = temp.value.as_ref().unwrap().parse().unwrap();
        assert!(
            (temp_val - 25.0).abs() < 5.0,
            "temp should be near 25.0, got {temp_val}"
        );

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

        let values = driver.read_data(&CancelToken::new()).unwrap();
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

    /// Serialize the tests that share the process-global simulated worlds.
    static WORLD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn create_lighting_device() -> Thing {
        use tinyiothub_core::models::thing_property::ThingProperty;

        let property = |name: &str, data_type: &str, unit: &str| ThingProperty {
            id: format!("prop-{name}"),
            thing_id: "test-light".to_string(),
            name: name.to_string(),
            display_name: None,
            description: None,
            data_type: Some(data_type.to_string()),
            unit: Some(unit.to_string()),
            min_value: None,
            max_value: None,
            default_value: None,
            is_read_only: 1,
            source: None,
            expression: None,
            refs: None,
            created_at: None,
            updated_at: None,
            current_value: None,
            alarm_status: None,
            derived_status: None,
        };

        let mut device = Thing {
            id: "test-light".to_string(),
            name: "lighting_workshop".to_string(),
            display_name: Some("车间照明".to_string()),
            category: Some("照明控制器".to_string()),
            workspace_id: Some("ws-simulator-test".to_string()),
            driver_name: Some("simulator".to_string()),
            driver_options: Some(r#"{"story":"off","clock_start":"2026-06-01T09:00:00","interval":"10"}"#.to_string()),
            properties: Some(vec![
                property("switch_on", "boolean", ""),
                property("brightness", "number", "%"),
                property("power", "number", "W"),
            ]),
            ..Default::default()
        };
        device
            .properties
            .as_mut()
            .expect("properties")
            .sort_by(|a, b| a.name.cmp(&b.name));
        device
    }

    #[test]
    fn test_world_command_updates_the_next_read() {
        let _guard = WORLD_TEST_LOCK.lock().expect("world test lock");
        world::reset_for_tests();
        let mut driver = SimulatedDriver::new(create_lighting_device());
        assert!(driver.world_key.is_some(), "workspace device should join a world");

        let before = driver.read_data(&CancelToken::new()).expect("first read");
        let switch = before.iter().find(|value| value.name == "switch_on").expect("switch");
        assert_eq!(switch.value.as_deref(), Some("true"));

        let cmd = ThingCommand {
            id: "effect:test-1".to_string(),
            thing_id: "test-light".to_string(),
            name: "turn_off".to_string(),
            display_name: None,
            description: None,
            parameters: Some("{}".to_string()),
            created_at: String::new(),
        };
        assert!(driver.execute_command(&cmd, &CancelToken::new()).expect("command"));

        let after = driver.read_data(&CancelToken::new()).expect("read after command");
        let switch = after.iter().find(|value| value.name == "switch_on").expect("switch");
        assert_eq!(switch.value.as_deref(), Some("false"));
        let power = after.iter().find(|value| value.name == "power").expect("power");
        assert_eq!(power.value.as_deref(), Some("0.00"));

        world::reset_for_tests();
    }

    #[test]
    fn test_fixed_mode_and_missing_workspace_keep_legacy_path() {
        let _guard = WORLD_TEST_LOCK.lock().expect("world test lock");
        world::reset_for_tests();

        let mut fixed = create_lighting_device();
        fixed.driver_options = Some(r#"{"mode":"fixed"}"#.to_string());
        let fixed_driver = SimulatedDriver::new(fixed);
        assert!(fixed_driver.world_key.is_none());

        let mut no_workspace = create_lighting_device();
        no_workspace.workspace_id = None;
        let no_workspace_driver = SimulatedDriver::new(no_workspace);
        assert!(no_workspace_driver.world_key.is_none());

        world::reset_for_tests();
    }

    #[test]
    fn test_anomaly_probability_zero_no_anomalies() {
        // Anomaly probability is still supported as a legacy option, but the new
        // anomaly engine uses its own probabilities. This test verifies that with
        // enable_anomaly=false, no spikes occur.
        let mut device = create_device_with_temp_property();
        device.driver_options =
            Some(r#"{"enable_anomaly": false, "enable_noise": false, "enable_periodic": false}"#.to_string());
        let mut driver = SimulatedDriver::new(device);

        let mut max_change: f64 = 0.0;
        let mut prev: Option<f64> = None;
        for _ in 0..50 {
            let values = driver.read_data(&CancelToken::new()).unwrap();
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

//! Shared plant simulation behind the simulator driver.
//!
//! Devices that belong to a workspace are registered into one world per
//! workspace. The world owns a virtual clock, a day/night + shift schedule, a
//! demo storyline, and the cross-device coupling that makes a simulated site
//! behave like a site:
//!
//! * lighting loads feed the lighting branch meter, which feeds the main meter;
//! * zone water flow feeds the main flow and drops the pipe pressure;
//! * a cold-room door opening warms the room above its alarm threshold;
//! * a smoke event raises both the smoke density and the heat detector.
//!
//! Commands mutate the device state, so the next read reflects the actuation
//! (turn a light on, read the branch power rise, read the main meter rise).
//!
//! Devices without a workspace, or in the legacy `mode=fixed`, keep the
//! original per-device telemetry generator. That keeps unit tests and the
//! fixed-value acceptance path unchanged.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;
use std::time::Instant;

use chrono::{DateTime, Datelike, Local, NaiveTime, TimeZone, Timelike};
use parking_lot::Mutex;
use rand::SeedableRng;
use serde_json::Value as JsonValue;
use tinyiothub_core::driver::ResultValue;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_property::ThingProperty;

use super::patterns::match_property;
use super::signal::SignalComposer;

// ── Public entry points ─────────────────────────────────────────────

static WORLDS: LazyLock<Mutex<HashMap<String, SimWorld>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register (or refresh) a device in its workspace world. Returns the world
/// key to use on later reads/commands. `None` means the legacy path.
pub fn register(thing: &Thing) -> Option<String> {
    let key = thing.workspace_id.clone()?;
    let config = WorldConfig::from_thing(thing);
    let role = config.role.unwrap_or_else(|| infer_role(thing));
    if config.role.is_none() && role == Role::Generic {
        // Devices without a recognizable physical role keep the original
        // per-property generator (anomalies, correlation, legacy options).
        return None;
    }
    let mut worlds = WORLDS.lock();
    let world = worlds.entry(key.clone()).or_insert_with(SimWorld::new);
    world.register(thing);
    Some(key)
}

/// Drop a deleted thing from every world. Called by `DataServer::remove_device`
/// so aggregates do not retain a thing whose worker is gone.
pub fn unregister(thing_id: &str) {
    let mut worlds = WORLDS.lock();
    worlds.retain(|_, world| {
        world.devices.remove(thing_id);
        !world.devices.is_empty()
    });
}

/// Read all declared properties of `thing_id` from its world.
pub fn read(key: &str, thing_id: &str) -> Option<Vec<ResultValue>> {
    let mut worlds = WORLDS.lock();
    worlds.get_mut(key)?.read(thing_id)
}

/// Apply a device command to the shared state. Returns false when the device
/// is not registered (caller falls back to the legacy no-op driver behavior).
pub fn command(key: &str, thing_id: &str, name: &str, parameters: Option<&str>) -> bool {
    let mut worlds = WORLDS.lock();
    worlds
        .get_mut(key)
        .is_some_and(|world| world.apply_command(thing_id, name, parameters))
}

/// Drop every world. Only used by tests that must not share process state.
#[cfg(test)]
pub fn reset_for_tests() {
    WORLDS.lock().clear();
}

// ── Roles and zones ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Environment,
    ColdRoom,
    PowerMeter(Branch),
    Lighting,
    WaterMeter { main: bool },
    Smoke,
    Heat,
    Access,
    Security,
    Robot,
    Gateway,
    Camera,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    Main,
    Lighting,
    Hvac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Workshop,
    Warehouse,
    ColdRoom,
    Corridor,
    Outdoor,
    Entrance,
    Other,
}

impl Role {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "environment" | "env" | "env_sensor" => Some(Self::Environment),
            "cold_room" | "cold" | "fridge" => Some(Self::ColdRoom),
            "power" | "power_meter" | "power_main" | "meter" => Some(Self::PowerMeter(Branch::Main)),
            "power_lighting" | "lighting_meter" => Some(Self::PowerMeter(Branch::Lighting)),
            "power_hvac" | "hvac_meter" => Some(Self::PowerMeter(Branch::Hvac)),
            "lighting" | "light" => Some(Self::Lighting),
            "water" | "water_meter" | "water_main" => Some(Self::WaterMeter { main: true }),
            "water_zone" | "water_zone_meter" => Some(Self::WaterMeter { main: false }),
            "smoke" | "smoke_detector" => Some(Self::Smoke),
            "heat" | "heat_detector" => Some(Self::Heat),
            "access" | "access_controller" => Some(Self::Access),
            "security" | "security_panel" => Some(Self::Security),
            "robot" => Some(Self::Robot),
            "gateway" => Some(Self::Gateway),
            "camera" => Some(Self::Camera),
            _ => None,
        }
    }
}

impl Zone {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "workshop" | "车间" => Some(Self::Workshop),
            "warehouse" | "仓库" => Some(Self::Warehouse),
            "cold_room" | "cold" | "冷链" => Some(Self::ColdRoom),
            "corridor" | "走廊" => Some(Self::Corridor),
            "outdoor" | "室外" => Some(Self::Outdoor),
            "entrance" | "入口" => Some(Self::Entrance),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

fn infer_role(thing: &Thing) -> Role {
    let props: Vec<String> = thing
        .properties
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|property| property.name.to_ascii_lowercase())
        .collect();
    let has = |needle: &str| props.iter().any(|name| name.contains(needle));
    let text = thing_text(thing);

    if has("smoke_density") {
        return Role::Smoke;
    }
    if has("pos_x") || has("torque") {
        return Role::Robot;
    }
    if has("flow_rate") && has("pressure") {
        let main = text.contains("main") || text.contains("总进水") || text.contains("总表");
        return Role::WaterMeter { main };
    }
    if has("active_power") {
        let branch = if text.contains("lighting") || text.contains("照明") {
            Branch::Lighting
        } else if text.contains("hvac") || text.contains("暖通") || text.contains("空调") {
            Branch::Hvac
        } else {
            Branch::Main
        };
        return Role::PowerMeter(branch);
    }
    if has("switch_on") && has("brightness") {
        return Role::Lighting;
    }
    if has("armed") && (has("zone_open") || has("signal_strength")) {
        return Role::Security;
    }
    if has("door_open") && has("locked") {
        return Role::Access;
    }
    if has("door_open") && has("temperature") {
        return Role::ColdRoom;
    }
    if has("temperature") && has("alarm") {
        return Role::Heat;
    }
    if has("cpu_usage") || has("connected_devices") || has("network_quality") {
        return Role::Gateway;
    }
    if has("motion_detected") || has("resolution") {
        return Role::Camera;
    }
    if has("temperature") && (has("humidity") || has("co2_level")) {
        return Role::Environment;
    }
    if text.contains("温度记录仪") || text.contains("cold") || text.contains("fridge") {
        return Role::ColdRoom;
    }
    Role::Generic
}

fn infer_zone(thing: &Thing, role: Role) -> Zone {
    let text = thing_text(thing);
    let zone = if text.contains("workshop") || text.contains("车间") {
        Zone::Workshop
    } else if text.contains("warehouse") || text.contains("仓库") {
        Zone::Warehouse
    } else if text.contains("cold") || text.contains("fridge") || text.contains("冷链") || text.contains("冰箱") {
        Zone::ColdRoom
    } else if text.contains("corridor") || text.contains("走廊") {
        Zone::Corridor
    } else if text.contains("outdoor") || text.contains("室外") {
        Zone::Outdoor
    } else if text.contains("entrance") || text.contains("入口") || text.contains("正门") {
        Zone::Entrance
    } else {
        Zone::Other
    };
    if zone == Zone::Other && matches!(role, Role::Environment) {
        Zone::Workshop
    } else {
        zone
    }
}

fn thing_text(thing: &Thing) -> String {
    format!(
        "{} {} {} {}",
        thing.id,
        thing.name,
        thing.display_name.as_deref().unwrap_or_default(),
        thing.category.as_deref().unwrap_or_default(),
    )
    .to_ascii_lowercase()
}

// ── Configuration ───────────────────────────────────────────────────

struct WorldConfig {
    story: Option<bool>,
    speed: Option<f64>,
    start: Option<DateTime<Local>>,
    seed: u64,
    role: Option<Role>,
    zone: Option<Zone>,
    interval_ms: u64,
    enable_noise: bool,
    enable_periodic: bool,
    noise_level: f64,
    daily_amplitude_scale: f64,
    drift_rate: f64,
}

impl WorldConfig {
    fn from_thing(thing: &Thing) -> Self {
        let options: JsonValue = thing
            .driver_options
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or(JsonValue::Null);

        let story = option_str(&options, "story").and_then(|raw| match raw.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" | "factory" => Some(true),
            "off" | "false" | "0" | "none" => Some(false),
            _ => None,
        });
        let speed = option_f64(&options, "clock_speed")
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| value.clamp(1.0, MAX_CLOCK_SPEED));
        let start = option_str(&options, "clock_start").and_then(|raw| parse_clock_start(&raw));

        Self {
            story,
            speed,
            start,
            seed: option_f64(&options, "seed").map(|value| value as u64).unwrap_or(0),
            role: option_str(&options, "role").and_then(|raw| Role::parse(&raw)),
            zone: option_str(&options, "zone").and_then(|raw| Zone::parse(&raw)),
            interval_ms: option_f64(&options, "interval")
                .filter(|value| *value > 0.0)
                .map(|value| value as u64)
                .unwrap_or(1000),
            enable_noise: option_bool(&options, "enable_noise").unwrap_or(true),
            enable_periodic: option_bool(&options, "enable_periodic").unwrap_or(true),
            noise_level: option_f64(&options, "noise_level").unwrap_or(1.0),
            daily_amplitude_scale: option_f64(&options, "daily_amplitude_scale").unwrap_or(1.0),
            drift_rate: option_f64(&options, "drift_rate").unwrap_or(0.0),
        }
    }
}

fn option_value<'a>(options: &'a JsonValue, key: &str) -> Option<&'a JsonValue> {
    options.get(key).filter(|value| !value.is_null())
}

fn option_str(options: &JsonValue, key: &str) -> Option<String> {
    option_value(options, key).and_then(|value| match value {
        JsonValue::String(text) => Some(text.clone()),
        JsonValue::Number(number) => Some(number.to_string()),
        JsonValue::Bool(flag) => Some(flag.to_string()),
        _ => None,
    })
}

fn option_f64(options: &JsonValue, key: &str) -> Option<f64> {
    option_value(options, key).and_then(|value| match value {
        JsonValue::Number(number) => number.as_f64(),
        JsonValue::String(text) => text.trim().parse().ok(),
        _ => None,
    })
}

fn option_bool(options: &JsonValue, key: &str) -> Option<bool> {
    option_value(options, key).and_then(|value| match value {
        JsonValue::Bool(flag) => Some(*flag),
        JsonValue::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

fn story_start() -> DateTime<Local> {
    let today = Local::now().date_naive();
    today
        .and_hms_opt(6, 0, 0)
        .and_then(|naive| Local.from_local_datetime(&naive).single())
        .unwrap_or_else(Local::now)
}

fn parse_clock_start(raw: &str) -> Option<DateTime<Local>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Local));
    }
    let today = Local::now().date_naive();
    if let Ok(time) = NaiveTime::parse_from_str(raw, "%H:%M") {
        return Local.from_local_datetime(&today.and_time(time)).single();
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S") {
        return Local.from_local_datetime(&naive).single();
    }
    None
}

// ── Device model ────────────────────────────────────────────────────

struct Prop {
    name: String,
    data_type: String,
    min: Option<f64>,
    max: Option<f64>,
}

impl Prop {
    fn from_thing_property(property: &ThingProperty) -> Self {
        Self {
            name: property.name.clone(),
            data_type: property.data_type.clone().unwrap_or_else(|| "float".to_string()),
            min: property.min_value,
            max: property.max_value,
        }
    }

    fn is_boolean(&self) -> bool {
        matches!(self.data_type.to_ascii_lowercase().as_str(), "bool" | "boolean")
    }

    fn is_string(&self) -> bool {
        matches!(self.data_type.to_ascii_lowercase().as_str(), "string" | "enum" | "text")
    }

    fn is_integer(&self) -> bool {
        matches!(self.data_type.to_ascii_lowercase().as_str(), "int" | "integer")
    }

    fn clamp(&self, value: f64) -> f64 {
        let value = self.min.map_or(value, |min| value.max(min));
        self.max.map_or(value, |max| value.min(max))
    }
}

#[derive(Default)]
struct DeviceState {
    manual_at: Option<Instant>,
    manual_switch: Option<bool>,
    manual_brightness: Option<f64>,
    manual_locked: Option<bool>,
    manual_armed: Option<bool>,
    manual_speed: Option<f64>,
    manual_run_status: Option<String>,
    manual_values: HashMap<String, f64>,
    cold_high: f64,
    cold_low: f64,
    energy_today: f64,
    total_consumption: f64,
    access_count: f64,
    runtime_hours: f64,
    last_counter_at: Option<f64>,
    counter_day: Option<i32>,
    prev_access_event: bool,
    robot_target: Option<(f64, f64, f64)>,
}

struct Device {
    last_seen: Instant,
    role: Role,
    zone: Zone,
    props: Vec<Prop>,
    seed: u64,
    interval_ms: u64,
    enable_noise: bool,
    enable_periodic: bool,
    noise_level: f64,
    daily_amplitude_scale: f64,
    drift_rate: f64,
    state: DeviceState,
}

impl Device {
    fn manual_active(&self) -> bool {
        self.state
            .manual_at
            .is_some_and(|at| at.elapsed() < MANUAL_OVERRIDE_HOLD)
    }
}

/// Real-time hold for a manual command. Virtual time would make a 6-hour
/// override expire after three real minutes at the demo's 120x clock.
const MANUAL_OVERRIDE_HOLD: std::time::Duration = std::time::Duration::from_secs(15 * 60);
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(3600);
const DEFAULT_STORY_SPEED: f64 = 120.0;
const MAX_CLOCK_SPEED: f64 = 3600.0;
const MAX_CLOCK_OFFSET_MS: f64 = 315_360_000_000.0;
const COLD_DOOR_OPEN_HOUR: f64 = 10.5;
const COLD_DOOR_CLOSE_HOUR: f64 = 10.75;

// ── World ───────────────────────────────────────────────────────────

pub struct SimWorld {
    story_override: Option<bool>,
    story_on: Option<bool>,
    speed_override: Option<f64>,
    speed: Option<f64>,
    start_override: Option<DateTime<Local>>,
    start_at: Option<DateTime<Local>>,
    /// Whether the cached anchor was created while the storyline was active.
    anchored_story: Option<bool>,
    started: Instant,
    devices: BTreeMap<String, Device>,
}

impl SimWorld {
    fn new() -> Self {
        Self {
            story_override: None,
            story_on: None,
            speed_override: None,
            speed: None,
            start_override: None,
            start_at: None,
            anchored_story: None,
            started: Instant::now(),
            devices: BTreeMap::new(),
        }
    }

    fn register(&mut self, thing: &Thing) {
        let config = WorldConfig::from_thing(thing);
        if self.story_on.is_none()
            && self.story_override.is_none()
            && let Some(story) = config.story
        {
            self.story_override = Some(story);
        }
        if self.speed.is_none()
            && self.speed_override.is_none()
            && let Some(speed) = config.speed
        {
            self.speed_override = Some(speed);
        }
        if self.start_at.is_none()
            && self.start_override.is_none()
            && let Some(start) = config.start
        {
            self.start_override = Some(start);
        }

        let role = config.role.unwrap_or_else(|| infer_role(thing));
        let zone = config.zone.unwrap_or_else(|| infer_zone(thing, role));
        let props: Vec<Prop> = thing
            .properties
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(Prop::from_thing_property)
            .collect();
        let seed = if config.seed != 0 {
            config.seed
        } else {
            stable_hash(&thing.id)
        };

        if let Some(device) = self.devices.get_mut(&thing.id) {
            let role_changed = device.role != role;
            device.last_seen = Instant::now();
            device.role = role;
            device.zone = zone;
            device.props = props;
            device.seed = seed;
            device.interval_ms = config.interval_ms;
            device.enable_noise = config.enable_noise;
            device.enable_periodic = config.enable_periodic;
            device.noise_level = config.noise_level;
            device.daily_amplitude_scale = config.daily_amplitude_scale;
            device.drift_rate = config.drift_rate;
            if role_changed {
                device.state = DeviceState::new();
            }
            return;
        }

        self.devices.insert(
            thing.id.clone(),
            Device {
                last_seen: Instant::now(),
                role,
                zone,
                props,
                seed,
                interval_ms: config.interval_ms,
                enable_noise: config.enable_noise,
                enable_periodic: config.enable_periodic,
                noise_level: config.noise_level,
                daily_amplitude_scale: config.daily_amplitude_scale,
                drift_rate: config.drift_rate,
                state: DeviceState::new(),
            },
        );
    }

    fn read(&mut self, thing_id: &str) -> Option<Vec<ResultValue>> {
        let now = self.virtual_now();
        let story = self.resolved_story();
        let start = self.start_at?;
        let t = (now - start).num_milliseconds() as f64 / 1000.0;
        let conditions = Conditions::from_local(&now, story);
        let aggregates = self.aggregates(&conditions, t);

        let device = self.devices.get_mut(thing_id)?;
        device.last_seen = Instant::now();
        update_counters(device, &conditions, &aggregates, t);
        Some(values_for(device, &conditions, &aggregates, t))
    }

    fn apply_command(&mut self, thing_id: &str, name: &str, parameters: Option<&str>) -> bool {
        let Some(device) = self.devices.get_mut(thing_id) else {
            return false;
        };
        device.last_seen = Instant::now();
        let params: JsonValue = parameters
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or(JsonValue::Null);
        apply_command_to_device(device, name, &params);
        true
    }

    fn resolved_story(&mut self) -> bool {
        if let Some(story) = self.story_on {
            return story;
        }
        if let Some(story) = self.story_override {
            self.story_on = Some(story);
            return story;
        }
        let fresh = |device: &Device| device.last_seen.elapsed() < STALE_AFTER;
        let has_cast = self
            .devices
            .values()
            .any(|device| fresh(device) && device.role == Role::ColdRoom)
            && self
                .devices
                .values()
                .any(|device| fresh(device) && device.role == Role::Smoke)
            && self
                .devices
                .values()
                .any(|device| fresh(device) && device.role == Role::Lighting)
            && self
                .devices
                .values()
                .any(|device| fresh(device) && matches!(device.role, Role::WaterMeter { .. }));
        if has_cast {
            self.story_on = Some(true);
        }
        has_cast
    }

    /// Resolve (and, when needed, re-anchor) the virtual clock.
    ///
    /// The storyline can become active after the first read when a scene adds
    /// the remaining cast (cold room, smoke, lighting, water). A non-story
    /// anchor therefore must not be permanent: once the cast completes we
    /// re-anchor to 06:00 at the story speed instead of silently running the
    /// demo at real speed from "now" forever.
    fn anchor(&mut self) -> (DateTime<Local>, f64) {
        let story = self.resolved_story();
        let reanchor = self.start_at.is_some()
            && self.start_override.is_none()
            && self.speed_override.is_none()
            && story
            && self.anchored_story != Some(true);
        if reanchor {
            self.start_at = None;
            self.speed = None;
            self.started = Instant::now();
        }

        let speed = self.speed.unwrap_or_else(|| {
            self.speed_override
                .unwrap_or(if story { DEFAULT_STORY_SPEED } else { 1.0 })
        });
        self.speed = Some(speed);

        let start = match self.start_at {
            Some(start) => start,
            None => {
                let start = self
                    .start_override
                    .unwrap_or_else(|| if story { story_start() } else { Local::now() });
                self.started = Instant::now();
                self.start_at = Some(start);
                tracing::info!(
                    simulated_start = %start,
                    clock_speed = speed,
                    story,
                    devices = self.devices.len(),
                    "simulated plant world started"
                );
                start
            }
        };
        self.anchored_story = Some(story);
        (start, speed)
    }

    fn virtual_now(&mut self) -> DateTime<Local> {
        let (start, speed) = self.anchor();
        let elapsed_ms = self.started.elapsed().as_millis() as f64;
        // Ten virtual years are enough for any demo and keep chrono far from
        // its overflow panic even when a caller sets an absurd clock speed.
        let offset_ms = (elapsed_ms * speed).clamp(0.0, MAX_CLOCK_OFFSET_MS);
        let offset = chrono::Duration::milliseconds(offset_ms as i64);
        start.checked_add_signed(offset).unwrap_or(start)
    }

    fn aggregates(&self, conditions: &Conditions, t: f64) -> Aggregates {
        let mut lighting_w = 0.0;
        let mut device_count = 0;
        for device in self.devices.values() {
            if device.last_seen.elapsed() >= STALE_AFTER {
                continue;
            }
            device_count += 1;
            if device.role == Role::Lighting {
                lighting_w += lighting_power(device, conditions, t);
            }
        }
        let lighting_kw = lighting_w / 1000.0 + 0.15;
        let hvac_kw = hvac_load(conditions);
        let production_kw = production_load(conditions);
        let main_kw = lighting_kw + hvac_kw + production_kw + 0.8;

        let grid_voltage = 221.0 + 3.0 * (t / 7200.0).sin() + 0.3 * unit_noise(0x51A7, (t * 2.0) as i64, 11);
        let zone_flow =
            22.0 + if conditions.occupancy { 18.0 } else { 2.0 } + if conditions.water_leak { 55.0 } else { 0.0 };
        let main_flow = zone_flow + 7.0;
        let main_pressure = (0.46_f64 - 0.0035 * main_flow).max(0.05);
        let zone_pressure = (main_pressure - 0.02 - 0.0012 * zone_flow).max(0.03);

        Aggregates {
            lighting_kw,
            hvac_kw,
            main_kw,
            grid_voltage,
            zone_flow,
            main_flow,
            main_pressure,
            zone_pressure,
            device_count,
        }
    }
}

impl DeviceState {
    fn new() -> Self {
        Self {
            cold_high: 8.0,
            cold_low: -25.0,
            ..Default::default()
        }
    }
}

struct Aggregates {
    lighting_kw: f64,
    hvac_kw: f64,
    main_kw: f64,
    grid_voltage: f64,
    zone_flow: f64,
    main_flow: f64,
    main_pressure: f64,
    zone_pressure: f64,
    device_count: usize,
}

// ── Conditions / storyline ──────────────────────────────────────────

struct Conditions {
    hour: f64,
    day_index: i32,
    daylight: f64,
    outdoor_temp: f64,
    occupancy: bool,
    workshop_occupied: bool,
    cold_door_open: bool,
    smoke: f64,
    heat: f64,
    water_leak: bool,
    humidity_spike: f64,
    hvac_surge: f64,
    access_event: bool,
    intrusion: bool,
    armed: bool,
}

impl Conditions {
    fn from_local(now: &DateTime<Local>, story: bool) -> Self {
        let hour = now.hour() as f64 + now.minute() as f64 / 60.0 + now.second() as f64 / 3600.0;
        let daylight = sun_up(hour);
        let outdoor_temp = 17.0 + 9.0 * daylight + 2.0 * (std::f64::consts::TAU * (hour - 6.0) / 24.0).sin();
        let workshop_occupied = in_window(hour, 8.0, 12.0) || in_window(hour, 13.5, 18.0);
        let smoke = if story { smoke_level(hour) } else { 0.0 };

        Self {
            hour,
            day_index: now.date_naive().num_days_from_ce(),
            daylight,
            outdoor_temp,
            occupancy: workshop_occupied,
            workshop_occupied,
            cold_door_open: story && in_window(hour, COLD_DOOR_OPEN_HOUR, COLD_DOOR_CLOSE_HOUR),
            smoke,
            heat: smoke / 8.0,
            water_leak: story && in_window(hour, 16.0, 16.5),
            humidity_spike: if story && in_window(hour, 12.5, 13.2) {
                22.0
            } else {
                0.0
            },
            hvac_surge: if story && in_window(hour, 15.0, 16.0) { 1.0 } else { 0.0 },
            access_event: story && (in_window(hour, 8.2, 8.6) || in_window(hour, 18.1, 18.6)),
            intrusion: story && in_window(hour, 2.0, 2.5),
            armed: !(7.0..19.0).contains(&hour),
        }
    }
}

fn in_window(value: f64, start: f64, end: f64) -> bool {
    (start..end).contains(&value)
}

fn sun_up(hour: f64) -> f64 {
    if !(6.0..18.0).contains(&hour) {
        return 0.0;
    }
    ((std::f64::consts::PI * (hour - 6.0) / 12.0).sin()).max(0.0)
}

fn smoke_level(hour: f64) -> f64 {
    if in_window(hour, 14.0, 14.05) {
        8.0 * (hour - 14.0) / 0.05
    } else if in_window(hour, 14.05, 14.25) {
        8.0
    } else if in_window(hour, 14.25, 14.4) {
        8.0 * (1.0 - (hour - 14.25) / 0.15).max(0.0)
    } else {
        0.0
    }
}

fn cold_room_temp(hour: f64) -> f64 {
    // The door opens 10:30-10:45, warming the room; closing drives it back
    // toward the 4 °C setpoint. A short defrost cycle runs at 03:00.
    let mut temperature = 4.0;
    if hour >= COLD_DOOR_CLOSE_HOUR {
        let open_hours = COLD_DOOR_CLOSE_HOUR - COLD_DOOR_OPEN_HOUR;
        let peak = 6.5 * (1.0 - (-open_hours / 0.12_f64).exp());
        temperature += peak * (-(hour - COLD_DOOR_CLOSE_HOUR) / 0.35_f64).exp();
    } else if hour >= COLD_DOOR_OPEN_HOUR {
        temperature += 6.5 * (1.0 - (-(hour - COLD_DOOR_OPEN_HOUR) / 0.12_f64).exp());
    }
    if hour >= 3.17 {
        temperature += 1.6 * (-(hour - 3.17) / 0.4_f64).exp();
    } else if hour >= 3.0 {
        temperature += 1.6 * (1.0 - (-(hour - 3.0) / 0.1_f64).exp());
    }
    temperature
}

// ── Coupling ────────────────────────────────────────────────────────

fn lighting_power(device: &Device, conditions: &Conditions, t: f64) -> f64 {
    if !light_switch_on(device, conditions) {
        return 0.0;
    }
    let brightness = lighting_brightness(device);
    let base = match device.zone {
        Zone::Workshop => 1600.0,
        Zone::Corridor => 600.0,
        Zone::Outdoor => 900.0,
        Zone::Warehouse => 1800.0,
        _ => 700.0,
    };
    base * brightness / 100.0 * (1.0 + 0.02 * (t / 300.0).sin())
}

fn lighting_brightness(device: &Device) -> f64 {
    if device.manual_active()
        && let Some(brightness) = device.state.manual_brightness
    {
        return brightness.clamp(0.0, 100.0);
    }
    match device.zone {
        Zone::Workshop => 80.0,
        Zone::Corridor => 60.0,
        Zone::Outdoor => 70.0,
        _ => 70.0,
    }
}

fn light_switch_on(device: &Device, conditions: &Conditions) -> bool {
    let automatic = match device.zone {
        Zone::Workshop => conditions.workshop_occupied,
        Zone::Corridor => conditions.occupancy || conditions.daylight < 0.2,
        Zone::Outdoor => conditions.daylight < 0.15,
        _ => conditions.occupancy,
    };
    if device.manual_active() {
        return device.state.manual_switch.unwrap_or(automatic);
    }
    automatic
}

fn hvac_load(conditions: &Conditions) -> f64 {
    let occupancy = if conditions.occupancy { 1.0 } else { 0.4 };
    9.0 + 6.0 * occupancy + (conditions.outdoor_temp - 26.0).max(0.0) * 0.6 + conditions.hvac_surge * 8.0
}

fn production_load(conditions: &Conditions) -> f64 {
    let base = if conditions.workshop_occupied { 6.5 } else { 0.8 };
    base + if conditions.occupancy { 0.4 } else { 0.0 }
}

// ── Values ──────────────────────────────────────────────────────────

enum Raw {
    F(f64),
    B(bool),
    S(String),
    I(i64),
    Missing,
}

fn values_for(device: &Device, conditions: &Conditions, aggregates: &Aggregates, t: f64) -> Vec<ResultValue> {
    device
        .props
        .iter()
        .map(|prop| make_value(prop, raw_value(device, prop, conditions, aggregates, t)))
        .collect()
}

fn raw_value(device: &Device, prop: &Prop, conditions: &Conditions, aggregates: &Aggregates, t: f64) -> Raw {
    let name = prop.name.to_ascii_lowercase();
    let role_value = match device.role {
        Role::Environment => environment_value(device, &name, conditions, t),
        Role::ColdRoom => cold_room_value(device, &name, conditions, t),
        Role::PowerMeter(branch) => power_meter_value(device, branch, &name, aggregates, t),
        Role::Lighting => lighting_value(device, &name, conditions, t),
        Role::WaterMeter { main } => water_value(device, main, &name, aggregates, t),
        Role::Smoke => smoke_value(device, &name, conditions, t),
        Role::Heat => heat_value(device, &name, conditions, t),
        Role::Access => access_value(device, &name, conditions),
        Role::Security => security_value(device, &name, conditions, t),
        Role::Robot => robot_value(device, &name, conditions, t),
        Role::Gateway => gateway_value(device, &name, conditions, aggregates, t),
        Role::Camera => camera_value(device, &name, conditions, t),
        Role::Generic => Raw::Missing,
    };
    if matches!(role_value, Raw::Missing) {
        generic_value(device, prop, t)
    } else {
        role_value
    }
}

fn make_value(prop: &Prop, raw: Raw) -> ResultValue {
    if prop.is_string() {
        let text = match raw {
            Raw::S(text) => text,
            Raw::B(flag) => {
                if flag {
                    "on".to_string()
                } else {
                    "off".to_string()
                }
            }
            Raw::F(value) => format!("{value:.2}"),
            Raw::I(value) => value.to_string(),
            Raw::Missing => "normal".to_string(),
        };
        return ResultValue::string(prop.name.clone(), text);
    }
    if prop.is_boolean() {
        let flag = match raw {
            Raw::B(flag) => flag,
            Raw::F(value) => value != 0.0,
            Raw::I(value) => value != 0,
            Raw::S(_) | Raw::Missing => false,
        };
        return ResultValue::boolean(prop.name.clone(), flag);
    }
    let value = match raw {
        Raw::F(value) => value,
        Raw::I(value) => value as f64,
        Raw::B(flag) => {
            if flag {
                1.0
            } else {
                0.0
            }
        }
        Raw::S(text) => text.parse().unwrap_or(0.0),
        Raw::Missing => 0.0,
    };
    let value = prop.clamp(value);
    if prop.is_integer() {
        ResultValue::integer(prop.name.clone(), value.round() as i64)
    } else {
        ResultValue::float_with_precision(prop.name.clone(), value, 2)
    }
}

fn environment_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    if name.contains("temp") {
        return Raw::F(zone_temperature(device.zone, conditions) + noise(device, name, 0.12, t));
    }
    if name.contains("humid") {
        return Raw::F(zone_humidity(device.zone, conditions) + noise(device, name, 0.4, t));
    }
    if name.contains("pressure") {
        return Raw::F(1013.0 + 2.0 * (t / 3600.0).sin() + noise(device, name, 0.15, t));
    }
    if name.contains("co2") {
        let base = if conditions.occupancy { 780.0 } else { 430.0 };
        return Raw::F(base + noise(device, name, 8.0, t));
    }
    if name.contains("battery") {
        return Raw::F(96.0 - t / 86_400.0 * 0.4);
    }
    Raw::Missing
}

fn zone_temperature(zone: Zone, conditions: &Conditions) -> f64 {
    match zone {
        Zone::Workshop => 24.0 + 2.0 * if conditions.occupancy { 1.0 } else { 0.0 } + 3.0 * (conditions.daylight - 0.4),
        Zone::Warehouse => 11.0 + 0.5 * conditions.outdoor_temp,
        Zone::ColdRoom => cold_room_temp(conditions.hour),
        Zone::Corridor => 24.0 + 2.0 * conditions.daylight,
        Zone::Entrance => 24.0 + 1.0 * conditions.daylight,
        Zone::Outdoor | Zone::Other => conditions.outdoor_temp,
    }
}

fn zone_humidity(zone: Zone, conditions: &Conditions) -> f64 {
    match zone {
        Zone::Workshop => 55.0 - 8.0 * conditions.daylight + if conditions.occupancy { 6.0 } else { 0.0 },
        Zone::Warehouse => 66.0 + 8.0 * (1.0 - conditions.daylight) + conditions.humidity_spike,
        Zone::ColdRoom => 62.0 + if conditions.cold_door_open { 6.0 } else { 0.0 },
        _ => 55.0,
    }
}

fn cold_room_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    if name.contains("temp") {
        return Raw::F(cold_room_temp(conditions.hour) + noise(device, name, 0.08, t));
    }
    if name.contains("door") {
        return Raw::B(conditions.cold_door_open);
    }
    if name.contains("humid") {
        return Raw::F(zone_humidity(Zone::ColdRoom, conditions) + noise(device, name, 0.3, t));
    }
    if name.contains("runtime") {
        return Raw::F(device.state.runtime_hours);
    }
    if name.contains("alarm") {
        let temperature = cold_room_temp(conditions.hour);
        return Raw::B(temperature > device.state.cold_high || temperature < device.state.cold_low);
    }
    Raw::Missing
}

fn power_meter_value(device: &Device, branch: Branch, name: &str, aggregates: &Aggregates, t: f64) -> Raw {
    let load_kw = match branch {
        Branch::Main => aggregates.main_kw,
        Branch::Lighting => aggregates.lighting_kw,
        Branch::Hvac => aggregates.hvac_kw,
    };
    if name.contains("voltage") {
        return Raw::F(aggregates.grid_voltage + noise(device, name, 0.4, t));
    }
    if name.contains("current") {
        let voltage = aggregates.grid_voltage.max(1.0);
        return Raw::F(load_kw * 1000.0 / (voltage * 0.9) + noise(device, name, 0.2, t));
    }
    if name.contains("active_power") || name == "power" {
        return Raw::F(load_kw + noise(device, name, 0.02, t));
    }
    if name.contains("energy") {
        return Raw::F(device.state.energy_today);
    }
    if name.contains("power_factor") || name.contains("factor") {
        let load = (load_kw / 30.0).clamp(0.0, 1.0);
        return Raw::F(0.93 - 0.05 * load + noise(device, name, 0.002, t));
    }
    if name.contains("frequency") {
        return Raw::F(50.0 + 0.03 * (t / 1800.0).sin() + noise(device, name, 0.01, t));
    }
    Raw::Missing
}

fn lighting_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    let on = light_switch_on(device, conditions);
    if name.contains("switch") {
        return Raw::B(on);
    }
    if name.contains("brightness") {
        return Raw::F(lighting_brightness(device));
    }
    if name.contains("power") && !name.contains("energy") {
        return Raw::F(lighting_power(device, conditions, t));
    }
    if name.contains("energy") {
        return Raw::F(device.state.energy_today);
    }
    if name.contains("presence") {
        return Raw::B(zone_presence(device.zone, conditions));
    }
    Raw::Missing
}

fn zone_presence(zone: Zone, conditions: &Conditions) -> bool {
    match zone {
        Zone::Workshop => conditions.workshop_occupied,
        Zone::Corridor => conditions.occupancy || conditions.access_event,
        Zone::Outdoor => conditions.access_event,
        _ => conditions.occupancy,
    }
}

fn water_value(device: &Device, main: bool, name: &str, aggregates: &Aggregates, t: f64) -> Raw {
    if name.contains("flow") {
        let flow = if main {
            aggregates.main_flow
        } else {
            aggregates.zone_flow
        };
        return Raw::F(flow + noise(device, name, 0.3, t));
    }
    if name.contains("total") || name.contains("consumption") {
        return Raw::F(device.state.total_consumption);
    }
    if name.contains("pressure") {
        let pressure = if main {
            aggregates.main_pressure
        } else {
            aggregates.zone_pressure
        };
        return Raw::F(pressure + noise(device, name, 0.004, t));
    }
    if name.contains("battery") {
        return Raw::F(92.0 - t / 86_400.0 * 0.3);
    }
    if name.contains("leak") {
        return Raw::B(aggregates.zone_flow > 70.0);
    }
    Raw::Missing
}

fn smoke_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    if name.contains("density") {
        return Raw::F(conditions.smoke + noise(device, name, 0.05, t));
    }
    if name.contains("alarm") {
        return Raw::B(conditions.smoke > 5.0);
    }
    if name.contains("battery") {
        return Raw::F(90.0 - t / 86_400.0 * 0.2);
    }
    if name.contains("fault") {
        return Raw::B(false);
    }
    Raw::Missing
}

fn heat_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    if name.contains("temp") {
        return Raw::F(26.0 + conditions.heat * 45.0 + noise(device, name, 0.2, t));
    }
    if name.contains("alarm") {
        return Raw::B(26.0 + conditions.heat * 45.0 > 70.0);
    }
    if name.contains("battery") {
        return Raw::F(91.0 - t / 86_400.0 * 0.2);
    }
    if name.contains("fault") {
        return Raw::B(false);
    }
    Raw::Missing
}

fn access_value(device: &Device, name: &str, conditions: &Conditions) -> Raw {
    let locked = if device.manual_active() {
        device.state.manual_locked.unwrap_or(true)
    } else {
        !conditions.access_event
    };
    if name.contains("door") {
        return Raw::B(conditions.access_event || !locked);
    }
    if name.contains("locked") {
        return Raw::B(locked);
    }
    if name.contains("count") {
        return Raw::I(device.state.access_count as i64);
    }
    if name.contains("tamper") {
        return Raw::B(false);
    }
    if name.contains("result") {
        let text = if conditions.access_event || !locked {
            "granted"
        } else {
            "idle"
        };
        return Raw::S(text.to_string());
    }
    Raw::Missing
}

fn security_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    let armed = if device.manual_active() {
        device.state.manual_armed.unwrap_or(conditions.armed)
    } else {
        conditions.armed
    };
    if name.contains("armed") {
        return Raw::B(armed);
    }
    if name.contains("alarm") {
        return Raw::B(conditions.intrusion && armed);
    }
    if name.contains("zone") {
        return Raw::B(conditions.intrusion);
    }
    if name.contains("battery") {
        return Raw::F(94.0 - t / 86_400.0 * 0.2);
    }
    if name.contains("signal") {
        return Raw::F(82.0 + noise(device, name, 2.0, t));
    }
    Raw::Missing
}

fn robot_value(device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    let speed = robot_speed(device, conditions);
    let status = robot_status(device, conditions);
    let running = status == "running" && speed > 0.0;
    if name.contains("pos_x") || name == "x" {
        return Raw::F(robot_position(device, conditions, t, running).0);
    }
    if name.contains("pos_y") || name == "y" {
        return Raw::F(robot_position(device, conditions, t, running).1);
    }
    if name.contains("pos_z") || name == "z" {
        return Raw::F(robot_position(device, conditions, t, running).2);
    }
    if name.contains("torque") {
        let torque = if running { 25.0 + speed * 0.6 } else { 1.5 };
        return Raw::F(torque + noise(device, name, 1.0, t));
    }
    if name.contains("speed") {
        return Raw::F(speed);
    }
    if name.contains("status") {
        return Raw::S(status.to_string());
    }
    Raw::Missing
}

fn robot_status(device: &Device, conditions: &Conditions) -> &'static str {
    if device.manual_active()
        && let Some(status) = &device.state.manual_run_status
    {
        return match status.as_str() {
            "running" => "running",
            "idle" => "idle",
            "stopped" => "stopped",
            _ => "idle",
        };
    }
    if conditions.workshop_occupied {
        "running"
    } else {
        "idle"
    }
}

fn robot_speed(device: &Device, conditions: &Conditions) -> f64 {
    if device.manual_active()
        && let Some(speed) = device.state.manual_speed
    {
        return speed.clamp(0.0, 100.0);
    }
    if conditions.workshop_occupied { 65.0 } else { 0.0 }
}

fn robot_position(device: &Device, conditions: &Conditions, t: f64, running: bool) -> (f64, f64, f64) {
    // Per-device phase keeps two robots on the same schedule from moving in
    // lockstep.
    let t = t + (device.seed % 360) as f64;
    if let Some((x, y, z)) = device.state.robot_target {
        let wobble = if running { 4.0 } else { 0.0 };
        return (
            x + wobble * (t / 90.0).sin(),
            y + wobble * (t / 120.0).cos(),
            z + wobble * 0.5 * (t / 70.0).sin(),
        );
    }
    if running && conditions.workshop_occupied {
        (
            120.0 * (t / 180.0).sin(),
            80.0 * (t / 240.0).cos(),
            200.0 + 50.0 * (t / 300.0).sin(),
        )
    } else {
        (0.0, 0.0, 200.0)
    }
}

fn gateway_value(device: &Device, name: &str, conditions: &Conditions, aggregates: &Aggregates, t: f64) -> Raw {
    let count = aggregates.device_count as f64;
    if name.contains("cpu") {
        let load = if conditions.occupancy { 6.0 } else { 1.0 };
        return Raw::F((16.0 + count * 1.8 + load + noise(device, name, 2.0, t)).clamp(0.0, 100.0));
    }
    if name.contains("memory") {
        return Raw::F((36.0 + count * 1.2 + noise(device, name, 1.0, t)).clamp(0.0, 100.0));
    }
    if name.contains("connected") {
        return Raw::I(aggregates.device_count as i64);
    }
    if name.contains("uptime") {
        return Raw::F(100.0 + t / 3600.0);
    }
    if name.contains("network") {
        return Raw::F(96.0 + noise(device, name, 1.5, t));
    }
    Raw::Missing
}

fn camera_value(_device: &Device, name: &str, conditions: &Conditions, t: f64) -> Raw {
    if name.contains("power") || name == "status" {
        return Raw::B(true);
    }
    if name.contains("resolution") {
        return Raw::S("1920x1080".to_string());
    }
    if name.contains("motion") {
        return Raw::B(conditions.access_event || conditions.intrusion);
    }
    if name.contains("storage") {
        return Raw::F((40.0 + t / 86_400.0 * 1.5).clamp(0.0, 100.0));
    }
    Raw::Missing
}

fn generic_value(device: &Device, prop: &Prop, t: f64) -> Raw {
    if let Some(value) = device.state.manual_values.get(&prop.name.to_ascii_lowercase()) {
        return Raw::F(*value);
    }
    if prop.is_string() {
        return Raw::S("normal".to_string());
    }
    let behavior = match_property(&prop.name, &prop.data_type);
    if behavior.discrete {
        let slot = (t / 60.0) as i64;
        return Raw::B(unit_noise(device.seed, slot, stable_hash(&prop.name)) > -0.35);
    }
    if behavior.cumulative {
        return Raw::F(behavior.baseline + t / 60.0 * 0.1);
    }
    let composer = SignalComposer {
        interval_ms: device.interval_ms,
        daily_amplitude_scale: device.daily_amplitude_scale,
        noise_level: device.noise_level,
        drift_rate: device.drift_rate,
        enable_periodic: device.enable_periodic,
        enable_noise: device.enable_noise,
    };
    let tick = (t * 1000.0 / device.interval_ms.max(1) as f64) as u64;
    let phase = (device.seed % 1000) as f64 / 1000.0;
    let mut rng = rand::rngs::StdRng::seed_from_u64(device.seed ^ tick.rotate_left(7));
    Raw::F(composer.compose(&behavior, tick, phase, &mut rng, None))
}

// ── Commands ────────────────────────────────────────────────────────

fn apply_command_to_device(device: &mut Device, name: &str, params: &JsonValue) {
    let action = name.to_ascii_lowercase();
    let mut manual = false;

    match action.as_str() {
        "turn_on" | "on" | "enable" => {
            device.state.manual_switch = Some(true);
            manual = true;
        }
        "turn_off" | "off" | "disable" => {
            device.state.manual_switch = Some(false);
            manual = true;
        }
        "lock" => {
            device.state.manual_locked = Some(true);
            manual = true;
        }
        "unlock" => {
            device.state.manual_locked = Some(false);
            manual = true;
        }
        "arm" => {
            device.state.manual_armed = Some(true);
            manual = true;
        }
        "disarm" => {
            device.state.manual_armed = Some(false);
            manual = true;
        }
        "set_brightness" => {
            let value = param_number(params, "brightness").unwrap_or(100.0);
            device.state.manual_brightness = Some(value);
            device.state.manual_switch = Some(value > 0.0);
            manual = true;
        }
        "set_speed" => {
            let value = param_number(params, "speed").unwrap_or(50.0);
            device.state.manual_speed = Some(value);
            device.state.manual_run_status = Some(if value > 0.0 { "running" } else { "idle" }.to_string());
            manual = true;
        }
        "move_to" => {
            let x = param_number(params, "x").unwrap_or(150.0);
            let y = param_number(params, "y").unwrap_or(100.0);
            let z = param_number(params, "z").unwrap_or(300.0);
            device.state.robot_target = Some((x, y, z));
            device.state.manual_run_status = Some("running".to_string());
            manual = true;
        }
        "go_home" => {
            device.state.robot_target = Some((0.0, 0.0, 200.0));
        }
        "emergency_stop" => {
            device.state.manual_speed = Some(0.0);
            device.state.manual_run_status = Some("stopped".to_string());
            manual = true;
        }
        "set_alarm" => {
            if let Some(value) = param_number(params, "high") {
                device.state.cold_high = value;
            }
            if let Some(value) = param_number(params, "low") {
                device.state.cold_low = value;
            }
        }
        "reset_energy" => {
            device.state.energy_today = 0.0;
        }
        _ => {}
    }

    // Generic parameter → property-state application: `set_<field>` with a
    // matching numeric parameter, or a bare parameter that names a declared
    // property.
    for (key, value) in params.as_object().into_iter().flatten() {
        let Some(number) = json_number(value) else {
            continue;
        };
        let field = key.strip_prefix("set_").unwrap_or(key).to_ascii_lowercase();
        if device.props.iter().any(|prop| prop.name.to_ascii_lowercase() == field) {
            device.state.manual_values.insert(field, number);
            manual = true;
        }
    }

    if manual {
        device.state.manual_at = Some(Instant::now());
    }
}

fn param_number(params: &JsonValue, key: &str) -> Option<f64> {
    params.get(key).and_then(json_number)
}

fn json_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(number) => number.as_f64(),
        JsonValue::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

// ── Counters ────────────────────────────────────────────────────────

fn update_counters(device: &mut Device, conditions: &Conditions, aggregates: &Aggregates, t: f64) {
    let dt = device
        .state
        .last_counter_at
        .map(|last| (t - last).clamp(0.0, 3600.0))
        .unwrap_or(0.0);
    device.state.last_counter_at = Some(t);

    if device.state.counter_day != Some(conditions.day_index) {
        device.state.counter_day = Some(conditions.day_index);
        device.state.energy_today = 0.0;
        device.state.access_count = 0.0;
        device.state.prev_access_event = false;
    }

    let hours = dt / 3600.0;
    match device.role {
        Role::Lighting => {
            let kilowatts = lighting_power(device, conditions, t) / 1000.0;
            device.state.energy_today += kilowatts * hours;
        }
        Role::PowerMeter(branch) => {
            let kilowatts = match branch {
                Branch::Main => aggregates.main_kw,
                Branch::Lighting => aggregates.lighting_kw,
                Branch::Hvac => aggregates.hvac_kw,
            };
            device.state.energy_today += kilowatts * hours;
        }
        Role::WaterMeter { main } => {
            let flow = if main {
                aggregates.main_flow
            } else {
                aggregates.zone_flow
            };
            // L/min → m³: litres / 1000, minutes = seconds / 60.
            device.state.total_consumption += flow * dt / 60.0 / 1000.0;
        }
        Role::ColdRoom if !conditions.cold_door_open => {
            device.state.runtime_hours += hours;
        }
        Role::Access => {
            if conditions.access_event && !device.state.prev_access_event {
                device.state.access_count += 1.0;
            }
            device.state.prev_access_event = conditions.access_event;
        }
        _ => {}
    }
}

// ── Deterministic noise ─────────────────────────────────────────────

fn noise(device: &Device, salt: &str, amplitude: f64, t: f64) -> f64 {
    if !device.enable_noise || amplitude == 0.0 {
        return 0.0;
    }
    amplitude * device.noise_level * unit_noise(device.seed, (t * 5.0) as i64, stable_hash(salt))
}

fn unit_noise(seed: u64, slot: i64, salt: u64) -> f64 {
    let mut value = seed ^ salt.rotate_left(17) ^ (slot as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
}

fn stable_hash(text: &str) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tinyiothub_core::models::thing_property::ThingProperty;

    fn prop(name: &str, data_type: &str, unit: &str, min: Option<f64>, max: Option<f64>) -> ThingProperty {
        ThingProperty {
            id: format!("prop-{name}"),
            thing_id: "thing".to_string(),
            name: name.to_string(),
            display_name: None,
            description: None,
            data_type: Some(data_type.to_string()),
            unit: Some(unit.to_string()),
            min_value: min,
            max_value: max,
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
        }
    }

    fn thing(id: &str, name: &str, category: &str, props: Vec<ThingProperty>) -> Thing {
        Thing {
            id: id.to_string(),
            name: name.to_string(),
            display_name: Some(name.to_string()),
            category: Some(category.to_string()),
            workspace_id: Some("ws-test".to_string()),
            driver_name: Some("simulator".to_string()),
            properties: Some(props),
            ..Default::default()
        }
    }

    fn world_at(hour: u8) -> DateTime<Local> {
        let today = NaiveDate::from_ymd_opt(2026, 6, 1).expect("date");
        Local
            .from_local_datetime(&today.and_hms_opt(u32::from(hour), 0, 0).expect("time"))
            .single()
            .expect("local time")
    }

    fn world_with(hour: u8, devices: &[Thing]) -> SimWorld {
        let mut world = SimWorld::new();
        world.start_override = Some(world_at(hour));
        world.speed_override = Some(1.0);
        world.story_override = Some(true);
        world.start_at = Some(world_at(hour));
        world.speed = Some(1.0);
        world.story_on = Some(true);
        for device in devices {
            world.register(device);
        }
        world
    }

    fn read_at(world: &mut SimWorld, thing_id: &str, seconds_after_start: f64) -> Vec<ResultValue> {
        let start = world.start_at.expect("start");
        // Drive the world clock deterministically: `read_at` computes values
        // from the virtual start plus an explicit offset.
        let now = start + chrono::Duration::milliseconds((seconds_after_start * 1000.0) as i64);
        let story = world.resolved_story();
        let t = seconds_after_start;
        let conditions = Conditions::from_local(&now, story);
        let aggregates = world.aggregates(&conditions, t);
        let device = world.devices.get_mut(thing_id).expect("registered device");
        update_counters(device, &conditions, &aggregates, t);
        values_for(device, &conditions, &aggregates, t)
    }

    fn read_hour(world: &mut SimWorld, thing_id: &str, hour: f64) -> Vec<ResultValue> {
        let start_hour = world.start_at.expect("start").hour() as f64;
        read_at(world, thing_id, (hour - start_hour) * 3600.0)
    }

    fn value(values: &[ResultValue], name: &str) -> String {
        values
            .iter()
            .find(|value| value.name == name)
            .and_then(|value| value.value.clone())
            .unwrap_or_default()
    }

    fn number(values: &[ResultValue], name: &str) -> f64 {
        value(values, name).parse().unwrap_or(f64::NAN)
    }

    #[test]
    fn infers_demo_plant_roles() {
        let env = thing(
            "env",
            "env_sensor_workshop",
            "环境传感器",
            vec![
                prop("temperature", "number", "°C", Some(-20.0), Some(60.0)),
                prop("humidity", "number", "%", None, None),
            ],
        );
        let cold = thing(
            "cold",
            "cold_chain_fridge",
            "温度记录仪",
            vec![
                prop("temperature", "number", "°C", Some(-30.0), Some(10.0)),
                prop("door_open", "boolean", "", None, None),
            ],
        );
        let smoke = thing(
            "smoke",
            "smoke_detector_a",
            "消防探测器",
            vec![
                prop("smoke_density", "number", "%/m", None, None),
                prop("alarm", "boolean", "", None, None),
            ],
        );
        let meter = thing(
            "meter",
            "power_meter_lighting",
            "电力仪表",
            vec![
                prop("active_power", "number", "kW", None, None),
                prop("voltage", "number", "V", None, None),
            ],
        );

        assert_eq!(infer_role(&env), Role::Environment);
        assert_eq!(infer_zone(&env, infer_role(&env)), Zone::Workshop);
        assert_eq!(infer_role(&cold), Role::ColdRoom);
        assert_eq!(infer_role(&smoke), Role::Smoke);
        assert_eq!(infer_role(&meter), Role::PowerMeter(Branch::Lighting));
        assert_eq!(infer_zone(&meter, infer_role(&meter)), Zone::Other);
    }

    #[test]
    fn command_turns_light_on_and_reads_back() {
        let mut light = thing(
            "light-workshop",
            "lighting_workshop",
            "照明控制器",
            vec![
                prop("switch_on", "boolean", "", None, None),
                prop("brightness", "number", "%", Some(0.0), Some(100.0)),
                prop("power", "number", "W", None, None),
                prop("energy_today", "number", "kWh", None, None),
                prop("presence", "boolean", "", None, None),
            ],
        );
        light.driver_options = Some(r#"{"clock_start":"2026-06-01T09:00:00"}"#.to_string());
        let mut world = world_with(9, std::slice::from_ref(&light));

        let before = read_hour(&mut world, "light-workshop", 9.0);
        assert_eq!(value(&before, "switch_on"), "true");
        assert!(number(&before, "power") > 0.0);

        assert!(world.apply_command("light-workshop", "turn_off", None));
        let after = read_hour(&mut world, "light-workshop", 10.0);
        assert_eq!(value(&after, "switch_on"), "false");
        assert_eq!(number(&after, "power"), 0.0);
    }

    #[test]
    fn lighting_feeds_branch_and_main_meter() {
        let light = thing(
            "light-workshop",
            "lighting_workshop",
            "照明控制器",
            vec![
                prop("switch_on", "boolean", "", None, None),
                prop("brightness", "number", "%", None, None),
                prop("power", "number", "W", None, None),
            ],
        );
        let branch = thing(
            "meter-lighting",
            "power_meter_lighting",
            "电力仪表",
            vec![
                prop("active_power", "number", "kW", None, None),
                prop("voltage", "number", "V", None, None),
            ],
        );
        let main = thing(
            "meter-main",
            "power_meter_main",
            "电力仪表",
            vec![prop("active_power", "number", "kW", None, None)],
        );
        let mut world = world_with(9, &[light, branch, main]);

        let branch_before = read_hour(&mut world, "meter-lighting", 9.0);
        let main_before = read_hour(&mut world, "meter-main", 9.0);
        assert!(world.apply_command("light-workshop", "turn_off", None));
        let branch_off = read_hour(&mut world, "meter-lighting", 10.0);
        let main_off = read_hour(&mut world, "meter-main", 10.0);
        assert!(number(&branch_off, "active_power") < number(&branch_before, "active_power") - 0.5);
        assert!(number(&main_off, "active_power") < number(&main_before, "active_power") - 0.5);

        assert!(world.apply_command("light-workshop", "set_brightness", Some(r#"{"brightness":100}"#)));
        let branch_on = read_hour(&mut world, "meter-lighting", 11.0);
        assert!(number(&branch_on, "active_power") > number(&branch_off, "active_power") + 0.5);
    }

    #[test]
    fn cold_room_door_raises_temperature_above_alarm() {
        let cold = thing(
            "cold",
            "cold_chain_fridge",
            "温度记录仪",
            vec![
                prop("temperature", "number", "°C", Some(-30.0), Some(10.0)),
                prop("door_open", "boolean", "", None, None),
            ],
        );
        let mut world = world_with(10, std::slice::from_ref(&cold));

        let before = read_hour(&mut world, "cold", 10.4);
        assert_eq!(value(&before, "door_open"), "false");
        assert!(number(&before, "temperature") < 4.5);

        let during = read_hour(&mut world, "cold", 10.7);
        assert_eq!(value(&during, "door_open"), "true");
        assert!(
            number(&during, "temperature") > 8.0,
            "door should warm the room above the 8°C alarm, got {}",
            number(&during, "temperature")
        );

        let later = read_hour(&mut world, "cold", 13.0);
        assert!(
            number(&later, "temperature") < 6.0,
            "room should recover after the door closes, got {}",
            number(&later, "temperature")
        );
    }

    #[test]
    fn smoke_story_raises_smoke_and_heat_alarm() {
        let smoke = thing(
            "smoke",
            "smoke_detector_a",
            "消防探测器",
            vec![
                prop("smoke_density", "number", "%/m", Some(0.0), Some(20.0)),
                prop("alarm", "boolean", "", None, None),
            ],
        );
        let heat = thing(
            "heat",
            "heat_detector_a",
            "消防探测器",
            vec![
                prop("temperature", "number", "°C", None, Some(150.0)),
                prop("alarm", "boolean", "", None, None),
            ],
        );
        let mut world = world_with(13, &[smoke, heat]);

        let quiet = read_hour(&mut world, "smoke", 13.0);
        assert_eq!(value(&quiet, "alarm"), "false");

        let fire = read_hour(&mut world, "smoke", 14.1);
        assert!(number(&fire, "smoke_density") > 5.0);
        assert_eq!(value(&fire, "alarm"), "true");

        let hot = read_hour(&mut world, "heat", 14.1);
        assert_eq!(value(&hot, "alarm"), "true");
    }

    #[test]
    fn water_leak_drops_zone_pressure() {
        let zone = thing(
            "water-zone",
            "water_meter_zone_a",
            "智能水表",
            vec![
                prop("flow_rate", "number", "L/min", None, None),
                prop("pressure", "number", "MPa", Some(0.0), Some(1.6)),
            ],
        );
        let main = thing(
            "water-main",
            "water_meter_main",
            "智能水表",
            vec![
                prop("flow_rate", "number", "L/min", None, None),
                prop("pressure", "number", "MPa", None, None),
            ],
        );
        let mut world = world_with(15, &[zone, main]);

        let normal = read_hour(&mut world, "water-zone", 15.0);
        assert!(number(&normal, "pressure") > 0.15);

        let leak = read_hour(&mut world, "water-zone", 16.2);
        assert!(
            number(&leak, "pressure") < 0.15,
            "leak should drop zone pressure below the alarm threshold, got {}",
            number(&leak, "pressure")
        );
        let main_leak = read_hour(&mut world, "water-main", 16.2);
        assert!(number(&main_leak, "flow_rate") > number(&normal, "flow_rate"));
    }

    #[test]
    fn same_seed_and_time_produce_same_values() {
        let make = || {
            thing(
                "env",
                "env_sensor_workshop",
                "环境传感器",
                vec![prop("temperature", "number", "°C", None, None)],
            )
        };
        let mut first = world_with(9, &[make()]);
        let mut second = world_with(9, &[make()]);
        let first_values = read_hour(&mut first, "env", 10.0);
        let second_values = read_hour(&mut second, "env", 10.0);
        assert_eq!(
            value(&first_values, "temperature"),
            value(&second_values, "temperature")
        );
    }

    #[test]
    fn every_declared_property_gets_a_value() {
        let device = thing(
            "robot",
            "robot_arm_assembly",
            "协作机器人",
            vec![
                prop("pos_x", "number", "mm", None, None),
                prop("run_status", "string", "", None, None),
                prop("unknown_metric", "number", "", None, None),
            ],
        );
        let mut world = world_with(9, &[device]);
        let values = read_hour(&mut world, "robot", 10.0);
        assert_eq!(values.len(), 3);
        assert!(value(&values, "unknown_metric").parse::<f64>().is_ok());
        assert_eq!(value(&values, "run_status"), "running");
    }

    #[test]
    fn absurd_clock_speed_is_bounded_instead_of_panicking() {
        let mut world = SimWorld::new();
        world.speed_override = Some(1.0e18);
        world.speed = Some(1.0e18);
        world.start_at = Some(world_at(6));
        world.started = Instant::now() - std::time::Duration::from_secs(1_000);

        let now = world.virtual_now();
        assert!(now.year() > 2026);
    }

    #[test]
    fn late_story_cast_reanchors_the_virtual_clock() {
        let env = thing(
            "env",
            "env_sensor_workshop",
            "环境传感器",
            vec![prop("temperature", "number", "°C", None, None)],
        );
        let cold = thing(
            "cold",
            "cold_chain_fridge",
            "温度记录仪",
            vec![
                prop("temperature", "number", "°C", None, None),
                prop("door_open", "boolean", "", None, None),
            ],
        );
        let smoke = thing(
            "smoke",
            "smoke_detector_a",
            "消防探测器",
            vec![prop("smoke_density", "number", "%/m", None, None)],
        );
        let light = thing(
            "light",
            "lighting_workshop",
            "照明控制器",
            vec![
                prop("switch_on", "boolean", "", None, None),
                prop("brightness", "number", "%", None, None),
            ],
        );
        let water = thing(
            "water",
            "water_meter_main",
            "智能水表",
            vec![
                prop("flow_rate", "number", "L/min", None, None),
                prop("pressure", "number", "MPa", None, None),
            ],
        );

        let mut world = SimWorld::new();
        world.register(&env);
        let _ = world.read("env");
        assert_eq!(world.speed, Some(1.0), "partial cast must start at real speed");
        assert_eq!(world.anchored_story, Some(false));

        for device in [&cold, &smoke, &light, &water] {
            world.register(device);
        }
        let _ = world.read("env");
        assert_eq!(
            world.speed,
            Some(120.0),
            "story cast completion must re-anchor to demo speed"
        );
        assert_eq!(world.anchored_story, Some(true));
        assert_eq!(world.start_at.expect("start").hour(), 6);
    }

    #[test]
    fn warehouse_humidity_spike_crosses_alarm_threshold() {
        let warehouse = thing(
            "env-warehouse",
            "env_sensor_warehouse",
            "环境传感器",
            vec![
                prop("temperature", "number", "°C", None, None),
                prop("humidity", "number", "%", Some(0.0), Some(100.0)),
            ],
        );
        let mut world = world_with(12, &[warehouse]);

        let normal = read_hour(&mut world, "env-warehouse", 12.0);
        assert!(number(&normal, "humidity") < 85.0);

        let spike = read_hour(&mut world, "env-warehouse", 12.6);
        assert!(
            number(&spike, "humidity") > 85.0,
            "humidity spike should cross the warehouse alarm threshold, got {}",
            number(&spike, "humidity")
        );
    }

    #[test]
    fn emergency_stop_halts_the_robot() {
        let robot = thing(
            "robot",
            "robot_arm_assembly",
            "协作机器人",
            vec![
                prop("pos_x", "number", "mm", None, None),
                prop("speed", "number", "%", None, None),
                prop("run_status", "string", "", None, None),
            ],
        );
        let mut world = world_with(9, &[robot]);
        assert!(world.apply_command("robot", "emergency_stop", None));

        let stopped = read_hour(&mut world, "robot", 10.0);
        assert_eq!(value(&stopped, "run_status"), "stopped");
        assert_eq!(number(&stopped, "speed"), 0.0);
        assert_eq!(number(&stopped, "pos_x"), 0.0);
    }

    #[test]
    fn gate_way_counts_registered_devices() {
        let env = thing(
            "env",
            "env_sensor_workshop",
            "环境传感器",
            vec![prop("temperature", "number", "°C", None, None)],
        );
        let gateway = thing(
            "gw",
            "gateway_floor1",
            "边缘网关",
            vec![prop("connected_devices", "number", "台", None, None)],
        );
        let mut world = world_with(9, &[env, gateway]);
        let values = read_hour(&mut world, "gw", 10.0);
        assert_eq!(number(&values, "connected_devices"), 2.0);
    }
}

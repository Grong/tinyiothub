# Simulated Driver Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade SimulatedDriver with pattern-based property matching, signal composition (periodic + trend + Gaussian noise), four anomaly types, and tag-based device correlation.

**Architecture:** Split the current 380-line monolithic `simulated_driver.rs` into a `simulated/` subdirectory with four focused modules (`patterns`, `signal`, `anomaly`, `correlation`). The main driver file shrinks to ~200 lines, orchestrating the pipeline: patterns → signal → anomaly → correlation.

**Tech Stack:** Rust, rand crate (StdRng, Box-Muller for Gaussian), parking_lot::Mutex (global correlation state), regex (tag pattern matching), existing DeviceDriver trait (unchanged).

---

### Task 1: Create `simulated/` module scaffolding

**Files:**
- Create: `crates/tinyiothub-runtime/src/driver/drivers/simulated/mod.rs`
- Modify: `crates/tinyiothub-runtime/src/driver/drivers/mod.rs`

- [ ] **Step 1: Create simulated/mod.rs with module declarations**

File: `crates/tinyiothub-runtime/src/driver/drivers/simulated/mod.rs`
```rust
pub mod anomaly;
pub mod correlation;
pub mod patterns;
pub mod signal;
```

- [ ] **Step 2: Add `pub mod simulated;` to drivers/mod.rs**

File: `crates/tinyiothub-runtime/src/driver/drivers/mod.rs`

Change:
```rust
pub use modbus_driver::ModbusDriver;
pub use simulated_driver::SimulatedDriver;

pub mod modbus_driver;
pub mod simulated_driver;
pub mod snmp_driver;
```

To:
```rust
pub use modbus_driver::ModbusDriver;
pub use simulated_driver::SimulatedDriver;

pub mod modbus_driver;
pub mod simulated;
pub mod simulated_driver;
pub mod snmp_driver;
```

- [ ] **Step 3: Build to verify scaffolding compiles**

Run: `cargo build -p tinyiothub-runtime 2>&1`
Expected: Compiles successfully (no warnings)

- [ ] **Step 4: Commit**

```bash
git add crates/tinyiothub-runtime/src/driver/drivers/mod.rs crates/tinyiothub-runtime/src/driver/drivers/simulated/mod.rs
git commit -m "feat: add simulated/ module scaffolding"
```

---

### Task 2: Property name pattern matching (`patterns.rs`)

**Files:**
- Create: `crates/tinyiothub-runtime/src/driver/drivers/simulated/patterns.rs`

- [ ] **Step 1: Write PropertyBehavior struct and match_property function**

File: `crates/tinyiothub-runtime/src/driver/drivers/simulated/patterns.rs`
```rust
/// Behavior profile for a property, determined by matching its name against
/// known patterns.
#[derive(Debug, Clone)]
pub struct PropertyBehavior {
    /// Baseline value around which the signal oscillates.
    pub baseline: f64,
    /// Amplitude of the daily periodic component (sine wave).
    pub daily_amplitude: f64,
    /// Standard deviation of Gaussian noise added each tick.
    pub noise_sigma: f64,
    /// Display unit (e.g., "°C", "%", "A").
    pub unit: String,
    /// Whether this property uses cumulative mode (value always increases).
    pub cumulative: bool,
    /// Whether this property is boolean/enum-like (no random walk).
    pub discrete: bool,
}

/// Match a property name against the built-in pattern table, returning
/// the corresponding behavior profile.
///
/// Matching is case-insensitive substring match. Falls back to sensible
/// defaults based on `data_type` when no pattern matches.
pub fn match_property(name: &str, data_type: &str) -> PropertyBehavior {
    let name_lower = name.to_lowercase();

    // Temperature sensors
    if name_lower.contains("temp") {
        return PropertyBehavior {
            baseline: 25.0,
            daily_amplitude: 8.0,
            noise_sigma: 0.3,
            unit: "°C".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Humidity sensors
    if name_lower.contains("humid") {
        return PropertyBehavior {
            baseline: 60.0,
            daily_amplitude: 15.0,
            noise_sigma: 1.0,
            unit: "%".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Vibration sensors
    if name_lower.contains("vibration") || name_lower.contains("vib") {
        return PropertyBehavior {
            baseline: 2.0,
            daily_amplitude: 1.5,
            noise_sigma: 0.2,
            unit: "mm/s".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Current / amperage
    if name_lower.contains("current") || name_lower.contains("amp") {
        return PropertyBehavior {
            baseline: 10.0,
            daily_amplitude: 5.0,
            noise_sigma: 0.5,
            unit: "A".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Voltage
    if name_lower.contains("voltage") || name_lower.contains("volt") {
        return PropertyBehavior {
            baseline: 220.0,
            daily_amplitude: 5.0,
            noise_sigma: 1.0,
            unit: "V".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Power / wattage
    if name_lower.contains("power") || name_lower.contains("watt") || name_lower.contains("kw") {
        return PropertyBehavior {
            baseline: 50.0,
            daily_amplitude: 30.0,
            noise_sigma: 2.0,
            unit: "W".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Motor speed / RPM
    if name_lower.contains("speed") || name_lower.contains("rpm") {
        return PropertyBehavior {
            baseline: 1500.0,
            daily_amplitude: 200.0,
            noise_sigma: 30.0,
            unit: "rpm".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Flow rate
    if name_lower.contains("flow") {
        return PropertyBehavior {
            baseline: 100.0,
            daily_amplitude: 30.0,
            noise_sigma: 3.0,
            unit: "m³/h".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Pressure
    if name_lower.contains("pressure") {
        return PropertyBehavior {
            baseline: 1.0,
            daily_amplitude: 0.3,
            noise_sigma: 0.05,
            unit: "MPa".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Level / fill percentage
    if name_lower.contains("level") {
        return PropertyBehavior {
            baseline: 50.0,
            daily_amplitude: 10.0,
            noise_sigma: 1.0,
            unit: "%".to_string(),
            cumulative: false,
            discrete: false,
        };
    }

    // Energy / kWh — cumulative (always increasing)
    if name_lower.contains("energy") || name_lower.contains("kwh") {
        return PropertyBehavior {
            baseline: 0.0,
            daily_amplitude: 0.0,
            noise_sigma: 0.0,
            unit: "kWh".to_string(),
            cumulative: true,
            discrete: false,
        };
    }

    // Status / state — discrete enum values
    if name_lower.contains("status") || name_lower.contains("state") {
        return PropertyBehavior {
            baseline: 0.0,
            daily_amplitude: 0.0,
            noise_sigma: 0.0,
            unit: String::new(),
            cumulative: false,
            discrete: true,
        };
    }

    // Switch / relay — boolean that holds state
    if name_lower.contains("switch") || name_lower.contains("relay") {
        return PropertyBehavior {
            baseline: 0.0,
            daily_amplitude: 0.0,
            noise_sigma: 0.0,
            unit: String::new(),
            cumulative: false,
            discrete: true,
        };
    }

    // Fallback: use data_type to decide defaults
    fallback_by_type(data_type)
}

fn fallback_by_type(data_type: &str) -> PropertyBehavior {
    match data_type {
        "float" | "number" | "double" => PropertyBehavior {
            baseline: 50.0,
            daily_amplitude: 5.0,
            noise_sigma: 1.0,
            unit: String::new(),
            cumulative: false,
            discrete: false,
        },
        "int" | "integer" => PropertyBehavior {
            baseline: 50.0,
            daily_amplitude: 5.0,
            noise_sigma: 1.0,
            unit: String::new(),
            cumulative: false,
            discrete: false,
        },
        "bool" | "boolean" => PropertyBehavior {
            baseline: 0.0,
            daily_amplitude: 0.0,
            noise_sigma: 0.0,
            unit: String::new(),
            cumulative: false,
            discrete: true,
        },
        _ => PropertyBehavior {
            baseline: 0.0,
            daily_amplitude: 0.0,
            noise_sigma: 0.0,
            unit: String::new(),
            cumulative: false,
            discrete: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_match() {
        let b = match_property("temperature", "float");
        assert!((b.baseline - 25.0).abs() < f64::EPSILON);
        assert!((b.daily_amplitude - 8.0).abs() < f64::EPSILON);
        assert_eq!(b.unit, "°C");
        assert!(!b.cumulative);
        assert!(!b.discrete);
    }

    #[test]
    fn test_case_insensitive_match() {
        let b = match_property("Current_PhaseA", "float");
        assert!((b.baseline - 10.0).abs() < f64::EPSILON);
        assert_eq!(b.unit, "A");
    }

    #[test]
    fn test_partial_match() {
        let b = match_property("motor_temp_celsius", "float");
        assert!((b.baseline - 25.0).abs() < f64::EPSILON);
        assert_eq!(b.unit, "°C");
    }

    #[test]
    fn test_energy_cumulative() {
        let b = match_property("daily_energy", "float");
        assert!(b.cumulative);
    }

    #[test]
    fn test_status_discrete() {
        let b = match_property("device_status", "enum");
        assert!(b.discrete);
    }

    #[test]
    fn test_switch_discrete() {
        let b = match_property("relay_1", "boolean");
        assert!(b.discrete);
    }

    #[test]
    fn test_unknown_fallback_float() {
        let b = match_property("custom_metric", "float");
        assert!((b.baseline - 50.0).abs() < f64::EPSILON);
        assert!(!b.discrete);
    }

    #[test]
    fn test_unknown_fallback_bool() {
        let b = match_property("custom_flag", "boolean");
        assert!(b.discrete);
    }
}
```

- [ ] **Step 2: Run tests to verify pattern matching**

Run: `cargo test -p tinyiothub-runtime simulated::patterns -- --nocapture 2>&1`
Expected: All 8 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-runtime/src/driver/drivers/simulated/patterns.rs
git commit -m "feat: add property name pattern matching for 12+ device types"
```

---

### Task 3: Signal composition engine (`signal.rs`)

**Files:**
- Create: `crates/tinyiothub-runtime/src/driver/drivers/simulated/signal.rs`

- [ ] **Step 1: Write SignalComposer with periodic, trend, and Gaussian noise**

File: `crates/tinyiothub-runtime/src/driver/drivers/simulated/signal.rs`
```rust
use rand::Rng;
use std::f64::consts::PI;

use super::correlation::EnvironmentContext;
use super::patterns::PropertyBehavior;

/// Composes a sensor value from multiple signal components:
/// baseline + periodic (daily cycle) + trend (slow drift) + noise (Gaussian).
pub struct SignalComposer {
    /// Device refresh interval in milliseconds, used to calculate tick-to-time mapping.
    pub interval_ms: u64,
    /// Scale factor applied to the daily amplitude from PropertyBehavior.
    pub daily_amplitude_scale: f64,
    /// Scale factor applied to the noise sigma from PropertyBehavior.
    pub noise_level: f64,
    /// Long-term drift rate (units per tick).
    pub drift_rate: f64,
    /// Whether the periodic component is enabled.
    pub enable_periodic: bool,
    /// Whether noise is enabled.
    pub enable_noise: bool,
}

impl SignalComposer {
    /// Compose a signal value for the given tick and property behavior.
    ///
    /// `phase_offset` is a per-device random offset (0.0–1.0) so devices
    /// with the same behavior don't produce identical values.
    /// `group_ctx` is optional shared environment context from tag correlation.
    pub fn compose(
        &self,
        behavior: &PropertyBehavior,
        tick: u64,
        phase_offset: f64,
        rng: &mut impl Rng,
        group_ctx: Option<&EnvironmentContext>,
    ) -> f64 {
        let mut value = behavior.baseline;

        // 1. Periodic component (daily cycle sine wave)
        if self.enable_periodic && behavior.daily_amplitude > 0.0 {
            let amplitude = behavior.daily_amplitude * self.daily_amplitude_scale;
            value += periodic(tick, amplitude, phase_offset, self.interval_ms);
        }

        // 2. Trend component (slow long-term drift)
        if self.drift_rate != 0.0 {
            value += self.drift_rate * tick as f64;
        }

        // 3. Group context (tag-based correlation)
        if let Some(ctx) = group_ctx {
            // Apply context based on property type (detected from behavior)
            if behavior.unit == "°C" {
                value += ctx.temperature_offset;
            } else if behavior.unit == "V" {
                value += ctx.voltage_offset;
            } else if behavior.unit == "A" || behavior.unit == "W" {
                value *= 1.0 + ctx.load_factor;
            }
        }

        // 4. Gaussian noise
        if self.enable_noise && behavior.noise_sigma > 0.0 {
            let sigma = behavior.noise_sigma * self.noise_level;
            value += gaussian_noise(rng, sigma);
        }

        value
    }
}

/// Daily cycle sine wave: one full oscillation per 24 hours.
///
/// `phase_offset` shifts the wave horizontally (0.0–1.0 maps to 0–24h offset).
fn periodic(tick: u64, amplitude: f64, phase_offset: f64, interval_ms: u64) -> f64 {
    // Ticks per 24h period
    let ticks_per_day = (24.0 * 3600.0 * 1000.0) / interval_ms as f64;
    let angle = 2.0 * PI * (tick as f64 / ticks_per_day + phase_offset);
    amplitude * angle.sin()
}

/// Box-Muller transform: convert two uniform randoms into one Gaussian sample.
fn gaussian_noise(rng: &mut impl Rng, sigma: f64) -> f64 {
    let u1: f64 = rng.gen_range(0.0..1.0);
    let u2: f64 = rng.gen_range(0.0..1.0);
    // Clamp u1 to avoid ln(0)
    let u1 = if u1 < 1e-10 { 1e-10 } else { u1 };
    sigma * (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    fn make_behavior() -> PropertyBehavior {
        PropertyBehavior {
            baseline: 25.0,
            daily_amplitude: 8.0,
            noise_sigma: 0.3,
            unit: "°C".to_string(),
            cumulative: false,
            discrete: false,
        }
    }

    fn make_composer() -> SignalComposer {
        SignalComposer {
            interval_ms: 1000,
            daily_amplitude_scale: 1.0,
            noise_level: 1.0,
            drift_rate: 0.0,
            enable_periodic: true,
            enable_noise: true,
        }
    }

    #[test]
    fn test_periodic_zero_at_midnight() {
        // At tick=0, phase_offset=0, sin(0)=0 → value ≈ baseline + noise
        let composer = make_composer();
        let mut rng = StdRng::seed_from_u64(42);
        let val = composer.compose(&make_behavior(), 0, 0.0, &mut rng, None);
        // Should be close to baseline (25.0), with small noise
        assert!((val - 25.0).abs() < 2.0, "got {val}");
    }

    #[test]
    fn test_periodic_quarter_day_is_max() {
        // At 1/4 day, sin(π/2)=1 → value ≈ baseline + amplitude
        let composer = make_composer();
        let mut rng = StdRng::seed_from_u64(42);
        let ticks_per_day = (24.0 * 3600.0) / 1.0; // interval_ms=1000
        let quarter_day = (ticks_per_day / 4.0) as u64;
        // Disable noise for this test
        let mut composer_no_noise = make_composer();
        composer_no_noise.enable_noise = false;
        let val = composer_no_noise.compose(&make_behavior(), quarter_day, 0.0, &mut rng, None);
        // baseline 25.0 + amplitude 8.0 = 33.0
        assert!((val - 33.0).abs() < 1.0, "got {val}");
    }

    #[test]
    fn test_drift_adds_linearly() {
        let behavior = make_behavior();
        let mut rng = StdRng::seed_from_u64(42);
        let composer = SignalComposer {
            interval_ms: 1000,
            daily_amplitude_scale: 1.0,
            noise_level: 0.0,
            drift_rate: 0.01,
            enable_periodic: false,
            enable_noise: false,
        };
        let v0 = composer.compose(&behavior, 0, 0.0, &mut rng, None);
        let v100 = composer.compose(&behavior, 100, 0.0, &mut rng, None);
        // v100 should be 1.0 higher than v0
        assert!(((v100 - v0) - 1.0).abs() < 0.01, "v0={v0}, v100={v100}");
    }

    #[test]
    fn test_noise_disabled_returns_deterministic() {
        let behavior = make_behavior();
        let mut rng = StdRng::seed_from_u64(42);
        let composer = SignalComposer {
            enable_noise: false,
            enable_periodic: false,
            ..make_composer()
        };
        let v1 = composer.compose(&behavior, 100, 0.5, &mut rng, None);
        let v2 = composer.compose(&behavior, 100, 0.5, &mut rng, None);
        assert!((v1 - v2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gaussian_noise_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        // Generate many samples; 99.7% should be within ±3 sigma
        let sigma = 1.0;
        let mut within_3sigma = 0;
        let total = 1000;
        for _ in 0..total {
            let v = gaussian_noise(&mut rng, sigma);
            if v.abs() <= 3.0 * sigma {
                within_3sigma += 1;
            }
        }
        assert!(within_3sigma as f64 / total as f64 > 0.99);
    }
}
```

- [ ] **Step 2: Run tests to verify signal composition**

Run: `cargo test -p tinyiothub-runtime simulated::signal -- --nocapture 2>&1`
Expected: All 5 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-runtime/src/driver/drivers/simulated/signal.rs
git commit -m "feat: add signal composition engine with periodic, trend, and Gaussian noise"
```

---

### Task 4: Anomaly engine (`anomaly.rs`)

**Files:**
- Create: `crates/tinyiothub-runtime/src/driver/drivers/simulated/anomaly.rs`

- [ ] **Step 1: Write AnomalyEngine with four anomaly types**

File: `crates/tinyiothub-runtime/src/driver/drivers/simulated/anomaly.rs`
```rust
use rand::Rng;

/// The four anomaly types the engine can inject.
#[derive(Debug, Clone)]
enum AnomalyState {
    /// No anomaly active.
    Inactive,
    /// Value slowly drifts in one direction.
    Drift {
        direction: f64,
        rate: f64,
        remaining: u32,
    },
    /// Sudden spike above/below normal, recovers quickly.
    Spike {
        normal_value: f64,
        spike_value: f64,
        remaining: u32,
    },
    /// Value oscillates rapidly between normal and abnormal.
    Jitter {
        normal_value: f64,
        abnormal_value: f64,
        remaining: u32,
    },
    /// Value freezes at current reading.
    Stuck {
        frozen_value: f64,
        remaining: u32,
    },
}

/// Drives anomaly injection for a single property.
///
/// Each tick: if no anomaly is active, rolls for a new one. If active,
/// decrements the counter and returns the anomaly offset.
pub struct AnomalyEngine {
    state: AnomalyState,
    /// Per-tick probability of starting a slow drift.
    drift_probability: f64,
    /// Per-tick probability of starting a spike.
    spike_probability: f64,
    /// Per-tick probability of starting intermittent jitter.
    jitter_probability: f64,
    /// Per-tick probability of starting a stuck-value anomaly.
    stuck_probability: f64,
    /// Master enable/disable for all anomaly injection.
    pub enabled: bool,
    /// Global probability scale (multiplied into each type's probability).
    pub probability_scale: f64,
}

impl AnomalyEngine {
    pub fn new(
        drift_probability: f64,
        spike_probability: f64,
        jitter_probability: f64,
        stuck_probability: f64,
    ) -> Self {
        Self {
            state: AnomalyState::Inactive,
            drift_probability,
            spike_probability,
            jitter_probability,
            stuck_probability,
            enabled: true,
            probability_scale: 1.0,
        }
    }

    /// Create an engine with default probabilities (sum ≈ 5%).
    pub fn with_defaults() -> Self {
        Self::new(0.01, 0.02, 0.01, 0.01)
    }

    /// Advance one tick. Returns the anomaly offset to add to the normal value
    /// (0.0 when no anomaly is active).
    pub fn tick(&mut self, normal_value: f64, rng: &mut impl Rng) -> f64 {
        // If an anomaly is active, process it
        match &mut self.state {
            AnomalyState::Drift {
                direction,
                rate,
                remaining,
            } => {
                *remaining -= 1;
                let offset = *direction * *rate;
                if *remaining == 0 {
                    self.state = AnomalyState::Inactive;
                }
                return offset;
            }
            AnomalyState::Spike {
                normal_value: nv,
                spike_value,
                remaining,
            } => {
                *remaining -= 1;
                let offset = *spike_value - *nv;
                if *remaining == 0 {
                    self.state = AnomalyState::Inactive;
                }
                return offset;
            }
            AnomalyState::Jitter {
                normal_value: nv,
                abnormal_value,
                remaining,
            } => {
                *remaining -= 1;
                // Rapidly alternate between normal and abnormal each tick
                let offset = if *remaining % 2 == 0 {
                    *abnormal_value - *nv
                } else {
                    0.0
                };
                if *remaining == 0 {
                    self.state = AnomalyState::Inactive;
                }
                return offset;
            }
            AnomalyState::Stuck {
                frozen_value: _,
                remaining,
            } => {
                *remaining -= 1;
                // Return 0 offset — caller should use frozen_value, which is handled
                // by the driver returning the frozen value directly.
                if *remaining == 0 {
                    self.state = AnomalyState::Inactive;
                }
                return 0.0;
            }
            AnomalyState::Inactive => {}
        }

        // No anomaly active — maybe start one
        if !self.enabled {
            return 0.0;
        }

        let scale = self.probability_scale;
        let roll: f64 = rng.gen_range(0.0..1.0);

        // Check each type in order (drift → spike → jitter → stuck)
        let mut cumulative = 0.0;

        cumulative += self.drift_probability * scale;
        if roll < cumulative {
            let direction = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            let rate = rng.gen_range(0.1..0.5);
            let remaining = rng.gen_range(30..=120);
            self.state = AnomalyState::Drift {
                direction,
                rate,
                remaining,
            };
            return direction * rate;
        }

        cumulative += self.spike_probability * scale;
        if roll < cumulative {
            let magnitude = rng.gen_range(1.5..3.0);
            let direction = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            let offset = direction * magnitude * 10.0; // generic amplitude
            let spike_value = normal_value + offset;
            let remaining = rng.gen_range(1..=3);
            self.state = AnomalyState::Spike {
                normal_value,
                spike_value,
                remaining,
            };
            return offset;
        }

        cumulative += self.jitter_probability * scale;
        if roll < cumulative {
            let magnitude = rng.gen_range(1.5..2.5);
            let direction = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            let abnormal_value = normal_value + direction * magnitude * 10.0;
            let remaining = rng.gen_range(10..=30);
            self.state = AnomalyState::Jitter {
                normal_value,
                abnormal_value,
                remaining,
            };
            return 0.0; // first tick shows normal, alternates next tick
        }

        cumulative += self.stuck_probability * scale;
        if roll < cumulative {
            let remaining = rng.gen_range(20..=60);
            self.state = AnomalyState::Stuck {
                frozen_value: normal_value,
                remaining,
            };
            return 0.0;
        }

        0.0
    }

    /// Check if the engine is currently in a Stuck state.
    /// Caller uses this to return the frozen value instead of computing a new one.
    pub fn frozen_value(&self) -> Option<f64> {
        match &self.state {
            AnomalyState::Stuck { frozen_value, .. } => Some(*frozen_value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn test_no_anomaly_when_disabled() {
        let mut engine = AnomalyEngine::with_defaults();
        engine.enabled = false;
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            assert!((engine.tick(25.0, &mut rng) - 0.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_zero_probability_no_anomaly() {
        let mut engine = AnomalyEngine::new(0.0, 0.0, 0.0, 0.0);
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            assert!((engine.tick(25.0, &mut rng) - 0.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_anomaly_eventually_fires() {
        // With 100% probability, anomaly should fire immediately
        let mut engine = AnomalyEngine::new(1.0, 0.0, 0.0, 0.0);
        let mut rng = StdRng::seed_from_u64(42);
        let offset = engine.tick(25.0, &mut rng);
        // A drift anomaly should produce non-zero offset
        assert!(offset.abs() > 0.0, "expected anomaly offset, got {offset}");
    }

    #[test]
    fn test_spike_eventually_recovers() {
        let mut engine = AnomalyEngine::new(0.0, 1.0, 0.0, 0.0);
        let mut rng = StdRng::seed_from_u64(42);
        // First tick: spike starts
        let offset = engine.tick(25.0, &mut rng);
        assert!(offset.abs() > 0.0, "expected spike offset, got {offset}");
        // Tick through the remaining duration (max 3)
        for _ in 0..5 {
            engine.tick(25.0, &mut rng);
        }
        // Should be back to Inactive, next tick returns 0
        let offset2 = engine.tick(25.0, &mut rng);
        assert!((offset2 - 0.0).abs() < f64::EPSILON, "expected recovery, got {offset2}");
    }

    #[test]
    fn test_stuck_returns_zero_offset_but_frozen() {
        let mut engine = AnomalyEngine::new(0.0, 0.0, 0.0, 1.0);
        let mut rng = StdRng::seed_from_u64(42);
        let offset = engine.tick(25.0, &mut rng);
        assert!((offset - 0.0).abs() < f64::EPSILON, "stuck offset should be 0, got {offset}");
        assert!(engine.frozen_value().is_some(), "should have frozen value");
    }
}
```

- [ ] **Step 2: Run tests to verify anomaly engine**

Run: `cargo test -p tinyiothub-runtime simulated::anomaly -- --nocapture 2>&1`
Expected: All 5 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-runtime/src/driver/drivers/simulated/anomaly.rs
git commit -m "feat: add anomaly engine with drift, spike, jitter, and stuck types"
```

---

### Task 5: Tag-based correlation (`correlation.rs`)

**Files:**
- Create: `crates/tinyiothub-runtime/src/driver/drivers/simulated/correlation.rs`

- [ ] **Step 1: Write EnvironmentContext and CorrelationManager**

File: `crates/tinyiothub-runtime/src/driver/drivers/simulated/correlation.rs`
```rust
use parking_lot::Mutex;
use rand::Rng;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Shared environmental state for a group of devices (keyed by tag name).
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    pub tag_name: String,
    /// Temperature baseline offset for this zone (added to all temp sensors).
    pub temperature_offset: f64,
    /// Load factor (0.0–1.0), scales current and power readings.
    pub load_factor: f64,
    /// Voltage baseline offset for this zone (added to all voltage sensors).
    pub voltage_offset: f64,
    /// Base phase offset for the daily cycle (devices add their own per-device offset).
    pub phase_base: f64,
}

impl EnvironmentContext {
    fn new(tag_name: &str, rng: &mut impl Rng) -> Self {
        Self {
            tag_name: tag_name.to_string(),
            temperature_offset: rng.gen_range(-3.0..3.0),
            load_factor: rng.gen_range(-0.15..0.15),
            voltage_offset: rng.gen_range(-5.0..5.0),
            phase_base: rng.gen_range(0.0..1.0),
        }
    }
}

/// Global singleton managing per-tag environment contexts.
///
/// All SimulatedDriver instances within the same process share this state,
/// so devices with the same tag automatically share correlated behavior.
static CORRELATION_MANAGER: LazyLock<Mutex<CorrelationManager>> =
    LazyLock::new(|| Mutex::new(CorrelationManager::new()));

pub struct CorrelationManager {
    contexts: HashMap<String, EnvironmentContext>,
    /// Compiled regex patterns for tag filtering (from config).
    tag_pattern: Option<Regex>,
}

impl CorrelationManager {
    fn new() -> Self {
        Self {
            contexts: HashMap::new(),
            tag_pattern: None,
        }
    }

    /// Get or create an EnvironmentContext for the given tag name.
    pub fn get_or_create(tag_name: &str, rng: &mut impl Rng) -> EnvironmentContext {
        let mut manager = CORRELATION_MANAGER.lock();
        manager.get_or_create_inner(tag_name, rng)
    }

    fn get_or_create_inner(
        &mut self,
        tag_name: &str,
        rng: &mut impl Rng,
    ) -> EnvironmentContext {
        if let Some(ctx) = self.contexts.get(tag_name) {
            return ctx.clone();
        }
        let ctx = EnvironmentContext::new(tag_name, rng);
        self.contexts.insert(tag_name.to_string(), ctx.clone());
        ctx
    }

    /// Set the tag filter pattern from config string.
    /// Pass empty string to disable correlation entirely.
    pub fn set_pattern(pattern: &str) {
        let mut manager = CORRELATION_MANAGER.lock();
        manager.set_pattern_inner(pattern);
    }

    fn set_pattern_inner(&mut self, pattern: &str) {
        if pattern.is_empty() {
            self.tag_pattern = None;
            return;
        }
        if pattern == "*" {
            self.tag_pattern = None; // None means "match all"
            return;
        }
        // Convert glob-like pattern to regex: escape regex specials, replace * with .*
        let escaped = regex::escape(pattern);
        let regex_str = format!("^{}$", escaped.replace(r"\*", ".*"));
        self.tag_pattern = Regex::new(&regex_str).ok();
    }

    /// Filter tag names through the configured pattern.
    /// Returns true if this tag should participate in correlation.
    pub fn tag_matches(tag_name: &str) -> bool {
        let manager = CORRELATION_MANAGER.lock();
        match &manager.tag_pattern {
            None => true,  // No pattern set → match all
            Some(re) => re.is_match(tag_name),
        }
    }
}

/// Given a device's tags (as JSON Values), extract the environment contexts
/// for all matching tags.
///
/// Returns a list of EnvironmentContext references (one per matching tag).
/// The caller (signal composer) averages or sums their contributions.
pub fn get_contexts_for_device(
    tags: &[serde_json::Value],
    rng: &mut impl Rng,
) -> Vec<EnvironmentContext> {
    tags.iter()
        .filter_map(|tag_json| {
            let tag_name = tag_json.get("name")?.as_str()?;
            if CorrelationManager::tag_matches(tag_name) {
                Some(CorrelationManager::get_or_create(tag_name, rng))
            } else {
                None
            }
        })
        .collect()
}

/// Merge multiple EnvironmentContexts by averaging their contributions.
/// Returns a single context that represents the combined effect.
pub fn merge_contexts(contexts: &[EnvironmentContext]) -> EnvironmentContext {
    if contexts.is_empty() {
        return EnvironmentContext {
            tag_name: String::new(),
            temperature_offset: 0.0,
            load_factor: 0.0,
            voltage_offset: 0.0,
            phase_base: 0.0,
        };
    }
    let n = contexts.len() as f64;
    EnvironmentContext {
        tag_name: contexts
            .iter()
            .map(|c| c.tag_name.as_str())
            .collect::<Vec<_>>()
            .join(","),
        temperature_offset: contexts.iter().map(|c| c.temperature_offset).sum::<f64>() / n,
        load_factor: contexts.iter().map(|c| c.load_factor).sum::<f64>() / n,
        voltage_offset: contexts.iter().map(|c| c.voltage_offset).sum::<f64>() / n,
        phase_base: contexts.iter().map(|c| c.phase_base).sum::<f64>() / n,
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn test_same_tag_same_context() {
        let mut rng = StdRng::seed_from_u64(42);
        let ctx1 = CorrelationManager::get_or_create("workshop_A", &mut rng);
        let ctx2 = CorrelationManager::get_or_create("workshop_A", &mut rng);
        assert!((ctx1.temperature_offset - ctx2.temperature_offset).abs() < f64::EPSILON);
        assert!((ctx1.phase_base - ctx2.phase_base).abs() < f64::EPSILON);
    }

    #[test]
    fn test_different_tag_different_context() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let ctx1 = CorrelationManager::get_or_create("workshop_A", &mut rng);
        let ctx2 = CorrelationManager::get_or_create("workshop_B", &mut rng2);
        // Same seed should give same random values for first call, different tag = different creation
        assert!(
            (ctx1.temperature_offset - ctx2.temperature_offset).abs() > f64::EPSILON
                || (ctx1.phase_base - ctx2.phase_base).abs() > f64::EPSILON,
            "different tags should produce different contexts with same seed"
        );
    }

    #[test]
    fn test_empty_pattern_disables() {
        CorrelationManager::set_pattern("");
        let manager = CORRELATION_MANAGER.lock();
        assert!(manager.tag_pattern.is_none());
    }

    #[test]
    fn test_glob_pattern_matches() {
        CorrelationManager::set_pattern("area_*");
        assert!(CorrelationManager::tag_matches("area_workshop"));
        assert!(CorrelationManager::tag_matches("area_lab"));
        // Reset
        CorrelationManager::set_pattern("*");
    }

    #[test]
    fn test_merge_contexts_averages() {
        let ctxs = vec![
            EnvironmentContext {
                tag_name: "a".into(),
                temperature_offset: 2.0,
                load_factor: 0.1,
                voltage_offset: 3.0,
                phase_base: 0.2,
            },
            EnvironmentContext {
                tag_name: "b".into(),
                temperature_offset: 4.0,
                load_factor: -0.1,
                voltage_offset: -3.0,
                phase_base: 0.8,
            },
        ];
        let merged = merge_contexts(&ctxs);
        assert!((merged.temperature_offset - 3.0).abs() < f64::EPSILON); // (2+4)/2
        assert!((merged.load_factor - 0.0).abs() < f64::EPSILON); // (0.1 + -0.1)/2
        assert!((merged.voltage_offset - 0.0).abs() < f64::EPSILON); // (3 + -3)/2
        assert!((merged.phase_base - 0.5).abs() < f64::EPSILON); // (0.2+0.8)/2
    }
}
```

- [ ] **Step 2: Run tests to verify correlation**

Run: `cargo test -p tinyiothub-runtime simulated::correlation -- --nocapture 2>&1`
Expected: All 5 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/tinyiothub-runtime/src/driver/drivers/simulated/correlation.rs
git commit -m "feat: add tag-based device correlation via EnvironmentContext"
```

---

### Task 6: Refactor `simulated_driver.rs` to wire everything together

**Files:**
- Modify: `crates/tinyiothub-runtime/src/driver/drivers/simulated_driver.rs`

This is the core integration task. Replace the old monolithic implementation with the new pipeline.

- [ ] **Step 1: Rewrite simulated_driver.rs**

File: `crates/tinyiothub-runtime/src/driver/drivers/simulated_driver.rs`
```rust
use std::collections::HashMap;
use std::time::Instant;
use tinyiothub_core::models::{device::Device, device_command::DeviceCommand};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tinyiothub_core::driver::{BackoffStrategy, DeviceDriver, ResultValue, RetryConfig};
use tinyiothub_core::error::Error;

use super::simulated::anomaly::AnomalyEngine;
use super::simulated::correlation::{self, EnvironmentContext};
use super::simulated::patterns::match_property;
use super::simulated::signal::SignalComposer;

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
    /// Per-property anomaly engines.
    anomaly_engines: HashMap<String, AnomalyEngine>,
    /// Per-device random phase offset (ensures devices don't all peak at the same tick).
    phase_offset: f64,
    /// Merged environment context from device tags.
    group_context: Option<EnvironmentContext>,
}

impl SimulatedDriver {
    pub fn new(device: Device) -> Self {
        let mut rng = StdRng::from_entropy();
        let phase_offset = rng.gen_range(0.0..1.0);

        // Build group context from device tags if available
        let group_context = device
            .tags
            .as_ref()
            .map(|tags| {
                let contexts = correlation::get_contexts_for_device(tags, &mut rng);
                correlation::merge_contexts(&contexts)
            });

        Self {
            device,
            retry_count: 0,
            tick_counter: 0,
            rng,
            last_read: Instant::now(),
            cached_values: None,
            anomaly_engines: HashMap::new(),
            phase_offset,
            group_context,
        }
    }

    /// Build the SignalComposer from current driver config.
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

    /// Get or create an AnomalyEngine for a property.
    fn get_anomaly_engine(&mut self, prop_name: &str) -> &mut AnomalyEngine {
        self.anomaly_engines
            .entry(prop_name.to_string())
            .or_insert_with(AnomalyEngine::with_defaults)
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
        // Respect the configured interval
        let interval_ms = self.get_config_number("interval", 1000.0) as u64;
        let elapsed = self.last_read.elapsed().as_millis() as u64;
        if elapsed < interval_ms {
            if let Some(ref cached) = self.cached_values {
                return Ok(cached.clone());
            }
        }

        self.tick_counter = self.tick_counter.wrapping_add(1);
        self.last_read = Instant::now();

        // Set up correlation tag pattern from config
        let correlation_tags = self.get_config_string("correlation_tags", "*");
        correlation::CorrelationManager::set_pattern(&correlation_tags);

        let simulation_mode = self.get_config_string("mode", "random");
        let enable_anomaly = self.get_config_boolean("enable_anomaly", true);
        let anomaly_probability = self.get_config_number("anomaly_probability", 0.02);

        let composer = self.build_composer();
        let group_ctx = self.group_context.as_ref();

        let mut results = Vec::new();

        // Collect property info first to avoid borrow conflicts
        let property_infos: Vec<(String, Option<String>, Option<f64>, Option<f64>, Option<String>)> =
            self.device
                .properties
                .as_ref()
                .map(|props| {
                    props
                        .iter()
                        .map(|p| {
                            (
                                p.name.clone(),
                                p.data_type.clone(),
                                p.min_value,
                                p.max_value,
                                p.default_value.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();

        if !property_infos.is_empty() {
            // Apply correlation tags pattern right before processing
            let correlation_tags = self.get_config_string("correlation_tags", "*");
            correlation::CorrelationManager::set_pattern(&correlation_tags);

            for (prop_name, prop_data_type, min_val, max_val, default_val) in property_infos.iter() {
                let data_type = prop_data_type.as_deref().unwrap_or("string");
                let behavior = match_property(&prop_name, data_type);

                if behavior.discrete {
                    // Discrete properties: boolean/status/switch
                    let value = if simulation_mode == "fixed" {
                        matches!(data_type, "bool" | "boolean")
                    } else {
                        // Toggle occasionally (every ~7 ticks on average)
                        self.rng.gen_ratio(1, 7)
                    };
                    results.push(ResultValue::boolean(prop_name.clone(), value));
                } else if behavior.cumulative {
                    // Cumulative properties: always increasing
                    let increment = self.rng.gen_range(0.01..0.5);
                    results.push(ResultValue::float_with_precision(
                        prop_name.clone(),
                        self.tick_counter as f64 * increment,
                        2,
                    ));
                } else {
                    // Continuous properties: signal composition pipeline
                    let normal_value = composer.compose(
                        &behavior,
                        self.tick_counter,
                        self.phase_offset,
                        &mut self.rng,
                        group_ctx,
                    );

                    // Anomaly injection
                    let anomaly_engine = self.get_anomaly_engine(&prop_name);
                    anomaly_engine.enabled = enable_anomaly;
                    anomaly_engine.probability_scale = anomaly_probability / 0.05; // normalize to default 5%

                    // Check for stuck (frozen value)
                    let value = if let Some(frozen) = anomaly_engine.frozen_value() {
                        frozen
                    } else {
                        let anomaly_offset = anomaly_engine.tick(normal_value, &mut self.rng);
                        normal_value + anomaly_offset
                    };

                    // Clamp to property's min/max if defined
                    let value = if let Some(min) = min_val {
                        value.max(*min)
                    } else {
                        value
                    };
                    let value = if let Some(max) = max_val {
                        value.min(*max)
                    } else {
                        value
                    };

                    // Determine precision: use behavior hints or default to 2
                    let precision = match behavior.unit.as_str() {
                        "MPa" => 3,
                        "%" if data_type == "float" => 1,
                        _ => 2,
                    };

                    results.push(ResultValue::float_with_precision(
                        prop_name.clone(),
                        value,
                        precision,
                    ));
                }
            }
        } else {
            // No properties defined — fallback synthetic values
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
    use tinyiothub_core::models::device_property::DeviceProperty;

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
        assert_eq!(
            default_config.get("anomaly_probability"),
            Some(&"0.02".to_string())
        );
        // New options
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
        assert_eq!(default_config.get("noise_level"), Some(&"1.0".to_string()));
        assert_eq!(default_config.get("drift_rate"), Some(&"0.0".to_string()));
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
                    "Consecutive values should change gradually, got change {}",
                    change
                );
            }
            prev_temp = Some(temp);
        }
    }

    #[test]
    fn test_fixed_mode_unchanged() {
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
    }

    #[test]
    fn test_anomaly_disabled_no_spikes() {
        let mut device = create_device_with_temp_property();
        device.driver_options =
            Some(r#"{"enable_anomaly": false, "enable_periodic": false, "enable_noise": false}"#.to_string());
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

        // With no noise and no anomaly, the only change source is drift (default 0)
        // and periodic (disabled), so values should be nearly constant
        assert!(
            max_change < 2.0,
            "With all variation disabled, changes should be minimal, max change was {}",
            max_change
        );
    }

    #[test]
    fn test_multiple_property_types() {
        use tinyiothub_core::models::device_property::DeviceProperty;

        let device = Device {
            id: "multi-props".to_string(),
            name: "Multi Prop Device".to_string(),
            driver_name: Some("SimulatedDriver".to_string()),
            driver_options: Some(
                r#"{"enable_anomaly": false}"#.to_string(),
            ),
            protocol_type: Some("simulation".to_string()),
            properties: Some(vec![
                DeviceProperty {
                    id: "p1".to_string(),
                    device_id: "multi-props".to_string(),
                    name: "temperature".to_string(),
                    data_type: Some("float".to_string()),
                    ..Default::default()
                },
                DeviceProperty {
                    id: "p2".to_string(),
                    device_id: "multi-props".to_string(),
                    name: "current_phase_a".to_string(),
                    data_type: Some("float".to_string()),
                    ..Default::default()
                },
                DeviceProperty {
                    id: "p3".to_string(),
                    device_id: "multi-props".to_string(),
                    name: "power_status".to_string(),
                    data_type: Some("boolean".to_string()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let mut driver = SimulatedDriver::new(device);
        let values = driver.read_data().unwrap();

        assert_eq!(values.len(), 3);
        // temperature should be a float
        let temp = values.iter().find(|v| v.name == "temperature").unwrap();
        assert!(temp.value.as_ref().unwrap().parse::<f64>().is_ok());
        // current should be a float
        let current = values.iter().find(|v| v.name == "current_phase_a").unwrap();
        assert!(current.value.as_ref().unwrap().parse::<f64>().is_ok());
        // power_status should be boolean
        let power = values.iter().find(|v| v.name == "power_status").unwrap();
        assert!(power.value.as_ref().unwrap() == "true" || power.value.as_ref().unwrap() == "false");
    }

    #[test]
    fn test_correlation_tags_is_respected() {
        // With correlation_tags="", group context should not affect values
        let mut device = create_device_with_temp_property();
        device.driver_options = Some(
            r#"{"correlation_tags": "", "enable_anomaly": false, "enable_periodic": false, "enable_noise": false}"#
                .to_string(),
        );
        device.tags = Some(vec![serde_json::json!({"id": "t1", "name": "workshop_A", "type": "device"})]);

        let mut driver = SimulatedDriver::new(device);
        let values = driver.read_data().unwrap();
        let temp = values
            .iter()
            .find(|v| v.name == "temperature")
            .and_then(|v| v.value.as_ref()?.parse::<f64>().ok())
            .unwrap();

        // Should be close to baseline (25.0) since all variation is off
        assert!((temp - 25.0).abs() < 1.0, "temp={temp}, expected near 25.0");
    }
}
```

- [ ] **Step 2: Build to check for compilation errors**

Run: `cargo build -p tinyiothub-runtime 2>&1`
Expected: Compiles successfully

- [ ] **Step 3: Run all simulated driver tests**

Run: `cargo test -p tinyiothub-runtime simulated_driver -- --nocapture 2>&1`
Expected: All 7 tests pass

- [ ] **Step 4: Run the full test suite to check for regressions**

Run: `cargo test -p tinyiothub-runtime -- --nocapture 2>&1`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/tinyiothub-runtime/src/driver/drivers/simulated_driver.rs
git commit -m "feat: refactor SimulatedDriver to use pattern matching, signal composition, and anomaly engine"
```

---

### Task 7: Integration verification — run full test suite and build

**Files:** None (verification only)

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build 2>&1`
Expected: Compiles successfully, no warnings

- [ ] **Step 2: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 3: Run clippy and fix any warnings**

Run: `cargo clippy --all-targets -p tinyiothub-runtime 2>&1`
Expected: No warnings

- [ ] **Step 4: Commit (if any clippy fixes needed)**

```bash
git add -u
git commit -m "chore: fix clippy warnings from simulated driver upgrade"
```

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

    // Status / state — discrete (checked before sensor types so "power_status" is discrete)
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

    // Switch / relay — boolean that holds state (checked before sensor types)
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

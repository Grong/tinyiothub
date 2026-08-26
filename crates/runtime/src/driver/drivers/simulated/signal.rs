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
    use rand::SeedableRng;
    use rand::rngs::StdRng;

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

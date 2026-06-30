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
            let direction = if rng.r#gen::<bool>() { 1.0 } else { -1.0 };
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
            let direction = if rng.r#gen::<bool>() { 1.0 } else { -1.0 };
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
            let direction = if rng.r#gen::<bool>() { 1.0 } else { -1.0 };
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
        // Disable new anomalies so we can observe recovery
        engine.enabled = false;
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

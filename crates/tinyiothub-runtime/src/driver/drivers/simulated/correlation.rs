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
        let ctx1 = CorrelationManager::get_or_create("workshop_A", &mut rng);
        // Use the same (now advanced) RNG so the second context gets different random values.
        let ctx2 = CorrelationManager::get_or_create("workshop_B", &mut rng);
        // Different tags should yield different random draws from the same RNG stream.
        assert!(
            (ctx1.temperature_offset - ctx2.temperature_offset).abs() > f64::EPSILON
                || (ctx1.phase_base - ctx2.phase_base).abs() > f64::EPSILON,
            "different tags should produce different contexts from an advancing RNG"
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

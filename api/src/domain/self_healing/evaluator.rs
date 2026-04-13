
use super::entity::{
    LevelPolicy, ProbeFinding, ProbeResult, SelfHealingPolicy,
    SeverityLevel,
};

/// Evaluates probe results against self-healing policies
pub struct PolicyEvaluator {
    policy: SelfHealingPolicy,
}

impl PolicyEvaluator {
    /// Create a new PolicyEvaluator with the given policy
    pub fn new(policy: SelfHealingPolicy) -> Self {
        Self { policy }
    }

    /// Create a PolicyEvaluator with a default policy
    pub fn with_default_policy() -> Self {
        Self {
            policy: SelfHealingPolicy::default(),
        }
    }

    /// Evaluate a probe result and return the appropriate severity level
    ///
    /// Returns L0 if the policy is disabled or no findings exceed thresholds.
    /// Computes the highest severity from all findings using `.max()`.
    pub fn evaluate(&self, probe_result: &ProbeResult) -> SeverityLevel {
        if !self.policy.enabled {
            return SeverityLevel::L0;
        }

        // If healthy and no findings, return L0
        if probe_result.healthy && probe_result.findings.is_empty() {
            return SeverityLevel::L0;
        }

        // Compute highest severity from findings using Ord derive
        // This relies on SeverityLevel deriving PartialOrd and Ord
        probe_result
            .findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(SeverityLevel::L0)
    }

    /// Get the policy configuration for a specific severity level
    pub fn get_level_policy(&self, level: SeverityLevel) -> Option<&LevelPolicy> {
        self.policy.levels.get(&level)
    }

    /// Check if conditions are met for a given severity level and findings
    pub fn check_conditions(&self, level: SeverityLevel, findings: &[ProbeFinding]) -> bool {
        let Some(policy) = self.get_level_policy(level) else {
            return false;
        };

        // If no conditions are defined, always pass
        if policy.conditions.is_empty() {
            return true;
        }

        // Check each condition
        for condition in &policy.conditions {
            let matching_findings: Vec<&ProbeFinding> = findings
                .iter()
                .filter(|f| f.finding_type == condition.condition_type)
                .collect();

            if matching_findings.is_empty() {
                return false;
            }

            // Check if count threshold is met
            if matching_findings.len() < condition.count as usize {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_test_policy() -> SelfHealingPolicy {
        let mut levels = HashMap::new();

        // L1 policy with conditions
        levels.insert(
            SeverityLevel::L1,
            LevelPolicy {
                level: SeverityLevel::L1,
                actions: vec![],
                conditions: vec![HealingCondition {
                    condition_type: "HighCpu".to_string(),
                    threshold: 80.0,
                    count: 2,
                }],
                require_approval: false,
                cooldown_secs: 300,
            },
        );

        SelfHealingPolicy {
            enabled: true,
            levels,
        }
    }

    fn create_probe_result(severity: SeverityLevel, findings: Vec<ProbeFinding>) -> ProbeResult {
        ProbeResult {
            probe_type: ProbeType::System,
            timestamp: Utc::now(),
            healthy: severity == SeverityLevel::L0,
            severity,
            findings,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_evaluate_disabled_policy_returns_l0() {
        let policy = SelfHealingPolicy {
            enabled: false,
            levels: HashMap::new(),
        };
        let evaluator = PolicyEvaluator::new(policy);

        let probe_result = create_probe_result(
            SeverityLevel::L3,
            vec![ProbeFinding {
                finding_type: "HighCpu".to_string(),
                severity: SeverityLevel::L3,
                message: "CPU usage high".to_string(),
                target: "gateway-1".to_string(),
                value: Some("95%".to_string()),
            }],
        );

        assert_eq!(evaluator.evaluate(&probe_result), SeverityLevel::L0);
    }

    #[test]
    fn test_evaluate_healthy_probe_returns_l0() {
        let evaluator = PolicyEvaluator::with_default_policy();

        let probe_result = create_probe_result(SeverityLevel::L0, vec![]);
        assert_eq!(evaluator.evaluate(&probe_result), SeverityLevel::L0);
    }

    #[test]
    fn test_evaluate_highest_severity_from_findings() {
        let evaluator = PolicyEvaluator::with_default_policy();

        let probe_result = create_probe_result(
            SeverityLevel::L0,
            vec![
                ProbeFinding {
                    finding_type: "HighCpu".to_string(),
                    severity: SeverityLevel::L1,
                    message: "CPU high".to_string(),
                    target: "gw1".to_string(),
                    value: None,
                },
                ProbeFinding {
                    finding_type: "HighMemory".to_string(),
                    severity: SeverityLevel::L3,
                    message: "Memory high".to_string(),
                    target: "gw1".to_string(),
                    value: None,
                },
                ProbeFinding {
                    finding_type: "DiskFull".to_string(),
                    severity: SeverityLevel::L2,
                    message: "Disk full".to_string(),
                    target: "gw1".to_string(),
                    value: None,
                },
            ],
        );

        // Should return L3 as highest severity
        assert_eq!(evaluator.evaluate(&probe_result), SeverityLevel::L3);
    }

    #[test]
    fn test_get_level_policy() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let l1_policy = evaluator.get_level_policy(SeverityLevel::L1);
        assert!(l1_policy.is_some());
        assert_eq!(l1_policy.unwrap().level, SeverityLevel::L1);

        let l3_policy = evaluator.get_level_policy(SeverityLevel::L3);
        assert!(l3_policy.is_none());
    }

    #[test]
    fn test_check_conditions_empty_conditions_passes() {
        // Create a policy with L0 having no conditions
        let mut levels = HashMap::new();
        levels.insert(
            SeverityLevel::L0,
            LevelPolicy {
                level: SeverityLevel::L0,
                actions: vec![],
                conditions: vec![], // Empty conditions
                require_approval: false,
                cooldown_secs: 0,
            },
        );
        let policy = SelfHealingPolicy {
            enabled: true,
            levels,
        };
        let evaluator = PolicyEvaluator::new(policy);

        // Empty conditions should pass
        assert!(evaluator.check_conditions(SeverityLevel::L0, &[]));
    }

    #[test]
    fn test_check_conditions_no_matching_findings() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let findings = vec![ProbeFinding {
            finding_type: "UnknownType".to_string(),
            severity: SeverityLevel::L1,
            message: "Unknown".to_string(),
            target: "gw1".to_string(),
            value: None,
        }];

        assert!(!evaluator.check_conditions(SeverityLevel::L1, &findings));
    }

    #[test]
    fn test_check_conditions_count_threshold_met() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let findings = vec![
            ProbeFinding {
                finding_type: "HighCpu".to_string(),
                severity: SeverityLevel::L1,
                message: "CPU high 1".to_string(),
                target: "gw1".to_string(),
                value: None,
            },
            ProbeFinding {
                finding_type: "HighCpu".to_string(),
                severity: SeverityLevel::L1,
                message: "CPU high 2".to_string(),
                target: "gw1".to_string(),
                value: None,
            },
        ];

        // Count threshold is 2, we have 2 matching findings
        assert!(evaluator.check_conditions(SeverityLevel::L1, &findings));
    }

    #[test]
    fn test_check_conditions_count_threshold_not_met() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let findings = vec![ProbeFinding {
            finding_type: "HighCpu".to_string(),
            severity: SeverityLevel::L1,
            message: "CPU high".to_string(),
            target: "gw1".to_string(),
            value: None,
        }];

        // Count threshold is 2, but we only have 1
        assert!(!evaluator.check_conditions(SeverityLevel::L1, &findings));
    }

    #[test]
    fn test_severity_level_ord_derives_correctly() {
        use std::cmp::Ordering;

        assert_eq!(SeverityLevel::L0.cmp(&SeverityLevel::L0), Ordering::Equal);
        assert_eq!(SeverityLevel::L0.cmp(&SeverityLevel::L1), Ordering::Less);
        assert_eq!(SeverityLevel::L3.cmp(&SeverityLevel::L1), Ordering::Greater);

        assert!(SeverityLevel::L0 < SeverityLevel::L1);
        assert!(SeverityLevel::L1 < SeverityLevel::L2);
        assert!(SeverityLevel::L2 < SeverityLevel::L3);
    }

    #[test]
    fn test_severity_level_max() {
        let levels = vec![
            SeverityLevel::L1,
            SeverityLevel::L3,
            SeverityLevel::L0,
            SeverityLevel::L2,
        ];

        let max = levels.iter().copied().max().unwrap();
        assert_eq!(max, SeverityLevel::L3);
    }
}

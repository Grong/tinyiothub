// Self-healing service: evaluator + executor
// Migrated from domain/self_healing/evaluator.rs + executor.rs

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::{
    ExecutionResult, HealingExecution, LevelPolicy, ProbeFinding, ProbeResult,
    RecoveryActionType, SelfHealingPolicy, SeverityLevel,
};
use super::errors::{Result, SelfHealingError};

// ════════════════════════════════════════════════
// Policy Evaluator (from evaluator.rs)
// ════════════════════════════════════════════════

/// Evaluates probe results against self-healing policies
pub struct PolicyEvaluator {
    policy: SelfHealingPolicy,
}

impl PolicyEvaluator {
    pub fn new(policy: SelfHealingPolicy) -> Self {
        Self { policy }
    }

    pub fn with_default_policy() -> Self {
        Self { policy: SelfHealingPolicy::default() }
    }

    pub fn evaluate(&self, probe_result: &ProbeResult) -> SeverityLevel {
        if !self.policy.enabled {
            return SeverityLevel::L0;
        }
        if probe_result.healthy && probe_result.findings.is_empty() {
            return SeverityLevel::L0;
        }
        probe_result.findings.iter().map(|f| f.severity).max().unwrap_or(SeverityLevel::L0)
    }

    pub fn get_level_policy(&self, level: SeverityLevel) -> Option<&LevelPolicy> {
        self.policy.levels.get(&level)
    }

    pub fn check_conditions(&self, level: SeverityLevel, findings: &[ProbeFinding]) -> bool {
        let Some(policy) = self.get_level_policy(level) else {
            return false;
        };
        if policy.conditions.is_empty() {
            return true;
        }
        for condition in &policy.conditions {
            let matching_count = findings
                .iter()
                .filter(|f| f.finding_type == condition.condition_type)
                .count();
            if matching_count < condition.count as usize {
                return false;
            }
        }
        true
    }
}

// ════════════════════════════════════════════════
// Action Executor (from executor.rs)
// ════════════════════════════════════════════════

/// Executes recovery actions based on policy
pub struct ActionExecutor {
    last_execution: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl ActionExecutor {
    pub fn new() -> Self {
        Self { last_execution: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn execute(
        &self,
        level: SeverityLevel,
        action_type: RecoveryActionType,
        target: String,
        cooldown_secs: u64,
    ) -> Result<HealingExecution> {
        let key = format!("{}:{}:{}", target, action_type.as_str(), level.as_str());
        {
            let last_exec = self.last_execution.read().await;
            if let Some(last_time) = last_exec.get(&key) {
                let elapsed = Utc::now().signed_duration_since(*last_time);
                if elapsed.num_seconds() < cooldown_secs as i64 {
                    return Err(SelfHealingError::CooldownActive(format!(
                        "{} ({}s remaining)",
                        target,
                        cooldown_secs - elapsed.num_seconds() as u64
                    )));
                }
            }
        }
        {
            let mut last_exec = self.last_execution.write().await;
            last_exec.insert(key, Utc::now());
        }

        let execution = match action_type {
            RecoveryActionType::RestartDriver => {
                tracing::info!("Restarting driver: {}", target);
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
            RecoveryActionType::RejoinLora => {
                tracing::info!("Rejoining LoRa network: {}", target);
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
            RecoveryActionType::ReconnectDevice => {
                tracing::info!("Reconnecting device: {}", target);
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
            RecoveryActionType::CleanLogs => {
                tracing::info!("Cleaning logs: {}", target);
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
            RecoveryActionType::ReportCloud => {
                tracing::info!("Reporting to cloud: {}", target);
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
            RecoveryActionType::CreateTicket => {
                tracing::info!("Creating support ticket: {}", target);
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
            RecoveryActionType::LogOnly => {
                HealingExecution::new("default".to_string(), level, action_type, target, ExecutionResult::Success)
            }
        };

        Ok(execution)
    }
}

impl Default for ActionExecutor {
    fn default() -> Self { Self::new() }
}

// ════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{HealingCondition, ProbeType};
    use std::collections::HashMap;

    fn create_test_policy() -> SelfHealingPolicy {
        let mut levels = HashMap::new();
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
        SelfHealingPolicy { enabled: true, levels }
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

    // ── Evaluator tests ──

    #[test]
    fn test_evaluate_disabled_policy_returns_l0() {
        let policy = SelfHealingPolicy { enabled: false, levels: HashMap::new() };
        let evaluator = PolicyEvaluator::new(policy);
        let probe_result = create_probe_result(SeverityLevel::L3, vec![ProbeFinding {
            finding_type: "HighCpu".to_string(),
            severity: SeverityLevel::L3,
            message: "CPU usage high".to_string(),
            target: "gateway-1".to_string(),
            value: Some("95%".to_string()),
        }]);
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
        let probe_result = create_probe_result(SeverityLevel::L0, vec![
            ProbeFinding { finding_type: "HighCpu".to_string(), severity: SeverityLevel::L1, message: "CPU high".to_string(), target: "gw1".to_string(), value: None },
            ProbeFinding { finding_type: "HighMemory".to_string(), severity: SeverityLevel::L3, message: "Memory high".to_string(), target: "gw1".to_string(), value: None },
            ProbeFinding { finding_type: "DiskFull".to_string(), severity: SeverityLevel::L2, message: "Disk full".to_string(), target: "gw1".to_string(), value: None },
        ]);
        assert_eq!(evaluator.evaluate(&probe_result), SeverityLevel::L3);
    }

    #[test]
    fn test_check_conditions_count_threshold_met() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);
        let findings = vec![
            ProbeFinding { finding_type: "HighCpu".to_string(), severity: SeverityLevel::L1, message: "CPU high 1".to_string(), target: "gw1".to_string(), value: None },
            ProbeFinding { finding_type: "HighCpu".to_string(), severity: SeverityLevel::L1, message: "CPU high 2".to_string(), target: "gw1".to_string(), value: None },
        ];
        assert!(evaluator.check_conditions(SeverityLevel::L1, &findings));
    }

    #[test]
    fn test_check_conditions_count_threshold_not_met() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);
        let findings = vec![ProbeFinding {
            finding_type: "HighCpu".to_string(), severity: SeverityLevel::L1, message: "CPU high".to_string(), target: "gw1".to_string(), value: None,
        }];
        assert!(!evaluator.check_conditions(SeverityLevel::L1, &findings));
    }

    // ── Executor tests ──

    #[tokio::test]
    async fn test_execute_with_cooldown_blocks() {
        let executor = ActionExecutor::new();
        let result = executor.execute(SeverityLevel::L1, RecoveryActionType::RestartDriver, "driver-1".to_string(), 300).await;
        assert!(result.is_ok());
        let result = executor.execute(SeverityLevel::L1, RecoveryActionType::RestartDriver, "driver-1".to_string(), 300).await;
        assert!(matches!(result, Err(SelfHealingError::CooldownActive(_))));
    }

    #[tokio::test]
    async fn test_execute_different_targets_no_cooldown() {
        let executor = ActionExecutor::new();
        let result = executor.execute(SeverityLevel::L1, RecoveryActionType::RestartDriver, "driver-1".to_string(), 300).await;
        assert!(result.is_ok());
        let result = executor.execute(SeverityLevel::L1, RecoveryActionType::RestartDriver, "driver-2".to_string(), 300).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_log_only() {
        let executor = ActionExecutor::new();
        let result = executor.execute(SeverityLevel::L0, RecoveryActionType::LogOnly, "gateway-1".to_string(), 0).await;
        assert!(result.is_ok());
        let execution = result.unwrap();
        assert_eq!(execution.result, ExecutionResult::Success);
    }
}

// Self-healing entity types
// Migrated from domain/self_healing/entity.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SeverityLevel {
    L0,
    L1,
    L2,
    L3,
}

impl SeverityLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeverityLevel::L0 => "L0",
            SeverityLevel::L1 => "L1",
            SeverityLevel::L2 => "L2",
            SeverityLevel::L3 => "L3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryActionType {
    LogOnly,
    RestartDriver,
    RejoinLora,
    ReconnectDevice,
    CleanLogs,
    ReportCloud,
    CreateTicket,
}

impl RecoveryActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecoveryActionType::LogOnly => "logOnly",
            RecoveryActionType::RestartDriver => "restartDriver",
            RecoveryActionType::RejoinLora => "rejoinLora",
            RecoveryActionType::ReconnectDevice => "reconnectDevice",
            RecoveryActionType::CleanLogs => "cleanLogs",
            RecoveryActionType::ReportCloud => "reportCloud",
            RecoveryActionType::CreateTicket => "createTicket",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub action_type: RecoveryActionType,
    pub target: String,
    pub max_retries: u32,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingCondition {
    pub condition_type: String,
    pub threshold: f64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelPolicy {
    pub level: SeverityLevel,
    pub actions: Vec<RecoveryAction>,
    pub conditions: Vec<HealingCondition>,
    pub require_approval: bool,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingPolicy {
    pub enabled: bool,
    pub levels: HashMap<SeverityLevel, LevelPolicy>,
}

impl Default for SelfHealingPolicy {
    fn default() -> Self {
        Self { enabled: true, levels: HashMap::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ProbeType {
    System,
    Device,
    Task,
}

impl ProbeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProbeType::System => "System",
            ProbeType::Device => "Device",
            ProbeType::Task => "Task",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeFinding {
    pub finding_type: String,
    pub severity: SeverityLevel,
    pub message: String,
    pub target: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub probe_type: ProbeType,
    pub timestamp: DateTime<Utc>,
    pub healthy: bool,
    pub severity: SeverityLevel,
    pub findings: Vec<ProbeFinding>,
    pub metadata: HashMap<String, String>,
}

impl ProbeResult {
    pub fn new(probe_type: ProbeType, healthy: bool, findings: Vec<ProbeFinding>) -> Self {
        let severity = findings.iter().map(|f| f.severity).max().unwrap_or(SeverityLevel::L0);
        Self {
            probe_type,
            timestamp: Utc::now(),
            healthy,
            severity,
            findings,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ExecutionResult {
    Success,
    Failed,
    PendingApproval,
    Skipped,
}

impl ExecutionResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionResult::Success => "Success",
            ExecutionResult::Failed => "Failed",
            ExecutionResult::PendingApproval => "PendingApproval",
            ExecutionResult::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingExecution {
    pub id: String,
    pub tenant_id: String,
    pub timestamp: DateTime<Utc>,
    pub level: SeverityLevel,
    pub action_type: RecoveryActionType,
    pub target: String,
    pub result: ExecutionResult,
    pub logs: Vec<String>,
}

impl HealingExecution {
    pub fn new(
        tenant_id: String,
        level: SeverityLevel,
        action_type: RecoveryActionType,
        target: String,
        result: ExecutionResult,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            timestamp: Utc::now(),
            level,
            action_type,
            target,
            result,
            logs: Vec::new(),
        }
    }
}

// Self-Healing DTO
// DTOs for self-healing API endpoints

/// Recovery action DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryActionDto {
    pub action_type: String,
    pub target: String,
    pub max_retries: u32,
    pub cooldown_secs: u64,
}

impl From<&RecoveryAction> for RecoveryActionDto {
    fn from(action: &RecoveryAction) -> Self {
        Self {
            action_type: action.action_type.as_str().to_string(),
            target: action.target.clone(),
            max_retries: action.max_retries,
            cooldown_secs: action.cooldown_secs,
        }
    }
}

/// Healing condition DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingConditionDto {
    pub condition_type: String,
    pub threshold: f64,
    pub count: u32,
}

impl From<&HealingCondition> for HealingConditionDto {
    fn from(condition: &HealingCondition) -> Self {
        Self {
            condition_type: condition.condition_type.clone(),
            threshold: condition.threshold,
            count: condition.count,
        }
    }
}

/// Level policy DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelPolicyDto {
    pub level: String,
    pub actions: Vec<RecoveryActionDto>,
    pub conditions: Vec<HealingConditionDto>,
    pub require_approval: bool,
    pub cooldown_secs: u64,
}

impl From<&LevelPolicy> for LevelPolicyDto {
    fn from(policy: &LevelPolicy) -> Self {
        Self {
            level: policy.level.as_str().to_string(),
            actions: policy.actions.iter().map(RecoveryActionDto::from).collect(),
            conditions: policy.conditions.iter().map(HealingConditionDto::from).collect(),
            require_approval: policy.require_approval,
            cooldown_secs: policy.cooldown_secs,
        }
    }
}

/// Self-healing policy DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfHealingPolicyDto {
    pub enabled: bool,
    pub levels: HashMap<String, LevelPolicyDto>,
}

impl From<&SelfHealingPolicy> for SelfHealingPolicyDto {
    fn from(policy: &SelfHealingPolicy) -> Self {
        let levels = policy
            .levels
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), LevelPolicyDto::from(v)))
            .collect();

        Self {
            enabled: policy.enabled,
            levels,
        }
    }
}

/// Probe finding DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeFindingDto {
    pub finding_type: String,
    pub severity: String,
    pub message: String,
    pub target: String,
    pub value: Option<String>,
}

impl From<&crate::modules::self_healing::ProbeFinding> for ProbeFindingDto {
    fn from(finding: &crate::modules::self_healing::ProbeFinding) -> Self {
        Self {
            finding_type: finding.finding_type.clone(),
            severity: finding.severity.as_str().to_string(),
            message: finding.message.clone(),
            target: finding.target.clone(),
            value: finding.value.clone(),
        }
    }
}

/// Probe result DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResultDto {
    pub probe_type: String,
    pub timestamp: DateTime<Utc>,
    pub healthy: bool,
    pub severity: String,
    pub findings: Vec<ProbeFindingDto>,
    pub metadata: HashMap<String, String>,
}

impl From<&crate::modules::self_healing::ProbeResult> for ProbeResultDto {
    fn from(result: &crate::modules::self_healing::ProbeResult) -> Self {
        Self {
            probe_type: result.probe_type.as_str().to_string(),
            timestamp: result.timestamp,
            healthy: result.healthy,
            severity: result.severity.as_str().to_string(),
            findings: result.findings.iter().map(ProbeFindingDto::from).collect(),
            metadata: result.metadata.clone(),
        }
    }
}

/// Healing execution DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingExecutionDto {
    pub id: String,
    pub tenant_id: String,
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub action_type: String,
    pub target: String,
    pub result: String,
    pub logs: Vec<String>,
}

impl From<&HealingExecution> for HealingExecutionDto {
    fn from(execution: &HealingExecution) -> Self {
        Self {
            id: execution.id.clone(),
            tenant_id: execution.tenant_id.clone(),
            timestamp: execution.timestamp,
            level: execution.level.as_str().to_string(),
            action_type: execution.action_type.as_str().to_string(),
            target: execution.target.clone(),
            result: execution.result.as_str().to_string(),
            logs: execution.logs.clone(),
        }
    }
}

/// Probe configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeConfig {
    pub system_probe_enabled: bool,
    pub device_probe_enabled: bool,
    pub task_probe_enabled: bool,
    pub system_probe_interval_secs: u64,
    pub device_probe_interval_secs: u64,
    pub task_probe_interval_secs: u64,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            system_probe_enabled: true,
            device_probe_enabled: true,
            task_probe_enabled: true,
            system_probe_interval_secs: 60,
            device_probe_interval_secs: 300,
            task_probe_interval_secs: 300,
        }
    }
}

/// Request to execute self-healing action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSelfHealRequest {
    pub level: String,
    pub action_type: String,
    pub target: String,
    pub cooldown_secs: Option<u64>,
}

/// Response after executing self-healing action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSelfHealResponse {
    pub execution: HealingExecutionDto,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_level_as_str() {
        assert_eq!(SeverityLevel::L0.as_str(), "L0");
        assert_eq!(SeverityLevel::L1.as_str(), "L1");
        assert_eq!(SeverityLevel::L2.as_str(), "L2");
        assert_eq!(SeverityLevel::L3.as_str(), "L3");
    }

    #[test]
    fn test_recovery_action_type_as_str() {
        assert_eq!(RecoveryActionType::LogOnly.as_str(), "logOnly");
        assert_eq!(RecoveryActionType::RestartDriver.as_str(), "restartDriver");
        assert_eq!(RecoveryActionType::RejoinLora.as_str(), "rejoinLora");
        assert_eq!(RecoveryActionType::ReconnectDevice.as_str(), "reconnectDevice");
        assert_eq!(RecoveryActionType::CleanLogs.as_str(), "cleanLogs");
        assert_eq!(RecoveryActionType::ReportCloud.as_str(), "reportCloud");
        assert_eq!(RecoveryActionType::CreateTicket.as_str(), "createTicket");
    }

    #[test]
    fn test_probe_type_as_str() {
        assert_eq!(ProbeType::System.as_str(), "System");
        assert_eq!(ProbeType::Device.as_str(), "Device");
        assert_eq!(ProbeType::Task.as_str(), "Task");
    }

    #[test]
    fn test_execution_result_as_str() {
        assert_eq!(ExecutionResult::Success.as_str(), "Success");
        assert_eq!(ExecutionResult::Failed.as_str(), "Failed");
        assert_eq!(ExecutionResult::PendingApproval.as_str(), "PendingApproval");
        assert_eq!(ExecutionResult::Skipped.as_str(), "Skipped");
    }

    #[test]
    fn test_probe_result_new_severity_from_findings() {
        let findings = vec![
            ProbeFinding {
                finding_type: "error".to_string(),
                severity: SeverityLevel::L2,
                message: "High CPU".to_string(),
                target: "system".to_string(),
                value: Some("95%".to_string()),
            },
            ProbeFinding {
                finding_type: "error".to_string(),
                severity: SeverityLevel::L1,
                message: "Low memory".to_string(),
                target: "system".to_string(),
                value: Some("10%".to_string()),
            },
        ];
        let result = ProbeResult::new(ProbeType::System, false, findings);
        assert!(!result.healthy);
        assert_eq!(result.severity, SeverityLevel::L2);
        assert_eq!(result.findings.len(), 2);
        assert!(result.metadata.is_empty());
    }

    #[test]
    fn test_probe_result_new_empty_findings() {
        let result = ProbeResult::new(ProbeType::Device, true, vec![]);
        assert!(result.healthy);
        assert_eq!(result.severity, SeverityLevel::L0);
    }

    #[test]
    fn test_healing_execution_new() {
        let exec = HealingExecution::new(
            "tenant-1".to_string(),
            SeverityLevel::L2,
            RecoveryActionType::RestartDriver,
            "device-1".to_string(),
            ExecutionResult::Success,
        );
        assert!(!exec.id.is_empty());
        assert_eq!(exec.tenant_id, "tenant-1");
        assert_eq!(exec.level, SeverityLevel::L2);
        assert_eq!(exec.action_type, RecoveryActionType::RestartDriver);
        assert_eq!(exec.target, "device-1");
        assert_eq!(exec.result, ExecutionResult::Success);
        assert!(exec.logs.is_empty());
    }

    #[test]
    fn test_self_healing_policy_default() {
        let policy = SelfHealingPolicy::default();
        assert!(policy.enabled);
        assert!(policy.levels.is_empty());
    }

    #[test]
    fn test_probe_config_default() {
        let config = ProbeConfig::default();
        assert!(config.system_probe_enabled);
        assert!(config.device_probe_enabled);
        assert!(config.task_probe_enabled);
        assert_eq!(config.system_probe_interval_secs, 60);
        assert_eq!(config.device_probe_interval_secs, 300);
        assert_eq!(config.task_probe_interval_secs, 300);
    }

    #[test]
    fn test_recovery_action_dto_from() {
        let action = RecoveryAction {
            action_type: RecoveryActionType::RestartDriver,
            target: "device-1".to_string(),
            max_retries: 3,
            cooldown_secs: 60,
        };
        let dto = RecoveryActionDto::from(&action);
        assert_eq!(dto.action_type, "restartDriver");
        assert_eq!(dto.target, "device-1");
        assert_eq!(dto.max_retries, 3);
        assert_eq!(dto.cooldown_secs, 60);
    }

    #[test]
    fn test_healing_condition_dto_from() {
        let condition = HealingCondition {
            condition_type: "cpu_usage".to_string(),
            threshold: 90.0,
            count: 3,
        };
        let dto = HealingConditionDto::from(&condition);
        assert_eq!(dto.condition_type, "cpu_usage");
        assert!((dto.threshold - 90.0).abs() < f64::EPSILON);
        assert_eq!(dto.count, 3);
    }

    #[test]
    fn test_level_policy_dto_from() {
        let policy = LevelPolicy {
            level: SeverityLevel::L2,
            actions: vec![RecoveryAction {
                action_type: RecoveryActionType::LogOnly,
                target: "system".to_string(),
                max_retries: 1,
                cooldown_secs: 0,
            }],
            conditions: vec![HealingCondition {
                condition_type: "cpu".to_string(),
                threshold: 95.0,
                count: 1,
            }],
            require_approval: true,
            cooldown_secs: 300,
        };
        let dto = LevelPolicyDto::from(&policy);
        assert_eq!(dto.level, "L2");
        assert_eq!(dto.actions.len(), 1);
        assert_eq!(dto.conditions.len(), 1);
        assert!(dto.require_approval);
        assert_eq!(dto.cooldown_secs, 300);
    }

    #[test]
    fn test_probe_finding_dto_from() {
        let finding = ProbeFinding {
            finding_type: "error".to_string(),
            severity: SeverityLevel::L3,
            message: "Critical".to_string(),
            target: "device-1".to_string(),
            value: None,
        };
        let dto = ProbeFindingDto::from(&finding);
        assert_eq!(dto.finding_type, "error");
        assert_eq!(dto.severity, "L3");
        assert_eq!(dto.message, "Critical");
        assert_eq!(dto.target, "device-1");
        assert_eq!(dto.value, None);
    }

    #[test]
    fn test_probe_result_dto_from() {
        let result = ProbeResult::new(
            ProbeType::Device,
            true,
            vec![ProbeFinding {
                finding_type: "info".to_string(),
                severity: SeverityLevel::L0,
                message: "OK".to_string(),
                target: "device-1".to_string(),
                value: None,
            }],
        );
        let dto = ProbeResultDto::from(&result);
        assert_eq!(dto.probe_type, "Device");
        assert!(dto.healthy);
        assert_eq!(dto.severity, "L0");
        assert_eq!(dto.findings.len(), 1);
    }

    #[test]
    fn test_healing_execution_dto_from() {
        let exec = HealingExecution::new(
            "tenant-1".to_string(),
            SeverityLevel::L1,
            RecoveryActionType::ReconnectDevice,
            "device-1".to_string(),
            ExecutionResult::Failed,
        );
        let dto = HealingExecutionDto::from(&exec);
        assert_eq!(dto.id, exec.id);
        assert_eq!(dto.tenant_id, "tenant-1");
        assert_eq!(dto.level, "L1");
        assert_eq!(dto.action_type, "reconnectDevice");
        assert_eq!(dto.target, "device-1");
        assert_eq!(dto.result, "Failed");
    }

    #[test]
    fn test_self_healing_policy_dto_from() {
        let mut levels = HashMap::new();
        levels.insert(
            SeverityLevel::L1,
            LevelPolicy {
                level: SeverityLevel::L1,
                actions: vec![],
                conditions: vec![],
                require_approval: false,
                cooldown_secs: 60,
            },
        );
        let policy = SelfHealingPolicy { enabled: true, levels };
        let dto = SelfHealingPolicyDto::from(&policy);
        assert!(dto.enabled);
        assert_eq!(dto.levels.len(), 1);
        assert!(dto.levels.contains_key("L1"));
    }
}

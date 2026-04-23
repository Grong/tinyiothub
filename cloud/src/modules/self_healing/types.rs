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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity level for self-healing issues
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

/// Type of recovery action
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

/// A recovery action to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub action_type: RecoveryActionType,
    pub target: String,
    pub max_retries: u32,
    pub cooldown_secs: u64,
}

/// Condition for triggering recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingCondition {
    pub condition_type: String,
    pub threshold: f64,
    pub count: u32,
}

/// Policy for a specific severity level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelPolicy {
    pub level: SeverityLevel,
    pub actions: Vec<RecoveryAction>,
    pub conditions: Vec<HealingCondition>,
    pub require_approval: bool,
    pub cooldown_secs: u64,
}

/// Self-healing policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingPolicy {
    pub enabled: bool,
    pub levels: HashMap<SeverityLevel, LevelPolicy>,
}

impl Default for SelfHealingPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            levels: HashMap::new(),
        }
    }
}

/// Type of probe
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// A finding from a probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeFinding {
    pub finding_type: String,
    pub severity: SeverityLevel,
    pub message: String,
    pub target: String,
    pub value: Option<String>,
}

/// Result from a probe execution
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
        let severity = findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(SeverityLevel::L0);

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

/// Result of executing a healing action
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

/// Record of a healing execution
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

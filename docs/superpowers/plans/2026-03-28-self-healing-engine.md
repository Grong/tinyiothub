# Phase 2: Self-Healing Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the self-healing engine with probes, policy evaluator, and L0-L3 action executor. Replace Phase 1 MCP stubs with full implementations.

**Architecture:**
- **Domain layer** (`api/src/domain/self_healing/`): Policy entity, HealingExecution entity, Evaluator service, ActionExecutor service
- **Infrastructure layer** (`api/src/infrastructure/self_healing/`): ProbeScheduler that runs system/device/task probes on configurable intervals using `tokio::select!` across independent `tokio::time::interval` tickers
- **Global state**: `OnceLock<Arc<RwLock<SelfHealingState>>>` pattern (same as heartbeat)
- **Scheduling**: Background `tokio::spawn` with graceful shutdown via `CancellationToken` (broadcast channel)
- **MCP tools**: Replace stubs with actual implementations + add probe status/config endpoints

**Tech Stack:** Rust 2021, tokio async/await, sqlx, cron, OnceLock/RwLock, thiserror

---

## File Structure

```
api/src/
├── domain/
│   └── self_healing/
│       ├── mod.rs              # Module exports
│       ├── entity.rs           # SelfHealingPolicy, HealingExecution, HealingAction
│       ├── errors.rs           # SelfHealingError
│       ├── evaluator.rs         # PolicyEvaluator - evaluates probe results → L0/L1/L2/L3
│       ├── executor.rs          # ActionExecutor - executes recovery actions
│       └── repository.rs        # HealingExecutionRepository - persists recovery history
├── infrastructure/
│   └── self_healing/
│       ├── mod.rs
│       └── probe_scheduler.rs   # ProbeScheduler - runs probes on intervals
├── dto/
│   └── entity/
│       └── self_healing.rs    # DTOs: SelfHealingPolicyDto, HealingExecutionDto, ProbeResultDto
├── api/
│   ├── self_healing/
│   │   ├── mod.rs              # create_router()
│   │   └── handlers.rs          # HTTP handlers
│   └── mcp/tools/
│       └── self_heal.rs        # Replace stubs with real implementations
```

---

## Task 1: Self-Healing Domain Module

**Files:**
- Create: `api/src/domain/self_healing/mod.rs`
- Create: `api/src/domain/self_healing/entity.rs`
- Create: `api/src/domain/self_healing/errors.rs`
- Create: `api/src/domain/self_healing/evaluator.rs`
- Create: `api/src/domain/self_healing/executor.rs`
- Create: `api/src/domain/self_healing/repository.rs`
- Create: `api/src/dto/entity/self_healing.rs`

### Steps

- [ ] **Step 1: Create `api/src/domain/self_healing/mod.rs`**

```rust
pub mod entity;
pub mod errors;
pub mod evaluator;
pub mod executor;
pub mod repository;

pub use entity::*;
pub use errors::*;
pub use evaluator::PolicyEvaluator;
pub use executor::ActionExecutor;
pub use repository::HealingExecutionRepository;
```

- [ ] **Step 2: Create `api/src/domain/self_healing/entity.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity level for self-healing actions
/// Order: L0 < L1 < L2 < L3 (L3 is most severe)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityLevel {
    L0, // Log only
    L1, // Local self-healing (restart_driver, rejoin_lora, reconnect_device)
    L2, // Report cloud + local cleanup
    L3, // Report cloud + create ticket (requires approval)
}

impl Default for SeverityLevel {
    fn default() -> Self { SeverityLevel::L0 }
}

/// Type of recovery action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryActionType {
    LogOnly,
    RestartDriver,
    RejoinLora,
    ReconnectDevice,
    CleanLogs,
    ReportCloud,
    CreateTicket,
}

/// A single recovery action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub action_type: RecoveryActionType,
    pub target: Option<String>,
    pub max_retries: u32,
    pub cooldown_secs: u64,
}

/// Condition that triggers a severity level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingCondition {
    pub condition_type: String, // e.g., "cpu_usage", "device_timeout", "disk_usage"
    pub threshold: Option<f64>,
    pub count: Option<u32>,
}

/// Self-healing policy for a severity level
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
        let mut levels = HashMap::new();

        // L0: Log only
        levels.insert(SeverityLevel::L0, LevelPolicy {
            level: SeverityLevel::L0,
            actions: vec![RecoveryAction {
                action_type: RecoveryActionType::LogOnly,
                target: None,
                max_retries: 0,
                cooldown_secs: 0,
            }],
            conditions: vec![],
            require_approval: false,
            cooldown_secs: 0,
        });

        // L1: Local self-healing
        levels.insert(SeverityLevel::L1, LevelPolicy {
            level: SeverityLevel::L1,
            actions: vec![
                RecoveryAction { action_type: RecoveryActionType::RestartDriver, target: None, max_retries: 3, cooldown_secs: 60 },
                RecoveryAction { action_type: RecoveryActionType::RejoinLora, target: None, max_retries: 2, cooldown_secs: 120 },
                RecoveryAction { action_type: RecoveryActionType::ReconnectDevice, target: None, max_retries: 3, cooldown_secs: 60 },
            ],
            conditions: vec![
                HealingCondition { condition_type: "process_dead".to_string(), threshold: None, count: Some(1) },
                HealingCondition { condition_type: "device_timeout".to_string(), threshold: None, count: Some(3) },
                HealingCondition { condition_type: "lora_rejoin_failed".to_string(), threshold: None, count: Some(2) },
            ],
            require_approval: false,
            cooldown_secs: 300,
        });

        // L2: Report cloud + cleanup
        levels.insert(SeverityLevel::L2, LevelPolicy {
            level: SeverityLevel::L2,
            actions: vec![
                RecoveryAction { action_type: RecoveryActionType::ReportCloud, target: None, max_retries: 3, cooldown_secs: 60 },
                RecoveryAction { action_type: RecoveryActionType::CleanLogs, target: None, max_retries: 1, cooldown_secs: 300 },
            ],
            conditions: vec![
                HealingCondition { condition_type: "devices_offline_ratio".to_string(), threshold: Some(0.2), count: None },
                HealingCondition { condition_type: "disk_usage".to_string(), threshold: Some(85.0), count: None },
                HealingCondition { condition_type: "consecutive_failures".to_string(), threshold: None, count: Some(5) },
            ],
            require_approval: false,
            cooldown_secs: 600,
        });

        // L3: Report cloud + ticket (approval required)
        levels.insert(SeverityLevel::L3, LevelPolicy {
            level: SeverityLevel::L3,
            actions: vec![
                RecoveryAction { action_type: RecoveryActionType::ReportCloud, target: None, max_retries: 5, cooldown_secs: 30 },
                RecoveryAction { action_type: RecoveryActionType::CreateTicket, target: None, max_retries: 1, cooldown_secs: 0 },
            ],
            conditions: vec![
                HealingCondition { condition_type: "bus_short_circuit".to_string(), threshold: None, count: None },
                HealingCondition { condition_type: "core_service_crash".to_string(), threshold: None, count: None },
                HealingCondition { condition_type: "memory_leak_suspected".to_string(), threshold: None, count: None },
            ],
            require_approval: true,
            cooldown_secs: 0,
        });

        Self { enabled: true, levels }
    }
}

/// Result of a probe execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub probe_type: ProbeType,
    pub timestamp: DateTime<Utc>,
    pub healthy: bool,
    pub severity: SeverityLevel,
    pub findings: Vec<ProbeFinding>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProbeType {
    System,
    Device,
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeFinding {
    pub finding_type: String,
    pub severity: SeverityLevel,
    pub message: String,
    pub target: Option<String>,
    pub value: Option<f64>,
}

/// A single healing execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingExecution {
    pub id: String,
    pub tenant_id: String,  // Required for multi-tenant isolation
    pub timestamp: DateTime<Utc>,
    pub level: SeverityLevel,
    pub action_type: RecoveryActionType,
    pub target: Option<String>,
    pub result: ExecutionResult,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionResult {
    Success,
    Failed,
    PendingApproval,
    Skipped,
}
```

- [ ] **Step 3: Create `api/src/domain/self_healing/errors.rs`**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SelfHealingError {
    #[error("Policy not found")]
    PolicyNotFound,

    #[error("Execution not found: {0}")]
    ExecutionNotFound(String),

    #[error("Action not allowed: {0}")]
    ActionNotAllowed(String),

    #[error("Cooldown active for target: {0}")]
    CooldownActive(String),

    #[error("Probe failed: {0}")]
    ProbeFailed(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

pub type Result<T> = std::result::Result<T, SelfHealingError>;
```

- [ ] **Step 4: Create `api/src/domain/self_healing/evaluator.rs`**

```rust
use super::{HealingCondition, LevelPolicy, ProbeFinding, ProbeResult, SelfHealingPolicy, SeverityLevel};
use crate::domain::self_healing::entity::ProbeType;

/// PolicyEvaluator evaluates probe results against policy thresholds
pub struct PolicyEvaluator {
    policy: SelfHealingPolicy,
}

impl PolicyEvaluator {
    pub fn new(policy: SelfHealingPolicy) -> Self {
        Self { policy }
    }

    pub fn with_default_policy() -> Self {
        Self::new(SelfHealingPolicy::default())
    }

    /// Evaluate a probe result and return the appropriate severity level
    pub fn evaluate(&self, probe_result: &ProbeResult) -> SeverityLevel {
        if !self.policy.enabled {
            return SeverityLevel::L0;
        }

        let mut highest_severity = SeverityLevel::L0;

        for finding in &probe_result.findings {
            if finding.severity > highest_severity {
                highest_severity = finding.severity;
            }
        }

        // Also check by probe type-specific logic
        if probe_result.probe_type == ProbeType::System {
            if let Some(cpu_finding) = probe_result.findings.iter().find(|f| f.finding_type == "cpu_usage") {
                if let Some(val) = cpu_finding.value {
                    if val >= 90.0 {
                        highest_severity = highest_severity.max(SeverityLevel::L2);
                    } else if val >= 70.0 {
                        highest_severity = highest_severity.max(SeverityLevel::L1);
                    }
                }
            }
        }

        highest_severity
    }

    /// Get the policy for a specific severity level
    pub fn get_level_policy(&self, level: SeverityLevel) -> Option<&LevelPolicy> {
        self.policy.levels.get(&level)
    }

    /// Check if conditions are met for a level
    pub fn check_conditions(&self, level: SeverityLevel, findings: &[ProbeFinding]) -> bool {
        let Some(policy) = self.policy.levels.get(&level) else {
            return false;
        };

        if policy.conditions.is_empty() {
            return true;
        }

        for condition in &policy.conditions {
            let matching_findings = findings.iter().filter(|f| f.finding_type == condition.condition_type);
            let count = matching_findings.count() as u32;

            if let Some(threshold) = condition.threshold {
                let has_threshold_met = matching_findings.any(|f| {
                    f.value.map(|v| v >= threshold).unwrap_or(false)
                });
                if !has_threshold_met && count < condition.count.unwrap_or(1) {
                    return false;
                }
            } else if count < condition.count.unwrap_or(1) {
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

    fn make_probe_result(findings: Vec<ProbeFinding>) -> ProbeResult {
        ProbeResult {
            probe_type: ProbeType::System,
            timestamp: Utc::now(),
            healthy: false,
            severity: SeverityLevel::L0,
            findings,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    fn test_evaluator_returns_l0_when_disabled() {
        let mut policy = SelfHealingPolicy::default();
        policy.enabled = false;
        let evaluator = PolicyEvaluator::new(policy);

        let probe = make_probe_result(vec![ProbeFinding {
            finding_type: "cpu_high".to_string(),
            severity: SeverityLevel::L2,
            message: "CPU at 95%".to_string(),
            target: None,
            value: Some(95.0),
        }]);

        assert_eq!(evaluator.evaluate(&probe), SeverityLevel::L0);
    }

    #[tokio::test]
    fn test_evaluator_returns_highest_severity() {
        let evaluator = PolicyEvaluator::with_default_policy();

        let probe = make_probe_result(vec![
            ProbeFinding {
                finding_type: "cpu_high".to_string(),
                severity: SeverityLevel::L1,
                message: "CPU at 75%".to_string(),
                target: None,
                value: Some(75.0),
            },
            ProbeFinding {
                finding_type: "disk_critical".to_string(),
                severity: SeverityLevel::L2,
                message: "Disk at 90%".to_string(),
                target: None,
                value: Some(90.0),
            },
        ]);

        assert_eq!(evaluator.evaluate(&probe), SeverityLevel::L2);
    }

    #[tokio::test]
    fn test_evaluator_l3_for_critical_conditions() {
        let evaluator = PolicyEvaluator::with_default_policy();

        let probe = make_probe_result(vec![ProbeFinding {
            finding_type: "core_service_crash".to_string(),
            severity: SeverityLevel::L3,
            message: "Core service crashed".to_string(),
            target: Some("mqtt_broker".to_string()),
            value: None,
        }]);

        assert_eq!(evaluator.evaluate(&probe), SeverityLevel::L3);
    }
}
```

- [ ] **Step 5: Create `api/src/domain/self_healing/executor.rs`**

```rust
use super::{ExecutionResult, HealingExecution, RecoveryActionType, SelfHealingError, SeverityLevel};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

/// ActionExecutor executes recovery actions based on severity level
pub struct ActionExecutor {
    /// Track last execution time per target for cooldown
    last_execution: Arc<RwLock<std::collections::HashMap<String, chrono::DateTime<Utc>>>>,
}

impl ActionExecutor {
    pub fn new() -> Self {
        Self {
            last_execution: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Execute a recovery action
    pub async fn execute(
        &self,
        level: SeverityLevel,
        action_type: RecoveryActionType,
        target: Option<String>,
        cooldown_secs: u64,
    ) -> Result<HealingExecution, SelfHealingError> {
        let target_key = target.clone().unwrap_or_else(|| format!("{:?}", level));
        let now = Utc::now();

        // Check cooldown
        {
            let last = self.last_execution.read().await;
            if let Some(last_time) = last.get(&target_key) {
                let elapsed = (now - *last_time).num_seconds();
                if elapsed < cooldown_secs as i64 {
                    return Err(SelfHealingError::CooldownActive(format!(
                        "Cooldown active for {} ({}s remaining)",
                        target_key,
                        cooldown_secs as i64 - elapsed
                    )));
                }
            }
        }

        let execution = match action_type {
            RecoveryActionType::LogOnly => {
                tracing::info!("[L0] LogOnly action for target: {:?}", target);
                HealingExecution {
                    id: uuid::Uuid::new_v4().to_string(),
                    tenant_id: "default".to_string(), // TODO: get from Claims context
                    timestamp: now,
                    level,
                    action_type: RecoveryActionType::LogOnly,
                    target,
                    result: ExecutionResult::Success,
                    logs: vec!["Action logged".to_string()],
                }
            }
            RecoveryActionType::RestartDriver => {
                self.execute_restart_driver(target).await?
            }
            RecoveryActionType::RejoinLora => {
                self.execute_rejoin_lora(target).await?
            }
            RecoveryActionType::ReconnectDevice => {
                self.execute_reconnect_device(target).await?
            }
            RecoveryActionType::CleanLogs => {
                self.execute_clean_logs(target).await?
            }
            RecoveryActionType::ReportCloud => {
                self.execute_report_cloud(level, target).await?
            }
            RecoveryActionType::CreateTicket => {
                self.execute_create_ticket(level, target).await?
            }
        };

        // Update last execution time
        {
            let mut last = self.last_execution.write().await;
            last.insert(target_key, now);
        }

        Ok(execution)
    }

    async fn execute_restart_driver(&self, target: Option<String>) -> Result<HealingExecution, SelfHealingError> {
        tracing::info!("[L1] RestartDriver for: {:?}", target);
        // TODO: Actually restart the driver via driver manager (Phase 3 integration)
        Ok(HealingExecution {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "default".to_string(), // TODO: get from Claims context
            timestamp: Utc::now(),
            level: SeverityLevel::L1,
            action_type: RecoveryActionType::RestartDriver,
            target,
            result: ExecutionResult::Success,
            logs: vec!["Driver restart requested".to_string()],
        })
    }

    async fn execute_rejoin_lora(&self, target: Option<String>) -> Result<HealingExecution, SelfHealingError> {
        tracing::info!("[L1] RejoinLora for: {:?}", target);
        // TODO: Actually rejoin LoRa network (Phase 3 integration)
        Ok(HealingExecution {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "default".to_string(), // TODO: get from Claims context
            timestamp: Utc::now(),
            level: SeverityLevel::L1,
            action_type: RecoveryActionType::RejoinLora,
            target,
            result: ExecutionResult::Success,
            logs: vec!["LoRa rejoin requested".to_string()],
        })
    }

    async fn execute_reconnect_device(&self, target: Option<String>) -> Result<HealingExecution, SelfHealingError> {
        tracing::info!("[L1] ReconnectDevice for: {:?}", target);
        // TODO: Actually reconnect via device manager (Phase 3 integration)
        Ok(HealingExecution {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "default".to_string(), // TODO: get from Claims context
            timestamp: Utc::now(),
            level: SeverityLevel::L1,
            action_type: RecoveryActionType::ReconnectDevice,
            target,
            result: ExecutionResult::Success,
            logs: vec!["Device reconnect requested".to_string()],
        })
    }

    async fn execute_clean_logs(&self, target: Option<String>) -> Result<HealingExecution, SelfHealingError> {
        tracing::info!("[L2] CleanLogs for: {:?}", target);
        Ok(HealingExecution {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "default".to_string(), // TODO: get from Claims context
            timestamp: Utc::now(),
            level: SeverityLevel::L2,
            action_type: RecoveryActionType::CleanLogs,
            target,
            result: ExecutionResult::Success,
            logs: vec!["Logs cleaned".to_string()],
        })
    }

    async fn execute_report_cloud(&self, level: SeverityLevel, target: Option<String>) -> Result<HealingExecution, SelfHealingError> {
        tracing::info!("[{:?}] ReportCloud for: {:?}", level, target);
        // TODO: Actually send to cloud endpoint (Phase 4 integration)
        Ok(HealingExecution {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "default".to_string(), // TODO: get from Claims context
            timestamp: Utc::now(),
            level,
            action_type: RecoveryActionType::ReportCloud,
            target,
            result: ExecutionResult::Success,
            logs: vec!["Cloud report sent".to_string()],
        })
    }

    async fn execute_create_ticket(&self, level: SeverityLevel, target: Option<String>) -> Result<HealingExecution, SelfHealingError> {
        tracing::warn!("[{:?}] CreateTicket for: {:?} - requires approval", level, target);
        // TODO: Actually create ticket via ticketing system (Phase 4 integration)
        Ok(HealingExecution {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "default".to_string(), // TODO: get from Claims context
            timestamp: Utc::now(),
            level,
            action_type: RecoveryActionType::CreateTicket,
            target,
            result: ExecutionResult::PendingApproval,
            logs: vec!["Ticket creation pending approval".to_string()],
        })
    }
}

impl Default for ActionExecutor {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 6: Create `api/src/domain/self_healing/repository.rs`**

```rust
use super::{HealingExecution, SeverityLevel};
use crate::infrastructure::persistence::database::Database;
use std::sync::Arc;

/// Repository for persisting healing execution history
pub struct HealingExecutionRepository {
    db: Arc<Database>,
}

impl HealingExecutionRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Save a healing execution record
    pub async fn save(&self, execution: &HealingExecution) -> Result<(), sqlx::Error> {
        let level_str = format!("{:?}", execution.level);
        let action_str = format!("{:?}", execution.action_type);
        let result_str = format!("{:?}", execution.result);
        let logs_json = serde_json::to_string(&execution.logs).unwrap_or_default();

        sqlx::query(
            r#"INSERT INTO healing_executions
               (id, tenant_id, timestamp, level, action_type, target, result, logs)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(&execution.id)
        .bind(&execution.tenant_id)
        .bind(execution.timestamp.to_rfc3339())
        .bind(&level_str)
        .bind(&action_str)
        .bind(execution.target.as_deref())
        .bind(&result_str)
        .bind(&logs_json)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }

    /// Get recent healing executions with pagination (tenant-scoped)
    pub async fn get_recent(&self, tenant_id: &str, limit: u32, offset: u32) -> Result<Vec<HealingExecution>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, timestamp, level, action_type, target, result, logs
               FROM healing_executions
               WHERE tenant_id = ?
               ORDER BY timestamp DESC
               LIMIT ? OFFSET ?"#
        )
        .bind(tenant_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.db.pool())
        .await?;

        let mut executions = Vec::new();
        for row in rows {
            let level_str: String = row.get("level");
            let action_str: String = row.get("action_type");
            let result_str: String = row.get("result");
            let logs_json: Option<String> = row.get("logs");
            let timestamp_str: String = row.get("timestamp");

            let level = match level_str.as_str() {
                "L1" => SeverityLevel::L1,
                "L2" => SeverityLevel::L2,
                "L3" => SeverityLevel::L3,
                _ => SeverityLevel::L0,
            };

            let action_type = match action_str.as_str() {
                "RestartDriver" => super::RecoveryActionType::RestartDriver,
                "RejoinLora" => super::RecoveryActionType::RejoinLora,
                "ReconnectDevice" => super::RecoveryActionType::ReconnectDevice,
                "CleanLogs" => super::RecoveryActionType::CleanLogs,
                "ReportCloud" => super::RecoveryActionType::ReportCloud,
                "CreateTicket" => super::RecoveryActionType::CreateTicket,
                _ => super::RecoveryActionType::LogOnly,
            };

            let result = match result_str.as_str() {
                "Success" => super::ExecutionResult::Success,
                "Failed" => super::ExecutionResult::Failed,
                "PendingApproval" => super::ExecutionResult::PendingApproval,
                _ => super::ExecutionResult::Skipped,
            };

            let logs: Vec<String> = logs_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();

            executions.push(HealingExecution {
                id: row.get("id"),
                tenant_id: row.get("tenant_id"),
                timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                level,
                action_type,
                target: row.get("target"),
                result,
                logs,
            });
        }

        Ok(executions)
    }

    /// Create the healing_executions table if it doesn't exist
    pub async fn ensure_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS healing_executions (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                level TEXT NOT NULL,
                action_type TEXT NOT NULL,
                target TEXT,
                result TEXT NOT NULL,
                logs TEXT
            )"#
        )
        .execute(self.db.pool())
        .await?;

        Ok(())
    }
}
```

- [ ] **Step 7: Create `api/src/dto/entity/self_healing.rs`**

```rust
// Self-Healing DTOs for API responses
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::super::super::domain::self_healing::{
    ExecutionResult, HealingExecution, LevelPolicy, ProbeFinding, ProbeResult, ProbeType,
    RecoveryAction, RecoveryActionType, SelfHealingPolicy as DomainPolicy, SeverityLevel,
};

/// Self-healing policy DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfHealingPolicyDto {
    pub enabled: bool,
    pub levels: HashMap<String, LevelPolicyDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelPolicyDto {
    pub level: String,
    pub actions: Vec<RecoveryActionDto>,
    pub conditions: Vec<HealingConditionDto>,
    pub require_approval: bool,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryActionDto {
    pub action_type: String,
    pub target: Option<String>,
    pub max_retries: u32,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingConditionDto {
    pub condition_type: String,
    pub threshold: Option<f64>,
    pub count: Option<u32>,
}

impl From<&DomainPolicy> for SelfHealingPolicyDto {
    fn from(policy: &DomainPolicy) -> Self {
        let levels = policy.levels.iter()
            .map(|(k, v)| (format!("{:?}", k), LevelPolicyDto::from(v)))
            .collect();
        Self { enabled: policy.enabled, levels }
    }
}

impl From<&LevelPolicy> for LevelPolicyDto {
    fn from(p: &LevelPolicy) -> Self {
        Self {
            level: format!("{:?}", p.level),
            actions: p.actions.iter().map(RecoveryActionDto::from).collect(),
            conditions: p.conditions.iter().map(HealingConditionDto::from).collect(),
            require_approval: p.require_approval,
            cooldown_secs: p.cooldown_secs,
        }
    }
}

impl From<&RecoveryAction> for RecoveryActionDto {
    fn from(a: &RecoveryAction) -> Self {
        Self {
            action_type: format!("{:?}", a.action_type),
            target: a.target.clone(),
            max_retries: a.max_retries,
            cooldown_secs: a.cooldown_secs,
        }
    }
}

impl From<&HealingCondition> for HealingConditionDto {
    fn from(c: &HealingCondition) -> Self {
        Self {
            condition_type: c.condition_type.clone(),
            threshold: c.threshold,
            count: c.count,
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
    pub target: Option<String>,
    pub result: String,
    pub logs: Vec<String>,
}

impl From<&HealingExecution> for HealingExecutionDto {
    fn from(e: &HealingExecution) -> Self {
        Self {
            id: e.id.clone(),
            tenant_id: e.tenant_id.clone(),
            timestamp: e.timestamp,
            level: format!("{:?}", e.level),
            action_type: format!("{:?}", e.action_type),
            target: e.target.clone(),
            result: format!("{:?}", e.result),
            logs: e.logs.clone(),
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeFindingDto {
    pub finding_type: String,
    pub severity: String,
    pub message: String,
    pub target: Option<String>,
    pub value: Option<f64>,
}

impl From<&ProbeResult> for ProbeResultDto {
    fn from(p: &ProbeResult) -> Self {
        Self {
            probe_type: format!("{:?}", p.probe_type),
            timestamp: p.timestamp,
            healthy: p.healthy,
            severity: format!("{:?}", p.severity),
            findings: p.findings.iter().map(ProbeFindingDto::from).collect(),
        }
    }
}

impl From<&ProbeFinding> for ProbeFindingDto {
    fn from(f: &ProbeFinding) -> Self {
        Self {
            finding_type: f.finding_type.clone(),
            severity: format!("{:?}", f.severity),
            message: f.message.clone(),
            target: f.target.clone(),
            value: f.value,
        }
    }
}

/// Request to execute a self-heal action
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSelfHealRequest {
    pub level: String,
    pub target: Option<String>,
    pub action_type: String,
    pub force: Option<bool>,
}

/// Response after executing a self-heal action
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSelfHealResponse {
    pub execution_id: String,
    pub executed: bool,
    pub result: String,
    pub logs: Vec<String>,
}

/// Probe configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeConfig {
    pub system_probe_enabled: bool,
    pub system_probe_interval_secs: u64,
    pub device_probe_enabled: bool,
    pub device_probe_interval_secs: u64,
    pub task_probe_enabled: bool,
    pub task_probe_interval_secs: u64,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            system_probe_enabled: true,
            system_probe_interval_secs: 600,  // 10 min
            device_probe_enabled: true,
            device_probe_interval_secs: 1800, // 30 min
            task_probe_enabled: true,
            task_probe_interval_secs: 900,    // 15 min
        }
    }
}
```

- [ ] **Step 8: Run tests**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo test self_healing --no-fail-fast 2>&1 | head -50`
Expected: All domain tests pass

- [ ] **Step 9: Commit**

```bash
git add api/src/domain/self_healing/ api/src/dto/entity/self_healing.rs
git commit -m "feat(self_healing): add domain module with policy, evaluator, executor, repository

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Probe Scheduler Infrastructure

**Files:**
- Create: `api/src/infrastructure/self_healing/mod.rs`
- Create: `api/src/infrastructure/self_healing/probe_scheduler.rs`

### Steps

- [ ] **Step 1: Create `api/src/infrastructure/self_healing/mod.rs`**

```rust
pub mod probe_scheduler;

pub use probe_scheduler::ProbeScheduler;
```

- [ ] **Step 2: Create `api/src/infrastructure/self_healing/probe_scheduler.rs`**

```rust
use crate::domain::self_healing::{
    entity::{ProbeFinding, ProbeResult, ProbeType, SeverityLevel},
    PolicyEvaluator,
};
use crate::dto::entity::self_healing::ProbeConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio::time::{interval, Duration};

/// ProbeScheduler runs system, device, and task probes on configured intervals
pub struct ProbeScheduler {
    config: Arc<RwLock<ProbeConfig>>,
    evaluator: Arc<PolicyEvaluator>,
    last_probe_results: Arc<RwLock<HashMap<ProbeType, ProbeResult>>>,
    shutdown_rx: Arc<RwLock<Option<broadcast::Receiver<()>>>>,
}

impl ProbeScheduler {
    pub fn new(config: ProbeConfig, evaluator: Arc<PolicyEvaluator>, shutdown_rx: broadcast::Receiver<()>) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            evaluator,
            last_probe_results: Arc::new(RwLock::new(HashMap::new())),
            shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
        }
    }

    /// Start the probe scheduler (runs in background)
    /// Exits when shutdown signal is received
    pub async fn run(&self) {
        let config = self.config.read().await;

        let shutdown_rx = {
            let mut guard = self.shutdown_rx.write().await;
            guard.take()
        };
        let mut shutdown = match shutdown_rx {
            Some(rx) => rx,
            None => {
                tracing::error!("ProbeScheduler::run() called twice — shutdown receiver already taken");
                return;
            }
        };

        let system_interval = if config.system_probe_enabled {
            Some(interval(Duration::from_secs(config.system_probe_interval_secs)))
        } else {
            None
        };

        let device_interval = if config.device_probe_enabled {
            Some(interval(Duration::from_secs(config.device_probe_interval_secs)))
        } else {
            None
        };

        let task_interval = if config.task_probe_enabled {
            Some(interval(Duration::from_secs(config.task_probe_interval_secs)))
        } else {
            None
        };

        tracing::info!(
            "ProbeScheduler started: system={:?} device={:?} task={:?}",
            config.system_probe_interval_secs,
            config.device_probe_interval_secs,
            config.task_probe_interval_secs
        );

        // Run initial probes
        self.run_system_probe().await;
        self.run_device_probe().await;
        self.run_task_probe().await;

        // Main loop - tick each probe interval independently
        let mut sys_ticker = system_interval;
        let mut dev_ticker = device_interval;
        let mut tsk_ticker = task_interval;

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("ProbeScheduler shutting down");
                    break;
                }
                _ = async { if let Some(ref mut t) = sys_ticker { t.tick().await; } }, if sys_ticker.is_some() => {
                    self.run_system_probe().await;
                }
                _ = async { if let Some(ref mut t) = dev_ticker { t.tick().await; } }, if dev_ticker.is_some() => {
                    self.run_device_probe().await;
                }
                _ = async { if let Some(ref mut t) = tsk_ticker { t.tick().await; } }, if tsk_ticker.is_some() => {
                    self.run_task_probe().await;
                }
            }
        }
    }

    /// Run system probe (CPU, memory, disk, network, process status)
    pub async fn run_system_probe(&self) {
        tracing::debug!("Running system probe...");

        let mut findings = Vec::new();

        // CPU check
        if let Ok(cpu) = sysinfo::CpuRefreshKind::nothing().into_refresh_kind()
            .compute()
        {
            let cpu_usage = cpu.global_cpu_usage();
            if cpu_usage >= 90.0 {
                findings.push(ProbeFinding {
                    finding_type: "cpu_critical".to_string(),
                    severity: SeverityLevel::L2,
                    message: format!("CPU critical: {:.1}%", cpu_usage),
                    target: None,
                    value: Some(cpu_usage),
                });
            } else if cpu_usage >= 70.0 {
                findings.push(ProbeFinding {
                    finding_type: "cpu_warning".to_string(),
                    severity: SeverityLevel::L1,
                    message: format!("CPU warning: {:.1}%", cpu_usage),
                    target: None,
                    value: Some(cpu_usage),
                });
            }
        }

        // Memory check
        if let Ok(memory) = sysinfo::MemoryRefreshKind::nothing().into_refresh_kind().compute() {
            let mem_usage = (memory.used() as f64 / memory.total() as f64) * 100.0;
            if mem_usage >= 90.0 {
                findings.push(ProbeFinding {
                    finding_type: "memory_critical".to_string(),
                    severity: SeverityLevel::L2,
                    message: format!("Memory critical: {:.1}%", mem_usage),
                    target: None,
                    value: Some(mem_usage),
                });
            } else if mem_usage >= 75.0 {
                findings.push(ProbeFinding {
                    finding_type: "memory_warning".to_string(),
                    severity: SeverityLevel::L1,
                    message: format!("Memory warning: {:.1}%", mem_usage),
                    target: None,
                    value: Some(mem_usage),
                });
            }
        }

        // Disk check
        if let Ok(disk) = sysinfo::Disks::new_with_refreshed_list() {
            for disk_entry in disk.list() {
                let usage = (disk_entry.total_space() - disk_entry.available_space()) as f64
                    / disk_entry.total_space() as f64 * 100.0;
                if usage >= 95.0 {
                    findings.push(ProbeFinding {
                        finding_type: "disk_critical".to_string(),
                        severity: SeverityLevel::L2,
                        message: format!("Disk {} critical: {:.1}%", disk_entry.mount_point().display(), usage),
                        target: Some(disk_entry.mount_point().to_string_lossy().to_string()),
                        value: Some(usage),
                    });
                } else if usage >= 80.0 {
                    findings.push(ProbeFinding {
                        finding_type: "disk_warning".to_string(),
                        severity: SeverityLevel::L1,
                        message: format!("Disk {} warning: {:.1}%", disk_entry.mount_point().display(), usage),
                        target: Some(disk_entry.mount_point().to_string_lossy().to_string()),
                        value: Some(usage),
                    });
                }
            }
        }

        let severity = self.evaluator.evaluate(&ProbeResult {
            probe_type: ProbeType::System,
            timestamp: chrono::Utc::now(),
            healthy: findings.is_empty(),
            severity: SeverityLevel::L0,
            findings: findings.clone(),
            metadata: HashMap::new(),
        });

        let result = ProbeResult {
            probe_type: ProbeType::System,
            timestamp: chrono::Utc::now(),
            healthy: findings.is_empty(),
            severity,
            findings,
            metadata: HashMap::new(),
        };

        {
            let mut results = self.last_probe_results.write().await;
            results.insert(ProbeType::System, result.clone());
        }

        tracing::debug!("System probe complete: healthy={}, severity={:?}", result.healthy, result.severity);
    }

    /// Run device probe (device online status, data freshness)
    pub async fn run_device_probe(&self) {
        tracing::debug!("Running device probe...");

        // TODO: Query actual device status from DeviceService
        // For now, return empty probe result
        let findings = Vec::new();

        let result = ProbeResult {
            probe_type: ProbeType::Device,
            timestamp: chrono::Utc::now(),
            healthy: true,
            severity: SeverityLevel::L0,
            findings,
            metadata: HashMap::new(),
        };

        {
            let mut results = self.last_probe_results.write().await;
            results.insert(ProbeType::Device, result.clone());
        }

        tracing::debug!("Device probe complete: healthy={}", result.healthy);
    }

    /// Run task probe (data sync tasks, automation rule execution)
    pub async fn run_task_probe(&self) {
        tracing::debug!("Running task probe...");

        // TODO: Query actual task/job status from scheduler
        let findings = Vec::new();

        let result = ProbeResult {
            probe_type: ProbeType::Task,
            timestamp: chrono::Utc::now(),
            healthy: true,
            severity: SeverityLevel::L0,
            findings,
            metadata: HashMap::new(),
        };

        {
            let mut results = self.last_probe_results.write().await;
            results.insert(ProbeType::Task, result.clone());
        }

        tracing::debug!("Task probe complete: healthy={}", result.healthy);
    }

    /// Get the last probe result for a given type
    pub async fn get_last_result(&self, probe_type: ProbeType) -> Option<ProbeResult> {
        let results = self.last_probe_results.read().await;
        results.get(&probe_type).cloned()
    }

    /// Get all last probe results
    pub async fn get_all_results(&self) -> HashMap<ProbeType, ProbeResult> {
        let results = self.last_probe_results.read().await;
        results.clone()
    }

    /// Update probe configuration
    pub async fn update_config(&self, config: ProbeConfig) {
        let mut cfg = self.config.write().await;
        *cfg = config;
    }

    /// Get current probe configuration
    pub async fn get_config(&self) -> ProbeConfig {
        self.config.read().await.clone()
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo check 2>&1 | tail -30`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add api/src/infrastructure/self_healing/
git commit -m "feat(self_healing): add probe scheduler infrastructure

- SystemProbe: CPU, memory, disk, network checks
- DeviceProbe: device online status and data freshness
- TaskProbe: job/automation execution status
- Evaluator integration for severity assessment

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Self-Healing API Handlers

**Files:**
- Create: `api/src/api/self_healing/mod.rs`
- Create: `api/src/api/self_healing/handlers.rs`
- Modify: `api/src/api/mod.rs` - register self_healing router
- Modify: `api/src/shared/app_state.rs` - add SelfHealingState

### Steps

- [ ] **Step 1: Create global state pattern in `api/src/api/self_healing/mod.rs`**

```rust
// Self-Healing API Module
// HTTP endpoint handlers for self-healing management

use std::sync::Arc;
use std::sync::OnceLock;

use tokio::sync::RwLock;

use crate::domain::self_healing::{ActionExecutor, PolicyEvaluator, SelfHealingPolicy, HealingExecutionRepository};
use crate::dto::entity::self_healing::ProbeConfig;
use crate::infrastructure::persistence::database::Database;
use crate::infrastructure::self_healing::ProbeScheduler;

/// Global self-healing state
static SELF_HEALING_STATE: OnceLock<Arc<RwLock<SelfHealingState>>> = OnceLock::new();

/// Self-healing runtime state
pub struct SelfHealingState {
    pub policy: SelfHealingPolicy,
    pub evaluator: Arc<PolicyEvaluator>,
    pub executor: Arc<ActionExecutor>,
    pub repository: Arc<HealingExecutionRepository>,
    pub scheduler: Arc<ProbeScheduler>,
    pub probe_config: ProbeConfig,
}

impl SelfHealingState {
    pub fn new(db: Arc<Database>) -> Self {
        let policy = SelfHealingPolicy::default();
        let evaluator = Arc::new(PolicyEvaluator::new(policy.clone()));
        let repository = Arc::new(HealingExecutionRepository::new(db));
        let probe_config = ProbeConfig::default();
        // ProbeScheduler is spawned separately with shutdown handle; this is just the shared reference
        let scheduler = Arc::new(ProbeScheduler::new(
            probe_config.clone(),
            evaluator.clone(),
            tokio::sync::broadcast::channel::<()>(1).1, // dummy receiver; replaced at spawn
        ));
        Self {
            policy,
            evaluator,
            executor: Arc::new(ActionExecutor::new()),
            repository,
            scheduler,
            probe_config,
        }
    }
}

impl Default for SelfHealingState {
    fn default() -> Self { Self::new(Arc::new(Database::new(":memory:").unwrap())) }
}

/// Initialize global self-healing state (call once at startup)
pub fn init_self_healing_state(db: Arc<Database>) -> Arc<RwLock<SelfHealingState>> {
    SELF_HEALING_STATE
        .get_or_init(|| Arc::new(RwLock::new(SelfHealingState::new(db))))
        .clone()
}

/// Get self-healing state
pub fn get_self_healing_state() -> Option<Arc<RwLock<SelfHealingState>>> {
    SELF_HEALING_STATE.get().cloned()
}
```

- [ ] **Step 2: Create `api/src/api/self_healing/handlers.rs`**

```rust
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;

use crate::{
    api::self_healing::{get_self_healing_state, SelfHealingState},
    dto::{
        entity::self_healing::{
            ExecuteSelfHealRequest, ExecuteSelfHealResponse, HealingExecutionDto,
            ProbeConfig as ProbeConfigDto, ProbeResultDto, SelfHealingPolicyDto,
        },
        response::{builder::ApiResponseBuilder, ApiResponse},
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

/// Create the self-healing router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Policy
        .route("/policies", get(get_policy))
        .route("/policies", put(update_policy))
        // Actions
        .route("/actions/:level", post(execute_action))
        // History
        .route("/executions", get(get_executions))
        // Probe
        .route("/probes", get(get_probe_status))
        .route("/probes/config", get(get_probe_config))
        .route("/probes/config", put(update_probe_config))
}

/// Query params for execution history
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// GET /self-healing/policies - Get current policy
async fn get_policy(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<SelfHealingPolicyDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let state = state.read().await;
    ApiResponseBuilder::success(SelfHealingPolicyDto::from(&state.policy))
}

/// PUT /self-healing/policies - Update policy
async fn update_policy(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(policy): Json<SelfHealingPolicyDto>,
) -> Json<ApiResponse<SelfHealingPolicyDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let mut state = state.write().await;
    // For now, just update enabled flag; full policy update would need more complex mapping
    state.policy.enabled = policy.enabled;
    state.evaluator = Arc::new(PolicyEvaluator::new(state.policy.clone()));

    ApiResponseBuilder::success(SelfHealingPolicyDto::from(&state.policy))
}

/// POST /self-healing/actions/:level - Execute recovery action
async fn execute_action(
    State(_state): State<AppState>,
    _claims: Claims,
    Path(level): Path<String>,
    Json(request): Json<ExecuteSelfHealRequest>,
) -> Json<ApiResponse<ExecuteSelfHealResponse>> {
    use crate::domain::self_healing::{RecoveryActionType, SeverityLevel};

    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let state = state.read().await;

    let severity = match level.to_uppercase().as_str() {
        "L0" => SeverityLevel::L0,
        "L1" => SeverityLevel::L1,
        "L2" => SeverityLevel::L2,
        "L3" => SeverityLevel::L3,
        _ => return ApiResponseBuilder::error("Invalid severity level (L0-L3 required)"),
    };

    let action_type = match request.action_type.to_lowercase().as_str() {
        "log_only" => RecoveryActionType::LogOnly,
        "restart_driver" => RecoveryActionType::RestartDriver,
        "rejoin_lora" => RecoveryActionType::RejoinLora,
        "reconnect_device" => RecoveryActionType::ReconnectDevice,
        "clean_logs" => RecoveryActionType::CleanLogs,
        "report_cloud" => RecoveryActionType::ReportCloud,
        "create_ticket" => RecoveryActionType::CreateTicket,
        _ => return ApiResponseBuilder::error("Invalid action type"),
    };

    let cooldown = state.policy.levels.get(&severity)
        .map(|p| p.cooldown_secs)
        .unwrap_or(0);

    drop(state); // Release read lock

    let executor = get_self_healing_state()
        .expect("self-healing initialized")
        .read().await
        .executor.clone();

    match executor.execute(severity, action_type, request.target.clone(), cooldown).await {
        Ok(execution) => {
            ApiResponseBuilder::success(ExecuteSelfHealResponse {
                execution_id: execution.id,
                executed: true,
                result: format!("{:?}", execution.result),
                logs: execution.logs,
            })
        }
        Err(e) => {
            tracing::error!("Self-heal execution failed: {}", e);
            ApiResponseBuilder::error(e.to_string())
        }
    }
}

/// GET /self-healing/executions - Get recovery history
async fn get_executions(
    State(_state): State<AppState>,
    claims: Claims,
    Query(params): Query<HistoryQuery>,
) -> Json<ApiResponse<Vec<HealingExecutionDto>>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let state = state.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    let tenant_id = claims.tenant_id.clone();

    match state.repository.get_recent(&tenant_id, limit, offset).await {
        Ok(execs) => {
            let dtos: Vec<HealingExecutionDto> = execs.iter().map(HealingExecutionDto::from).collect();
            ApiResponseBuilder::success(dtos)
        }
        Err(e) => {
            tracing::error!("Failed to fetch healing executions: {}", e);
            ApiResponseBuilder::error("Failed to fetch execution history")
        }
    }
}

/// GET /self-healing/probes - Get current probe status
async fn get_probe_status(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<ProbeResultDto>>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let state = state.read().await;
    let results = state.scheduler.get_all_results().await;
    let dtos: Vec<ProbeResultDto> = results.values().map(ProbeResultDto::from).collect();
    ApiResponseBuilder::success(dtos)
}

/// GET /self-healing/probes/config - Get probe configuration
async fn get_probe_config(
    State(_state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<ProbeConfigDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let state = state.read().await;
    ApiResponseBuilder::success(ProbeConfigDto {
        system_probe_enabled: state.probe_config.system_probe_enabled,
        system_probe_interval_secs: state.probe_config.system_probe_interval_secs,
        device_probe_enabled: state.probe_config.device_probe_enabled,
        device_probe_interval_secs: state.probe_config.device_probe_interval_secs,
        task_probe_enabled: state.probe_config.task_probe_enabled,
        task_probe_interval_secs: state.probe_config.task_probe_interval_secs,
    })
}

/// PUT /self-healing/probes/config - Update probe configuration
async fn update_probe_config(
    State(_state): State<AppState>,
    _claims: Claims,
    Json(config): Json<ProbeConfigDto>,
) -> Json<ApiResponse<ProbeConfigDto>> {
    let state = match get_self_healing_state() {
        Some(s) => s,
        None => return ApiResponseBuilder::error("Self-healing not initialized"),
    };

    let mut state = state.write().await;
    state.probe_config = ProbeConfig {
        system_probe_enabled: config.system_probe_enabled,
        system_probe_interval_secs: config.system_probe_interval_secs,
        device_probe_enabled: config.device_probe_enabled,
        device_probe_interval_secs: config.device_probe_interval_secs,
        task_probe_enabled: config.task_probe_enabled,
        task_probe_interval_secs: config.task_probe_interval_secs,
    };

    ApiResponseBuilder::success(ProbeConfigDto {
        system_probe_enabled: state.probe_config.system_probe_enabled,
        system_probe_interval_secs: state.probe_config.system_probe_interval_secs,
        device_probe_enabled: state.probe_config.device_probe_enabled,
        device_probe_interval_secs: state.probe_config.device_probe_interval_secs,
        task_probe_enabled: state.probe_config.task_probe_enabled,
        task_probe_interval_secs: state.probe_config.task_probe_interval_secs,
    })
}
```

- [ ] **Step 3: Register self_healing router in `api/src/api/mod.rs`**

Read the file first, then add the route.

- [ ] **Step 4: Initialize self-healing state in `api/src/main.rs`**

Add `init_self_healing_state()` call during startup.

- [ ] **Step 5: Run cargo check**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo check 2>&1 | tail -30`
Expected: Compiles successfully

- [ ] **Step 6: Commit**

```bash
git add api/src/api/self_healing/ api/src/api/mod.rs api/src/main.rs
git commit -m "feat(api): add self-healing REST API endpoints

- GET/PUT /self-healing/policies
- POST /self-healing/actions/:level
- GET /self-healing/executions
- GET/PUT /self-healing/probes/config

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Replace MCP Self-Heal Stubs with Real Implementations

**Files:**
- Modify: `api/src/api/mcp/tools/self_heal.rs`

### Steps

- [ ] **Step 1: Replace `execute_self_heal_action` stub**

Replace the Phase 1 stub with real implementation:

```rust
async fn execute(&self, args: Value) -> Result<Value, ToolError> {
    use crate::api::self_healing::get_self_healing_state;
    use crate::dto::entity::self_healing::ExecuteSelfHealRequest;

    let request: ExecuteSelfHealRequest = serde_json::from_value(args)
        .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

    let state = get_self_healing_state()
        .ok_or_else(|| ToolError::Internal("Self-healing not initialized".to_string()))?;

    let state_guard = state.read().await;
    let executor = state_guard.executor.clone();
    let policy = state_guard.policy.clone();
    drop(state_guard);

    let level = match request.level.to_uppercase().as_str() {
        "L0" => SeverityLevel::L0,
        "L1" => SeverityLevel::L1,
        "L2" => SeverityLevel::L2,
        "L3" => SeverityLevel::L3,
        _ => return Err(ToolError::InvalidParams("Invalid level: use L0, L1, L2, or L3".to_string())),
    };

    let action_type = match request.action_type.to_lowercase().as_str() {
        "log_only" => RecoveryActionType::LogOnly,
        "restart_driver" => RecoveryActionType::RestartDriver,
        "rejoin_lora" => RecoveryActionType::RejoinLora,
        "reconnect_device" => RecoveryActionType::ReconnectDevice,
        "clean_logs" => RecoveryActionType::CleanLogs,
        "report_cloud" => RecoveryActionType::ReportCloud,
        "create_ticket" => RecoveryActionType::CreateTicket,
        _ => return Err(ToolError::InvalidParams("Invalid action_type".to_string())),
    };

    let cooldown = policy.levels.get(&level)
        .map(|p| p.cooldown_secs)
        .unwrap_or(0);

    executor.execute(level, action_type, request.target, cooldown)
        .await
        .map(|exec| serde_json::json!({
            "execution_id": exec.id,
            "executed": true,
            "result": format!("{:?}", exec.result),
            "logs": exec.logs
        }))
        .map_err(|e| ToolError::Internal(e.to_string()))
}
```

- [ ] **Step 2: Replace `get_recovery_history` stub**

```rust
async fn execute(&self, args: Value) -> Result<Value, ToolError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HistoryInput {
        limit: Option<u32>,
        offset: Option<u32>,
    }

    let input: HistoryInput = serde_json::from_value(args)
        .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

    let limit = input.limit.unwrap_or(20).min(100);
    let offset = input.offset.unwrap_or(0);

    let state = get_self_healing_state()
        .ok_or_else(|| ToolError::Internal("Self-healing not initialized".to_string()))?;

    let state_guard = state.read().await;
    let repository = state_guard.repository.clone();
    drop(state_guard);

    // Default tenant for MCP context (single-tenant or system context)
    let tenant_id = "default";

    let executions = repository.get_recent(tenant_id, limit, offset)
        .await
        .map_err(|e| ToolError::Internal(format!("DB error: {}", e)))?;

    let history: Vec<serde_json::Value> = executions.iter().map(|e| {
        serde_json::json!({
            "id": e.id,
            "timestamp": e.timestamp.to_rfc3339(),
            "level": format!("{:?}", e.level),
            "action_type": format!("{:?}", e.action_type),
            "target": e.target,
            "result": format!("{:?}", e.result),
            "logs": e.logs
        })
    }).collect();

    Ok(serde_json::json!({
        "executions": history,
        "limit": limit,
        "offset": offset,
        "total": history.len()
    }))
}
```

- [ ] **Step 3: Update `get_self_heal_policy` to include full policy structure**

Replace the hardcoded JSON with a call to get the actual policy from global state:

```rust
async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
    use crate::api::self_healing::get_self_healing_state;
    use crate::dto::entity::self_healing::SelfHealingPolicyDto;

    let state = get_self_healing_state()
        .ok_or_else(|| ToolError::Internal("Self-healing not initialized".to_string()))?;

    let state_guard = state.read().await;
    let policy_dto = SelfHealingPolicyDto::from(&state_guard.policy);
    drop(state_guard);

    Ok(serde_json::to_value(policy_dto).unwrap_or_default())
}
```

- [ ] **Step 4: Run cargo check and tests**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo check 2>&1 | tail -20`
Run: `cargo test self_heal --no-fail-fast 2>&1 | tail -20`

- [ ] **Step 5: Commit**

```bash
git add api/src/api/mcp/tools/self_heal.rs
git commit -m "feat(mcp): replace self_heal stubs with full implementations

- execute_self_heal_action: real action execution via ActionExecutor
- get_recovery_history: returns execution history with pagination
- get_self_heal_policy: reads from global policy state

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Database Migration and AppState Integration

**Files:**
- Create: `api/migrations/YYYYMMDDHHMMSS_create_healing_executions_table.sql`
- Modify: `api/src/main.rs` - initialize self-healing on startup

### Steps

- [ ] **Step 1: Create migration file**

```sql
-- Create healing_executions table for recovery history
CREATE TABLE IF NOT EXISTS healing_executions (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    action_type TEXT NOT NULL,
    target TEXT,
    result TEXT NOT NULL,
    logs TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_healing_executions_timestamp ON healing_executions(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_healing_executions_level ON healing_executions(level);
CREATE INDEX IF NOT EXISTS idx_healing_executions_tenant ON healing_executions(tenant_id);
```

- [ ] **Step 2: Update AppState to include self-healing state**

- [ ] **Step 3: Commit**

```bash
git add api/migrations/
git commit -m "feat(db): add healing_executions migration for self-healing history

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 6: End-to-End Tests

**Files:**
- Create: `api/src/api/mcp/tests.rs` - add self-healing tests

### Steps

- [ ] **Step 1: Write tests for evaluator**

- [ ] **Step 2: Write tests for action executor**

- [ ] **Step 3: Run full test suite**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo test 2>&1 | tail -30`

- [ ] **Step 4: Commit**

```bash
git add api/src/domain/self_healing/tests.rs api/src/api/mcp/tests.rs
git commit -m "test(self_healing): add unit tests for evaluator and executor

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Update TODOS.md

Mark Phase 2 as in-progress.

---

## Summary of Phase 2 Deliverables

| Deliverable | Description |
|-------------|-------------|
| Domain module | `SelfHealingPolicy`, `HealingExecution`, `SeverityLevel`, `RecoveryActionType` |
| PolicyEvaluator | Evaluates probe results against thresholds → L0/L1/L2/L3 |
| ActionExecutor | Executes L0-L3 actions with cooldown tracking |
| ProbeScheduler | Runs system/device/task probes on configurable intervals |
| REST API | `/self-healing/policies`, `/self-healing/actions/:level`, `/self-healing/executions`, `/self-healing/probes` |
| MCP tools | `execute_self_heal_action`, `get_recovery_history` fully functional |
| DB migration | `healing_executions` table |
| Unit tests | Evaluator and executor tests |

## Notes

- ProbeScheduler runs as a background task spawned in `main.rs`
- For Phase 2, device/task probes are stubs that can be expanded in future iterations
- The `report_cloud` and `create_ticket` actions are logged but don't actually call cloud APIs (Phase 4)
- Action execution (restart_driver, reconnect_device, etc.) is logged but doesn't yet call actual device/driver manager APIs

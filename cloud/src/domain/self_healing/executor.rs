use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::entity::{ExecutionResult, HealingExecution, RecoveryActionType, SeverityLevel};
use super::errors::{Result, SelfHealingError};

/// Executes recovery actions based on policy
pub struct ActionExecutor {
    last_execution: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl ActionExecutor {
    /// Create a new ActionExecutor
    pub fn new() -> Self {
        Self {
            last_execution: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute a healing action
    ///
    /// Checks cooldown before execution, then executes the appropriate action handler.
    /// Returns a HealingExecution with tenant_id set to "default" (placeholder).
    pub async fn execute(
        &self,
        level: SeverityLevel,
        action_type: RecoveryActionType,
        target: String,
        cooldown_secs: u64,
    ) -> Result<HealingExecution> {
        // Check cooldown
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

        // Update last execution time
        {
            let mut last_exec = self.last_execution.write().await;
            last_exec.insert(key, Utc::now());
        }

        // Execute the appropriate action
        let execution = match action_type {
            RecoveryActionType::RestartDriver => {
                self.execute_restart_driver(level, target).await?
            }
            RecoveryActionType::RejoinLora => self.execute_rejoin_lora(level, target).await?,
            RecoveryActionType::ReconnectDevice => {
                self.execute_reconnect_device(level, target).await?
            }
            RecoveryActionType::CleanLogs => self.execute_clean_logs(level, target).await?,
            RecoveryActionType::ReportCloud => self.execute_report_cloud(level, target).await?,
            RecoveryActionType::CreateTicket => self.execute_create_ticket(level, target).await?,
            RecoveryActionType::LogOnly => {
                // LogOnly doesn't need special handling, just return success
                HealingExecution::new(
                    "default".to_string(),
                    level,
                    action_type,
                    target,
                    ExecutionResult::Success,
                )
            }
        };

        Ok(execution)
    }

    async fn execute_restart_driver(
        &self,
        level: SeverityLevel,
        target: String,
    ) -> Result<HealingExecution> {
        tracing::info!("Restarting driver: {}", target);

        // Simulate driver restart logic
        // In production, this would call the driver management API

        Ok(HealingExecution::new(
            "default".to_string(),
            level,
            RecoveryActionType::RestartDriver,
            target,
            ExecutionResult::Success,
        ))
    }

    async fn execute_rejoin_lora(
        &self,
        level: SeverityLevel,
        target: String,
    ) -> Result<HealingExecution> {
        tracing::info!("Rejoining LoRa network: {}", target);

        // Simulate LoRa rejoin logic

        Ok(HealingExecution::new(
            "default".to_string(),
            level,
            RecoveryActionType::RejoinLora,
            target,
            ExecutionResult::Success,
        ))
    }

    async fn execute_reconnect_device(
        &self,
        level: SeverityLevel,
        target: String,
    ) -> Result<HealingExecution> {
        tracing::info!("Reconnecting device: {}", target);

        // Simulate device reconnection logic

        Ok(HealingExecution::new(
            "default".to_string(),
            level,
            RecoveryActionType::ReconnectDevice,
            target,
            ExecutionResult::Success,
        ))
    }

    async fn execute_clean_logs(
        &self,
        level: SeverityLevel,
        target: String,
    ) -> Result<HealingExecution> {
        tracing::info!("Cleaning logs: {}", target);

        // Simulate log cleanup

        Ok(HealingExecution::new(
            "default".to_string(),
            level,
            RecoveryActionType::CleanLogs,
            target,
            ExecutionResult::Success,
        ))
    }

    async fn execute_report_cloud(
        &self,
        level: SeverityLevel,
        target: String,
    ) -> Result<HealingExecution> {
        tracing::info!("Reporting to cloud: {}", target);

        // Simulate cloud reporting

        Ok(HealingExecution::new(
            "default".to_string(),
            level,
            RecoveryActionType::ReportCloud,
            target,
            ExecutionResult::Success,
        ))
    }

    async fn execute_create_ticket(
        &self,
        level: SeverityLevel,
        target: String,
    ) -> Result<HealingExecution> {
        tracing::info!("Creating support ticket: {}", target);

        // Simulate ticket creation

        Ok(HealingExecution::new(
            "default".to_string(),
            level,
            RecoveryActionType::CreateTicket,
            target,
            ExecutionResult::Success,
        ))
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_with_cooldown_blocks() {
        let executor = ActionExecutor::new();

        // First execution should succeed
        let result = executor
            .execute(
                SeverityLevel::L1,
                RecoveryActionType::RestartDriver,
                "driver-1".to_string(),
                300,
            )
            .await;
        assert!(result.is_ok());

        // Immediate second execution should fail due to cooldown
        let result = executor
            .execute(
                SeverityLevel::L1,
                RecoveryActionType::RestartDriver,
                "driver-1".to_string(),
                300,
            )
            .await;
        assert!(matches!(result, Err(SelfHealingError::CooldownActive(_))));
    }

    #[tokio::test]
    async fn test_execute_different_targets_no_cooldown() {
        let executor = ActionExecutor::new();

        // Execute on driver-1
        let result = executor
            .execute(
                SeverityLevel::L1,
                RecoveryActionType::RestartDriver,
                "driver-1".to_string(),
                300,
            )
            .await;
        assert!(result.is_ok());

        // Execute on driver-2 should succeed (different target)
        let result = executor
            .execute(
                SeverityLevel::L1,
                RecoveryActionType::RestartDriver,
                "driver-2".to_string(),
                300,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_log_only() {
        let executor = ActionExecutor::new();

        let result = executor
            .execute(
                SeverityLevel::L0,
                RecoveryActionType::LogOnly,
                "gateway-1".to_string(),
                0,
            )
            .await;

        assert!(result.is_ok());
        let execution = result.unwrap();
        assert_eq!(execution.result, ExecutionResult::Success);
        assert_eq!(execution.tenant_id, "default");
    }

    #[tokio::test]
    async fn test_execution_has_tenant_id() {
        let executor = ActionExecutor::new();

        let result = executor
            .execute(
                SeverityLevel::L1,
                RecoveryActionType::ReconnectDevice,
                "device-1".to_string(),
                60,
            )
            .await;

        assert!(result.is_ok());
        let execution = result.unwrap();
        assert_eq!(execution.tenant_id, "default");
        assert!(!execution.id.is_empty());
        assert_eq!(execution.level, SeverityLevel::L1);
        assert_eq!(execution.action_type, RecoveryActionType::ReconnectDevice);
        assert_eq!(execution.target, "device-1");
    }
}

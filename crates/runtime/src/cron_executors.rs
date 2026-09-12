//! Cron job executors.
//!
//! These executors need concrete infrastructure types (`DataServer`) and
//! persistence access (via `crate::ports` traits injected by the composition
//! root) and therefore live in the runtime crate rather than in
//! `tinyiothub_scheduler`, which depends only on `core` contracts. The
//! application wires them into the scheduler's `ExecutorRegistry`.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;

pub use tinyiothub_core::cron::{ExecutionResult, ExecutorError, JobExecutor};
use tinyiothub_core::models::cron_job::CronJob;

use crate::ports::{ApprovalTimeoutStore, EventRetentionStore, ThingCommandQueries};

/// Executes device commands via DataServer.
pub struct ThingCommandExecutor {
    data_server: Arc<crate::data_server::DataServer>,
    commands: Arc<dyn ThingCommandQueries>,
}

impl ThingCommandExecutor {
    pub fn new(data_server: Arc<crate::data_server::DataServer>, commands: Arc<dyn ThingCommandQueries>) -> Self {
        Self { data_server, commands }
    }
}

#[async_trait]
impl JobExecutor for ThingCommandExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "device_command"
    }

    async fn execute(&self, job: &CronJob, _run_id: &str) -> std::result::Result<ExecutionResult, ExecutorError> {
        let thing_id = job
            .target_thing_id()
            .ok_or_else(|| ExecutorError::InvalidConfig("missing thing_id in job config".to_string()))?;
        let command_name = job
            .target_command_name()
            .ok_or_else(|| ExecutorError::InvalidConfig("missing command_name in job config".to_string()))?;

        let start = Instant::now();

        // Look up the device command via the injected queries port
        let mut command = self
            .commands
            .find_by_thing_and_name(&thing_id, &command_name)
            .await
            .map_err(|e| ExecutorError::InvalidConfig(format!("DB error looking up command: {}", e)))?
            .ok_or_else(|| {
                ExecutorError::InvalidConfig(format!(
                    "command '{}' not found for device '{}'",
                    command_name, thing_id
                ))
            })?;

        // Apply params from job config if provided
        if let Some(params) = job.target_command_params() {
            command.parameters = Some(params);
        }

        // Execute via DataServer
        self.data_server.execute_command(command).map_err(|e| {
            ExecutorError::CommandFailed(format!(
                "failed to queue command '{}/{}': {}",
                thing_id, command_name, e
            ))
        })?;

        let duration_ms = start.elapsed().as_millis() as i64;

        Ok(ExecutionResult {
            status: "success".to_string(),
            output: Some(format!("command '{}/{}' queued for execution", thing_id, command_name)),
            error_message: None,
            duration_ms,
        })
    }
}

/// Deletes occurrence-type events older than `retention_days`.
///
/// The events table mixes immutable audit rows (is_status=0, log history —
/// safe to time-purge) with mutable status rows (is_status=1, the LIVE
/// current-state of devices — never time-purged). This distinction is the
/// whole point of the executor: a naive time-based purge would silently
/// destroy the current state of quiet devices (eng-review OV-1/X1).
pub struct EventRetentionExecutor {
    store: Arc<dyn EventRetentionStore>,
}

impl EventRetentionExecutor {
    pub fn new(store: Arc<dyn EventRetentionStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl JobExecutor for EventRetentionExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "event_retention"
    }

    async fn execute(&self, job: &CronJob, _run_id: &str) -> std::result::Result<ExecutionResult, ExecutorError> {
        let start = Instant::now();

        let config: Value =
            serde_json::from_str(&job.config).map_err(|e| ExecutorError::InvalidConfig(e.to_string()))?;
        let retention_days = config
            .get("retention_days")
            .and_then(|v| v.as_i64())
            .unwrap_or(90)
            .max(1);
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);

        let deleted = self
            .store
            .delete_occurrence_events_before(&cutoff.to_rfc3339())
            .await
            .map_err(|e| ExecutorError::CommandFailed(format!("retention purge failed: {}", e)))?;
        let duration_ms = start.elapsed().as_millis() as i64;
        tracing::info!(deleted, retention_days, "events retention purge complete");

        Ok(ExecutionResult {
            status: "success".to_string(),
            output: Some(format!(
                "deleted {} occurrence-type events older than {} days",
                deleted, retention_days
            )),
            error_message: None,
            duration_ms,
        })
    }
}

/// T6：审批超时升级（approval_timeout）——awaiting_approval 超过
/// `timeout_hours`（默认 24）的判断自动升级为工单。防"审批堆积"（创始人
/// 批评的"工单永远不关闭"问题的审批版变体）。
pub struct ApprovalTimeoutExecutor {
    store: Arc<dyn ApprovalTimeoutStore>,
}

impl ApprovalTimeoutExecutor {
    pub fn new(store: Arc<dyn ApprovalTimeoutStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl JobExecutor for ApprovalTimeoutExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "approval_timeout"
    }

    async fn execute(&self, job: &CronJob, _run_id: &str) -> std::result::Result<ExecutionResult, ExecutorError> {
        let start = Instant::now();

        let config: Value =
            serde_json::from_str(&job.config).map_err(|e| ExecutorError::InvalidConfig(e.to_string()))?;
        let timeout_hours = config
            .get("timeout_hours")
            .and_then(|v| v.as_i64())
            .unwrap_or(24)
            .max(1);
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(timeout_hours);

        let escalated = self
            .store
            .escalate_stale_approvals(&cutoff.to_rfc3339())
            .await
            .map_err(|e| ExecutorError::CommandFailed(format!("approval timeout escalation failed: {}", e)))?;
        let duration_ms = start.elapsed().as_millis() as i64;
        tracing::info!(escalated, timeout_hours, "approval timeout sweep complete");

        Ok(ExecutionResult {
            status: "success".to_string(),
            output: Some(format!(
                "escalated {} judgments pending approval longer than {}h",
                escalated, timeout_hours
            )),
            error_message: None,
            duration_ms,
        })
    }
}

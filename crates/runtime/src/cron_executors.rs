//! Db-bound cron job executors.
//!
//! These executors need concrete infrastructure types (`DataServer`,
//! `Database`) and therefore live in the runtime crate rather than in
//! `tinyiothub_scheduler`, which depends only on `core` contracts. The
//! application wires them into the scheduler's `ExecutorRegistry`.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;

pub use tinyiothub_core::cron::{ExecutionResult, ExecutorError, JobExecutor};
use tinyiothub_core::models::cron_job::CronJob;
use tinyiothub_storage::database::Database;

/// Executes device commands via DataServer.
pub struct DeviceCommandExecutor {
    data_server: Arc<crate::data_server::DataServer>,
    database: Database,
}

impl DeviceCommandExecutor {
    pub fn new(data_server: Arc<crate::data_server::DataServer>, database: Database) -> Self {
        Self { data_server, database }
    }
}

#[async_trait]
impl JobExecutor for DeviceCommandExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "device_command"
    }

    async fn execute(&self, job: &CronJob, _run_id: &str) -> std::result::Result<ExecutionResult, ExecutorError> {
        let device_id = job
            .target_device_id()
            .ok_or_else(|| ExecutorError::InvalidConfig("missing device_id in job config".to_string()))?;
        let command_name = job
            .target_command_name()
            .ok_or_else(|| ExecutorError::InvalidConfig("missing command_name in job config".to_string()))?;

        let start = Instant::now();

        // Look up the device command from DB
        let mut command = tinyiothub_storage::device_command::find_device_command_by_device_and_name(
            &self.database,
            &device_id,
            &command_name,
        )
        .await
        .map_err(|e| ExecutorError::InvalidConfig(format!("DB error looking up command: {}", e)))?
        .ok_or_else(|| {
            ExecutorError::InvalidConfig(format!(
                "command '{}' not found for device '{}'",
                command_name, device_id
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
                device_id, command_name, e
            ))
        })?;

        let duration_ms = start.elapsed().as_millis() as i64;

        Ok(ExecutionResult {
            status: "success".to_string(),
            output: Some(format!("command '{}/{}' queued for execution", device_id, command_name)),
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
    database: Database,
}

impl EventRetentionExecutor {
    pub fn new(database: Database) -> Self {
        Self { database }
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
            .database
            .execute_with_params(
                "DELETE FROM events WHERE is_status = 0 AND timestamp < ?",
                &[&cutoff.to_rfc3339()],
            )
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

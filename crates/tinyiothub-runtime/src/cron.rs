//! Cron job executor registry and concrete executors.

use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

pub use tinyiothub_core::cron::{ExecutionResult, ExecutorError, JobExecutor};
use tinyiothub_core::models::cron_job::CronJob;

/// Executes shell scripts via `tokio::process::Command`.
pub struct ShellExecutor;

#[async_trait]
impl JobExecutor for ShellExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "shell"
    }

    async fn execute(
        &self,
        job: &CronJob,
        run_id: &str,
    ) -> std::result::Result<ExecutionResult, ExecutorError> {
        let config: Value =
            serde_json::from_str(&job.config).map_err(|e| ExecutorError::InvalidConfig(e.to_string()))?;

        let script = config
            .get("script")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutorError::InvalidConfig("missing 'script' field".to_string()))?;

        let interpreter = config
            .get("interpreter")
            .and_then(|v| v.as_str())
            .unwrap_or("sh");

        let allowed = match interpreter {
            "sh" | "bash" | "python" | "python3" => interpreter,
            other => {
                return Err(ExecutorError::InvalidConfig(format!(
                    "interpreter '{other}' is not allowed"
                )));
            }
        };

        let working_dir = config.get("working_dir").and_then(|v| v.as_str());
        if let Some(dir) = working_dir {
            if dir.contains("..") {
                return Err(ExecutorError::InvalidConfig(format!(
                    "working_dir '{dir}' contains '..' which is not allowed"
                )));
            }
        }

        let timeout_secs = job.timeout_seconds.max(1) as u64;
        let start = Instant::now();

        let mut cmd = Command::new(allowed);
        cmd.arg("-c").arg(script);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let result = timeout(std::time::Duration::from_secs(timeout_secs), cmd.output()).await;
        let duration_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                if output.status.success() {
                    Ok(ExecutionResult {
                        status: "success".to_string(),
                        output: Some(stdout).filter(|s| !s.is_empty()),
                        error_message: None,
                        duration_ms,
                    })
                } else {
                    let exit = output.status.code().map_or("unknown".to_string(), |c| c.to_string());
                    Err(ExecutorError::CommandFailed(format!(
                        "exit code {exit}, run_id={run_id}, stdout={stdout}, stderr={stderr}"
                    )))
                }
            }
            Ok(Err(e)) => Err(ExecutorError::Io(e)),
            Err(_) => Err(ExecutorError::Timeout(timeout_secs)),
        }
    }
}

/// Stub executor for agent-type jobs.
pub struct AgentExecutor;

#[async_trait]
impl JobExecutor for AgentExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "agent"
    }

    async fn execute(
        &self,
        job: &CronJob,
        _run_id: &str,
    ) -> std::result::Result<ExecutionResult, ExecutorError> {
        let config: Value =
            serde_json::from_str(&job.config).map_err(|e| ExecutorError::InvalidConfig(e.to_string()))?;

        let task = config
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutorError::InvalidConfig("missing 'task' field".to_string()))?;

        let timeout_secs = job.timeout_seconds.max(1) as u64;
        let start = Instant::now();

        let result = timeout(
            std::time::Duration::from_secs(timeout_secs),
            async {
                tokio::task::yield_now().await;
            },
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(()) => Ok(ExecutionResult {
                status: "success".to_string(),
                output: Some(format!("agent execution stub: {task}")),
                error_message: None,
                duration_ms,
            }),
            Err(_) => Err(ExecutorError::Timeout(timeout_secs)),
        }
    }
}

/// Stub executor for device-command-type jobs.
pub struct DeviceCommandExecutor;

#[async_trait]
impl JobExecutor for DeviceCommandExecutor {
    fn can_handle(&self, job_type: &str) -> bool {
        job_type == "device_command"
    }

    async fn execute(
        &self,
        job: &CronJob,
        _run_id: &str,
    ) -> std::result::Result<ExecutionResult, ExecutorError> {
        let device_id = job
            .target_device_id()
            .ok_or_else(|| ExecutorError::InvalidConfig("missing device_id".to_string()))?;
        let command_name = job
            .target_command_name()
            .ok_or_else(|| ExecutorError::InvalidConfig("missing command_name".to_string()))?;

        let timeout_secs = job.timeout_seconds.max(1) as u64;
        let start = Instant::now();

        let result = timeout(
            std::time::Duration::from_secs(timeout_secs),
            async {
                tokio::task::yield_now().await;
            },
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(()) => Ok(ExecutionResult {
                status: "success".to_string(),
                output: Some(format!("device command stub: {device_id}/{command_name}")),
                error_message: None,
                duration_ms,
            }),
            Err(_) => Err(ExecutorError::Timeout(timeout_secs)),
        }
    }
}

/// Registry that holds all available executors and routes jobs by type.
pub struct ExecutorRegistry {
    executors: Vec<Box<dyn JobExecutor>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: vec![
                Box::new(ShellExecutor),
                Box::new(AgentExecutor),
                Box::new(DeviceCommandExecutor),
            ],
        }
    }

    pub fn register(&mut self, executor: Box<dyn JobExecutor>) {
        self.executors.push(executor);
    }

    pub fn find(&self, job_type: &str) -> Option<&dyn JobExecutor> {
        self.executors
            .iter()
            .find(|e| e.can_handle(job_type))
            .map(|e| e.as_ref())
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_find_shell() {
        let registry = ExecutorRegistry::new();
        let executor = registry.find("shell");
        assert!(executor.is_some());
        assert!(executor.unwrap().can_handle("shell"));
    }

    #[test]
    fn test_registry_find_agent() {
        let registry = ExecutorRegistry::new();
        let executor = registry.find("agent");
        assert!(executor.is_some());
    }

    #[test]
    fn test_registry_find_device_command() {
        let registry = ExecutorRegistry::new();
        let executor = registry.find("device_command");
        assert!(executor.is_some());
    }

    #[test]
    fn test_registry_find_unknown() {
        let registry = ExecutorRegistry::new();
        assert!(registry.find("unknown").is_none());
    }
}

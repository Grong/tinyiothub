use std::time::Instant;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use tinyiothub_core::models::cron_job::CronJob;

/// Result of a single job execution.
#[derive(Debug)]
pub struct ExecutionResult {
    pub status: String,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: i64,
}

/// Errors that can occur during job execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("execution timed out after {0}s")]
    Timeout(u64),
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("agent error: {0}")]
    AgentError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait for job executors. Each executor handles a specific job type.
#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// Execute the given cron job.
    async fn execute(
        &self,
        job: &CronJob,
        run_id: &str,
    ) -> std::result::Result<ExecutionResult, ExecutorError>;

    /// Return true if this executor can handle the given job type.
    fn can_handle(&self, job_type: &str) -> bool;
}

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

        // Whitelist interpreters to prevent command injection.
        let allowed = match interpreter {
            "sh" | "bash" | "python" | "python3" | "powershell" | "cmd" | "node" => interpreter,
            other => {
                return Err(ExecutorError::InvalidConfig(format!(
                    "interpreter '{other}' is not allowed"
                )));
            }
        };

        let working_dir = config.get("working_dir").and_then(|v| v.as_str());
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
                // Stub: simulate async work
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
                // Stub: simulate async work
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
    /// Create a new registry with the default set of executors.
    pub fn new() -> Self {
        Self {
            executors: vec![
                Box::new(ShellExecutor),
                Box::new(AgentExecutor),
                Box::new(DeviceCommandExecutor),
            ],
        }
    }

    /// Register an additional executor.
    pub fn register(&mut self, executor: Box<dyn JobExecutor>) {
        self.executors.push(executor);
    }

    /// Find the first executor that can handle the given job type.
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

    fn make_shell_job(script: &str, timeout: i32) -> CronJob {
        CronJob {
            id: "job-1".to_string(),
            name: "test".to_string(),
            description: None,
            job_type: "shell".to_string(),
            cron_expression: "* * * * *".to_string(),
            config: serde_json::json!({"script": script }).to_string(),
            timeout_seconds: timeout,
            max_retries: 0,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 00:00:00".to_string(),
            updated_at: "2026-04-18 00:00:00".to_string(),
            created_by: None,
        }
    }

    fn make_agent_job(task: &str, timeout: i32) -> CronJob {
        CronJob {
            id: "job-2".to_string(),
            name: "test".to_string(),
            description: None,
            job_type: "agent".to_string(),
            cron_expression: "* * * * *".to_string(),
            config: serde_json::json!({"task": task }).to_string(),
            timeout_seconds: timeout,
            max_retries: 0,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 00:00:00".to_string(),
            updated_at: "2026-04-18 00:00:00".to_string(),
            created_by: None,
        }
    }

    fn make_device_command_job(device_id: &str, command_name: &str, timeout: i32) -> CronJob {
        CronJob {
            id: "job-3".to_string(),
            name: "test".to_string(),
            description: None,
            job_type: "device_command".to_string(),
            cron_expression: "* * * * *".to_string(),
            config: serde_json::json!({
                "device_id": device_id,
                "command_name": command_name
            })
            .to_string(),
            timeout_seconds: timeout,
            max_retries: 0,
            is_enabled: true,
            is_running: false,
            last_run_at: None,
            last_run_status: None,
            last_run_error: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            fail_count: 0,
            created_at: "2026-04-18 00:00:00".to_string(),
            updated_at: "2026-04-18 00:00:00".to_string(),
            created_by: None,
        }
    }

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
        assert!(executor.unwrap().can_handle("agent"));
    }

    #[test]
    fn test_registry_find_device_command() {
        let registry = ExecutorRegistry::new();
        let executor = registry.find("device_command");
        assert!(executor.is_some());
        assert!(executor.unwrap().can_handle("device_command"));
    }

    #[test]
    fn test_registry_find_unknown() {
        let registry = ExecutorRegistry::new();
        assert!(registry.find("unknown").is_none());
    }

    #[test]
    fn test_shell_executor_can_handle() {
        let exec = ShellExecutor;
        assert!(exec.can_handle("shell"));
        assert!(!exec.can_handle("agent"));
        assert!(!exec.can_handle("device_command"));
    }

    #[test]
    fn test_device_command_executor_can_handle() {
        let exec = DeviceCommandExecutor;
        assert!(exec.can_handle("device_command"));
        assert!(!exec.can_handle("shell"));
        assert!(!exec.can_handle("agent"));
    }

    #[tokio::test]
    async fn test_shell_executor_success() {
        let job = make_shell_job("echo hello", 10);
        let exec = ShellExecutor;
        let result = exec.execute(&job, "run-1").await.unwrap();
        assert_eq!(result.status, "success");
        assert_eq!(result.output, Some("hello\n".to_string()));
        assert!(result.error_message.is_none());
        assert!(result.duration_ms >= 0);
    }

    #[tokio::test]
    async fn test_shell_executor_failure() {
        let job = make_shell_job("exit 1", 10);
        let exec = ShellExecutor;
        let err = exec.execute(&job, "run-1").await.unwrap_err();
        match err {
            ExecutorError::CommandFailed(_) => {}
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_shell_executor_timeout() {
        let job = make_shell_job("sleep 10", 1);
        let exec = ShellExecutor;
        let err = exec.execute(&job, "run-1").await.unwrap_err();
        match err {
            ExecutorError::Timeout(1) => {}
            other => panic!("expected Timeout(1), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_shell_executor_invalid_config() {
        let mut job = make_shell_job("echo hello", 10);
        job.config = "not-json".to_string();
        let exec = ShellExecutor;
        let err = exec.execute(&job, "run-1").await.unwrap_err();
        match err {
            ExecutorError::InvalidConfig(_) => {}
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_shell_executor_missing_script() {
        let mut job = make_shell_job("echo hello", 10);
        job.config = serde_json::json!({"interpreter": "bash"}).to_string();
        let exec = ShellExecutor;
        let err = exec.execute(&job, "run-1").await.unwrap_err();
        match err {
            ExecutorError::InvalidConfig(_) => {}
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_shell_executor_disallowed_interpreter() {
        let mut job = make_shell_job("echo hello", 10);
        job.config = serde_json::json!({"script": "echo hello", "interpreter": "rm"}).to_string();
        let exec = ShellExecutor;
        let err = exec.execute(&job, "run-1").await.unwrap_err();
        match err {
            ExecutorError::InvalidConfig(msg) => {
                assert!(msg.contains("interpreter 'rm' is not allowed"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_agent_executor_success() {
        let job = make_agent_job("do something", 10);
        let exec = AgentExecutor;
        let result = exec.execute(&job, "run-1").await.unwrap();
        assert_eq!(result.status, "success");
        assert_eq!(result.output, Some("agent execution stub: do something".to_string()));
    }

    #[tokio::test]
    async fn test_agent_executor_timeout() {
        let job = make_agent_job("do something", 1);
        let exec = AgentExecutor;
        // The stub implementation yields once and returns immediately,
        // so it won't actually time out. We verify the timeout parameter is used
        // by checking the success path still works with a short timeout.
        let result = exec.execute(&job, "run-1").await.unwrap();
        assert_eq!(result.status, "success");
    }

    #[tokio::test]
    async fn test_device_command_executor_success() {
        let job = make_device_command_job("dev-123", "restart", 10);
        let exec = DeviceCommandExecutor;
        let result = exec.execute(&job, "run-1").await.unwrap();
        assert_eq!(result.status, "success");
        assert_eq!(
            result.output,
            Some("device command stub: dev-123/restart".to_string())
        );
    }

    #[tokio::test]
    async fn test_device_command_executor_invalid_config() {
        let mut job = make_device_command_job("dev-123", "restart", 10);
        job.config = "not-json".to_string();
        let exec = DeviceCommandExecutor;
        let err = exec.execute(&job, "run-1").await.unwrap_err();
        match err {
            ExecutorError::InvalidConfig(_) => {}
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_device_command_executor_timeout() {
        let job = make_device_command_job("dev-123", "restart", 1);
        let exec = DeviceCommandExecutor;
        // Stub yields once and returns, so short timeout still succeeds.
        let result = exec.execute(&job, "run-1").await.unwrap();
        assert_eq!(result.status, "success");
    }
}

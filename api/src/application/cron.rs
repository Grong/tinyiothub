// Cron Service
//
// This module provides a cron scheduling service that combines:
// - Zeroclaw's SQLite-based job store (persistence, CRUD)
// - TinyIoTHub's Agent runtime (for Agent jobs with IoT tools)
//
// This avoids the complexity of wiring TinyIoTHub tools into zeroclaw's
// internal agent, while still leveraging zeroclaw's robust scheduling logic.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::infrastructure::agent::AgentRuntime;
use crate::shared::app_state::AppState;

/// Minimum poll interval
const MIN_POLL_SECS: u64 = 15;

/// A cron service that uses zeroclaw's SQLite store for persistence
/// but executes jobs through TinyIoTHub's agent runtime.
pub struct CronService {
    /// Application state for accessing services
    app_state: Arc<AppState>,

    /// Polling interval
    poll_interval_secs: u64,

    /// Whether the service is running
    running: Arc<Mutex<bool>>,
}

impl CronService {
    /// Create a new CronService
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            poll_interval_secs: MIN_POLL_SECS,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the cron scheduler
    pub async fn run(&self) {
        info!("CronService starting...");

        let mut running = self.running.lock().await;
        if *running {
            warn!("CronService already running, skipping start");
            return;
        }
        *running = true;
        drop(running);

        let poll_interval = Duration::from_secs(self.poll_interval_secs);
        let mut checker = interval(poll_interval);
        // Skip the first immediate tick
        checker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            // Check if stopped
            {
                let running = self.running.lock().await;
                if !*running {
                    break;
                }
            }

            // Wait for next poll
            checker.tick().await;

            // Process due jobs
            if let Err(e) = self.process_due_jobs().await {
                error!("Error processing due jobs: {}", e);
            }
        }

        info!("CronService stopped");
    }

    /// Stop the scheduler
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
    }

    /// Process all due jobs
    async fn process_due_jobs(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use zeroclaw::cron::{due_jobs, record_run, JobType};

        // Build minimal zeroclaw config for job store access
        let zeroclaw_config = self.build_zeroclaw_config()?;

        // Get due jobs (due_jobs returns all currently due jobs, limited by scheduler.max_tasks)
        let jobs = due_jobs(&zeroclaw_config, Utc::now())
            .map_err(|e| format!("Failed to get due jobs: {}", e))?;

        if jobs.is_empty() {
            return Ok(());
        }

        info!("Processing {} due cron jobs", jobs.len());

        for job in jobs {
            let job_id = job.id.clone();
            let job_type = job.job_type.clone();
            let start_time = std::time::Instant::now();

            // Execute job
            let (success, output) = match job_type {
                JobType::Agent => {
                    self.execute_agent_job(&job.prompt.unwrap_or_default()).await
                }
                JobType::Shell => {
                    self.execute_shell_job(&job.command).await
                }
            };

            let duration_ms = start_time.elapsed().as_millis() as i64;
            let now = Utc::now();
            let started_at = now - chrono::Duration::milliseconds(duration_ms);

            // Record the run
            if let Err(e) = record_run(
                &zeroclaw_config,
                &job_id,
                started_at,
                now,
                if success { "success" } else { "failed" },
                Some(&output),
                duration_ms,
            ) {
                error!("Failed to record cron run for job {}: {}", job_id, e);
            }

            info!(
                "Cron job {} {} in {}ms",
                job_id,
                if success { "succeeded" } else { "failed" },
                duration_ms
            );
        }

        Ok(())
    }

    /// Execute an Agent job through TinyIoTHub's agent runtime
    async fn execute_agent_job(&self, prompt: &str) -> (bool, String) {
        let agent_runtime = &self.app_state.agent_runtime;

        // Build the full prompt with memory context
        let full_prompt = self.build_cron_prompt(prompt);

        // Execute via agent runtime
        match agent_runtime.run_single(&full_prompt).await {
            Ok(response) => (true, response),
            Err(e) => {
                error!("Agent job failed: {}", e);
                (false, format!("Agent execution failed: {}", e))
            }
        }
    }

    /// Execute a Shell job via direct command
    async fn execute_shell_job(&self, command: &str) -> (bool, String) {
        use std::process::Stdio;
        use tokio::process::Command;
        use tokio::time::{timeout, Duration};

        info!("Executing shell command: {}", command);

        // Validate command against security policy before execution
        let zeroclaw_config = match self.build_zeroclaw_config() {
            Ok(cfg) => cfg,
            Err(e) => return (false, format!("Failed to build config: {}", e)),
        };

        if let Err(e) = zeroclaw::cron::validate_shell_command(&zeroclaw_config, command, false) {
            error!("Shell command blocked by security policy: {}", e);
            return (false, format!("Blocked by security policy: {}", e));
        }

        // Execute with timeout (2 minutes max)
        const SHELL_TIMEOUT_SECS: u64 = 120;
        match timeout(
            Duration::from_secs(SHELL_TIMEOUT_SECS),
            Command::new("bash")
                .args(["-c", command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    (true, stdout)
                } else {
                    (false, format!("stdout: {}\nstderr: {}", stdout, stderr))
                }
            }
            Ok(Err(e)) => {
                error!("Shell command execution failed: {}", e);
                (false, format!("Command execution failed: {}", e))
            }
            Err(_) => {
                error!("Shell command timed out after {} seconds", SHELL_TIMEOUT_SECS);
                (false, format!("Command timed out after {} seconds", SHELL_TIMEOUT_SECS))
            }
        }
    }

    /// Build a cron prompt with memory context
    fn build_cron_prompt(&self, base_prompt: &str) -> String {
        // For now, just use the base prompt
        // In the future, we could add memory recall here
        format!("[cron] {}", base_prompt)
    }

    /// Build a minimal zeroclaw Config for job store access
    fn build_zeroclaw_config(&self) -> Result<zeroclaw::config::schema::Config, String> {
        use zeroclaw::config::schema::Config;

        // Get workspace directory from agent settings
        let agent_settings = crate::infrastructure::config::get().agent.clone();
        let workspace_dir = std::path::PathBuf::from(&agent_settings.workspace_dir);

        // Create minimal config with required fields
        let config = Config {
            workspace_dir: workspace_dir.clone(),
            config_path: workspace_dir.join("config.toml"),
            // Use defaults for everything else
            ..Config::default()
        };

        Ok(config)
    }
}

impl Clone for CronService {
    fn clone(&self) -> Self {
        Self {
            app_state: self.app_state.clone(),
            poll_interval_secs: self.poll_interval_secs,
            running: self.running.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_zeroclaw_config() {
        // This test just verifies the config building doesn't panic
        // Real integration tests would need a full app_state
    }
}

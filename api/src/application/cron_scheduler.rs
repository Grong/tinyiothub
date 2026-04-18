use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use futures::FutureExt;
use tokio::sync::{broadcast, Semaphore};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::domain::cron::executor::{ExecutionResult, ExecutorError, ExecutorRegistry};
use crate::domain::cron::repository::{CronJobRepository, CronRunRepository};
use crate::dto::entity::cron_job::CronJob;
use crate::shared::error::{Error, Result};

/// Cron job scheduler service that polls for due jobs and executes them.
pub struct CronSchedulerService {
    job_repo: Arc<dyn CronJobRepository>,
    run_repo: Arc<dyn CronRunRepository>,
    registry: Arc<ExecutorRegistry>,
    shutdown_tx: broadcast::Sender<()>,
    poll_interval: std::time::Duration,
    max_concurrent: usize,
}

impl CronSchedulerService {
    /// Create a new scheduler service with the given repositories.
    pub fn new(
        job_repo: Arc<dyn CronJobRepository>,
        run_repo: Arc<dyn CronRunRepository>,
    ) -> Self {
        Self {
            job_repo,
            run_repo,
            registry: Arc::new(ExecutorRegistry::new()),
            shutdown_tx: broadcast::channel(1).0,
            poll_interval: std::time::Duration::from_secs(15),
            max_concurrent: 10,
        }
    }

    /// Start the background polling loop.
    pub fn start(&self) -> JoinHandle<Result<()>> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let job_repo = self.job_repo.clone();
        let run_repo = self.run_repo.clone();
        let registry = self.registry.clone();
        let poll_interval = self.poll_interval;
        let max_concurrent = self.max_concurrent;

        info!("CronSchedulerService started");

        tokio::spawn(async move {
            // Crash recovery: clear any stale is_running flags from previous session.
            if let Err(e) = job_repo.clear_all_running(None).await {
                warn!("Failed to clear stale running flags on startup: {}", e);
            }

            let mut interval = tokio::time::interval(poll_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = tick_impl(
                            job_repo.clone(),
                            run_repo.clone(),
                            registry.clone(),
                            max_concurrent,
                        ).await {
                            error!("Cron tick failed: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("CronSchedulerService received shutdown signal");
                        break;
                    }
                }
            }

            info!("CronSchedulerService stopped");
            Ok(())
        })
    }

    /// Send shutdown signal to stop the polling loop.
    pub fn shutdown(&self) {
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }
    }
}

/// Single polling cycle: find due jobs and execute them with concurrency limit.
async fn tick_impl(
    job_repo: Arc<dyn CronJobRepository>,
    run_repo: Arc<dyn CronRunRepository>,
    registry: Arc<ExecutorRegistry>,
    max_concurrent: usize,
) -> Result<()> {
    let now = Utc::now();
    let jobs = job_repo.find_due_jobs(None).await?;

    let due_jobs: Vec<CronJob> = jobs
        .into_iter()
        .filter(|job| {
            job.is_enabled
                && !job.is_running
                && job
                    .next_run_at
                    .as_ref()
                    .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
                    .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc) <= now)
                    .unwrap_or(false)
        })
        .collect();

    if due_jobs.is_empty() {
        return Ok(());
    }

    info!("Found {} due cron jobs", due_jobs.len());

    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for job in due_jobs {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| crate::shared::error::Error::Internal(e.to_string()))?;
        let job_repo = job_repo.clone();
        let run_repo = run_repo.clone();
        let registry = registry.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = execute_job(job, job_repo, run_repo, registry).await {
                error!("Job execution error: {}", e);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        if let Err(e) = handle.await {
            error!("Job task join error: {}", e);
        }
    }

    Ok(())
}

/// Execute a single cron job: set running, create run record, execute, update stats.
async fn execute_job(
    job: CronJob,
    job_repo: Arc<dyn CronJobRepository>,
    run_repo: Arc<dyn CronRunRepository>,
    registry: Arc<ExecutorRegistry>,
) -> Result<()> {
    // Set is_running = true
    if let Err(e) = job_repo.set_running(&job.id, &job.workspace_id, true).await {
        warn!("Failed to set job {} running: {}", job.id, e);
    }

    // Create run record
    let run = match run_repo
        .create(&job.id, &job.workspace_id, "schedule", None)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create run record for job {}: {}", job.id, e);
            let _ = job_repo.set_running(&job.id, &job.workspace_id, false).await;
            return Err(e);
        }
    };

    let run_id = run.id.clone();
    let timeout_secs = job.timeout_seconds.max(1) as u64;

    // Look up executor
    let executor = registry.find(&job.job_type);

    let result: std::result::Result<ExecutionResult, ExecutorError> = if let Some(exec) = executor {
        // Wrap execution in timeout and catch_unwind
        let fut = std::panic::AssertUnwindSafe(exec.execute(&job, &run_id));
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            fut.catch_unwind(),
        )
        .await;

        match timeout_result {
            Ok(Ok(Ok(res))) => Ok(res),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_panic)) => Err(ExecutorError::CommandFailed(
                "job panicked during execution".to_string(),
            )),
            Err(_) => Err(ExecutorError::Timeout(timeout_secs)),
        }
    } else {
        Err(ExecutorError::InvalidConfig(format!(
            "no executor for job type {}",
            job.job_type
        )))
    };

    // Update run record and job stats
    match result {
        Ok(res) => {
            if let Err(e) = run_repo
                .complete(
                    &run_id,
                    &job.workspace_id,
                    &res.status,
                    res.output.as_deref(),
                    res.error_message.as_deref(),
                    res.duration_ms,
                )
                .await
            {
                error!("Failed to complete run {}: {}", run_id, e);
            }

            if let Err(e) = job_repo
                .update_run_stats(
                    &job.id,
                    &job.workspace_id,
                    &res.status,
                    res.error_message.as_deref(),
                )
                .await
            {
                error!("Failed to update run stats for job {}: {}", job.id, e);
            }

            info!(
                "Job {} executed successfully ({}ms)",
                job.id, res.duration_ms
            );
        }
        Err(err) => {
            let err_msg = err.to_string();
            if let Err(e) = run_repo
                .complete(&run_id, &job.workspace_id, "failed", None, Some(&err_msg), 0)
                .await
            {
                error!("Failed to complete run {}: {}", run_id, e);
            }

            if let Err(e) = job_repo
                .update_run_stats(&job.id, &job.workspace_id, "failed", Some(&err_msg))
                .await
            {
                error!("Failed to update run stats for job {}: {}", job.id, e);
            }

            warn!("Job {} failed: {}", job.id, err_msg);
        }
    }

    // Compute and update next_run_at
    let next_run = compute_next_run_at(&job.cron_expression);
    if let Some(ref next) = next_run {
        if let Err(e) = job_repo
            .update_next_run_at(&job.id, &job.workspace_id, Some(next))
            .await
        {
            warn!("Failed to update next_run_at for job {}: {}", job.id, e);
        }
        info!("Job {} next run at: {}", job.id, next);
    }

    // Set is_running = false
    if let Err(e) = job_repo.set_running(&job.id, &job.workspace_id, false).await {
        warn!("Failed to set job {} not running: {}", job.id, e);
    }

    Ok(())
}

/// Compute the next run time from a cron expression.
fn compute_next_run_at(cron_expression: &str) -> Option<String> {
    let schedule = Schedule::from_str(cron_expression).ok()?;
    let next = schedule.upcoming(Utc).next()?;
    Some(next.format("%Y-%m-%d %H:%M:%S").to_string())
}

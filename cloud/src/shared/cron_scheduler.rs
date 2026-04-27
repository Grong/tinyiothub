use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use tokio::sync::{broadcast, Semaphore};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use tinyiothub_runtime::cron::{ExecutionResult, ExecutorError, ExecutorRegistry};
use tinyiothub_storage::traits::cron::{CronJobRepository, CronRunRepository};
use tinyiothub_core::models::cron_job::CronJob;
use crate::shared::error::Result;

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
            if let Err(e) = job_repo.clear_all_running().await {
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
    let jobs = job_repo.find_due_jobs().await?;

    if jobs.is_empty() {
        return Ok(());
    }

    info!("Found {} potentially due cron jobs", jobs.len());

    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for job in jobs {
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

/// Execute a single cron job: atomically claim, create run record, execute, update stats.
async fn execute_job(
    job: CronJob,
    job_repo: Arc<dyn CronJobRepository>,
    run_repo: Arc<dyn CronRunRepository>,
    registry: Arc<ExecutorRegistry>,
) -> Result<()> {
    // Atomically claim the job (prevents race between scheduler and manual trigger)
    let claimed = match job_repo.claim_job(&job.id).await {
        Ok(true) => true,
        Ok(false) => {
            info!("Job {} already claimed by another process, skipping", job.id);
            return Ok(());
        }
        Err(e) => {
            warn!("Failed to claim job {}: {}", job.id, e);
            return Err(e);
        }
    };

    if !claimed {
        return Ok(());
    }

    // Create run record
    let run = match run_repo
        .create(&job.id, "schedule", None)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create run record for job {}: {}", job.id, e);
            let _ = job_repo.set_running(&job.id, false).await;
            return Err(e);
        }
    };

    let run_id = run.id.clone();
    let timeout_secs = job.timeout_seconds.max(1) as u64;

    // Look up executor
    let executor = registry.find(&job.job_type);

    let start = std::time::Instant::now();
    let result: std::result::Result<ExecutionResult, ExecutorError> = if let Some(exec) = executor {
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            exec.execute(&job, &run_id),
        )
        .await;

        match timeout_result {
            Ok(Ok(res)) => Ok(res),
            Ok(Err(e)) => Err(e),
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
            let duration_ms = start.elapsed().as_millis() as i64;
            let err_msg = err.to_string();
            let status = match err {
                ExecutorError::Timeout(_) => "timeout",
                _ => "failed",
            };
            if let Err(e) = run_repo
                .complete(&run_id, status, None, Some(&err_msg), duration_ms)
                .await
            {
                error!("Failed to complete run {}: {}", run_id, e);
            }

            if let Err(e) = job_repo
                .update_run_stats(&job.id, status, Some(&err_msg))
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
            .update_next_run_at(&job.id, Some(next))
            .await
        {
            warn!("Failed to update next_run_at for job {}: {}", job.id, e);
        }
        info!("Job {} next run at: {}", job.id, next);
    }

    // Set is_running = false
    if let Err(e) = job_repo.set_running(&job.id, false).await {
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

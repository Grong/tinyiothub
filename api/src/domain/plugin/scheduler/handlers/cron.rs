//! Cron 调度处理器

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Scheduler, Job};
use tracing::{debug, info, error};

use super::SchedulerHandler;
use crate::domain::plugin::scheduler::ScheduledTask;
use crate::shared::error::Error;

use super::super::config::SchedulerConfig;

pub struct CronHandler {
    config: SchedulerConfig,
    scheduler: Arc<RwLock<Option<Scheduler>>>,
}

impl CronHandler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            scheduler: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        let scheduler = Scheduler::new()
            .await
            .map_err(|e| Error::Internal(format!("Failed to create scheduler: {}", e)))?;

        let cron_expr = self.config.cron.clone();
        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let cron_expr = cron_expr.clone();
            Box::pin(async move {
                info!("Cron job triggered: {}", cron_expr);
            })
        }).map_err(|e| Error::Internal(format!("Failed to create cron job: {}", e)))?;

        scheduler.add(job)
            .await
            .map_err(|e| Error::Internal(format!("Failed to add job: {}", e)))?;

        scheduler.start()
            .await
            .map_err(|e| Error::Internal(format!("Failed to start scheduler: {}", e)))?;

        *self.scheduler.write().await = Some(scheduler);
        Ok(())
    }
}

#[async_trait]
impl SchedulerHandler for CronHandler {
    async fn execute(&self, task: &ScheduledTask) -> Result<(), Error> {
        debug!("Executing scheduled task: {}", task.name);
        Ok(())
    }

    fn name(&self) -> &str {
        "CronHandler"
    }
}

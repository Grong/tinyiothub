//! Cron 调度处理器

use std::any::Any;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use super::SchedulerHandler;
use crate::modules::plugin::scheduler::ScheduledTask;
use crate::shared::error::Error;

use super::super::config::SchedulerConfig;
use crate::modules::plugin::{PluginHandler, PluginManifest, PluginType};
use tokio_cron_scheduler::{Job, JobScheduler};

pub struct CronHandler {
    config: SchedulerConfig,
    scheduler: Arc<RwLock<Option<JobScheduler>>>,
    tasks: Arc<RwLock<Vec<ScheduledTask>>>,
    manifest: PluginManifest,
}

impl CronHandler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            scheduler: Arc::new(RwLock::new(None)),
            tasks: Arc::new(RwLock::new(Vec::new())),
            manifest: PluginManifest {
                name: "cron".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Scheduler,
                description: Some("Cron scheduler handler".to_string()),
            },
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        if !self.config.enabled {
            info!("Cron scheduler is disabled");
            return Ok(());
        }

        let sched = JobScheduler::new()
            .await
            .map_err(|e| Error::Internal(format!("Failed to create job scheduler: {}", e)))?;

        let tasks = self.tasks.clone();
        let cron_expr = self.config.cron.clone();

        let job = Job::new_async(&cron_expr, move |_uuid, _lock| {
            let tasks = tasks.clone();
            Box::pin(async move {
                let guard = tasks.read().await;
                for task in guard.iter() {
                    debug!("Cron fired for scheduled task: {}", task.name);
                }
            })
        })
        .map_err(|e| Error::Internal(format!("Invalid cron expression '{}': {}", cron_expr, e)))?;

        sched.add(job).await.map_err(|e| {
            Error::Internal(format!("Failed to add cron job: {}", e))
        })?;

        let sched_clone = sched.clone();
        tokio::spawn(async move {
            if let Err(e) = sched_clone.start().await {
                error!("Cron scheduler error: {}", e);
            }
        });

        *self.scheduler.write().await = Some(sched);

        info!("Cron scheduler started with expression: {}", self.config.cron);
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut guard = self.scheduler.write().await;
        if let Some(ref mut sched) = *guard {
            sched.shutdown().await.map_err(|e| {
                Error::Internal(format!("Failed to shutdown cron scheduler: {}", e))
            })?;
            info!("Cron scheduler shut down");
        }
        *guard = None;
        Ok(())
    }

    pub async fn add_task(&self, task: ScheduledTask) {
        self.tasks.write().await.push(task);
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

impl PluginHandler for CronHandler {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn plugin_type(&self) -> PluginType {
        self.manifest.plugin_type
    }
}

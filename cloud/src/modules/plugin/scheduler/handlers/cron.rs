//! Cron 调度处理器

use std::any::Any;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::SchedulerHandler;
use crate::modules::plugin::scheduler::ScheduledTask;
use crate::shared::error::Error;

use super::super::config::SchedulerConfig;
use crate::modules::plugin::{PluginHandler, PluginManifest, PluginType};

pub struct CronHandler {
    config: SchedulerConfig,
    // TODO: Integrate with tokio-cron-scheduler for actual scheduling
    _scheduler: Arc<RwLock<()>>,
    manifest: PluginManifest,
}

impl CronHandler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            _scheduler: Arc::new(RwLock::new(())),
            manifest: PluginManifest {
                name: "cron".to_string(),
                version: Some("1.0.0".to_string()),
                plugin_type: PluginType::Scheduler,
                description: Some("Cron scheduler handler".to_string()),
            },
        }
    }

    pub async fn start(&self) -> Result<(), Error> {
        info!("Cron scheduler started with expression: {}", self.config.cron);
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

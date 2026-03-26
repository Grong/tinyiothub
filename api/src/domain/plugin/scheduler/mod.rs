//! 定时任务插件
//!
//! 支持 Cron 表达式调度的任务。

pub mod handlers;
pub mod config;

pub use config::SchedulerConfig;
pub use handlers::{SchedulerHandler, CronHandler};

use crate::domain::plugin::PluginHandler;
use crate::shared::error::Error;

pub struct ScheduledTask {
    pub name: String,
    pub payload: serde_json::Value,
}

pub fn create_handler(config: &toml::Value) -> Result<Box<dyn PluginHandler>, Error> {
    let scheduler_cfg = config.get("scheduler")
        .ok_or_else(|| Error::ValidationError("Missing [scheduler] section".to_string()))?;

    let cfg: SchedulerConfig = scheduler_cfg.try_into()?;
    Ok(Box::new(CronHandler::new(cfg)))
}

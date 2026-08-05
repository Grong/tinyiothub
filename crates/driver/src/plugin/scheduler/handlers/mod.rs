//! 调度处理器

use async_trait::async_trait;

use crate::plugin::scheduler::ScheduledTask;
use tinyiothub_core::error::Error;

#[async_trait]
pub trait SchedulerHandler: Send + Sync {
    async fn execute(&self, task: &ScheduledTask) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod cron;

pub use cron::CronHandler;

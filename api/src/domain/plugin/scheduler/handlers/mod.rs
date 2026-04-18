//! 调度处理器

use async_trait::async_trait;

use crate::domain::plugin::scheduler::ScheduledTask;
use crate::shared::error::Error;

#[async_trait]
pub trait SchedulerHandler: Send + Sync {
    async fn execute(&self, task: &ScheduledTask) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod cron;

pub use cron::CronHandler;

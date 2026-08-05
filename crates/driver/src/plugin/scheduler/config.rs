//! 定时任务配置结构体

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    #[serde(rename = "type")]
    pub scheduler_type: String,
    pub cron: String,
    pub enabled: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { scheduler_type: "cron".to_string(), cron: "0 * * * * *".to_string(), enabled: true }
    }
}

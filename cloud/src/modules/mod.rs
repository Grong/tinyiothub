// Feature modules — Handler → Service → Repo 三层架构
// 逐步迁移，每个模块就绪后取消注释

pub mod batch;
pub mod cron;
pub mod device;
pub mod jobs;
pub mod marketplace;
pub mod mcp;
pub mod monitoring;
pub mod open;
pub mod system;

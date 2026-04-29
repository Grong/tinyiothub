// Feature modules — Handler → Service → Repo 三层架构
// 逐步迁移，每个模块就绪后取消注释

pub mod tag;
pub mod role;
pub mod permission;
pub mod user;
pub mod tenant;
pub mod workspace;
pub mod device;
pub mod alarm;
pub mod template;
pub mod cron;
pub mod event;
pub mod notification;
pub mod self_healing;
pub mod mcp;
pub mod agent;
pub mod marketplace;
pub mod plugin;
pub mod auth;
pub mod batch;
pub mod chat;
pub mod drivers;
pub mod heartbeat;
pub mod jobs;
pub mod monitoring;
pub mod open;
pub mod system;

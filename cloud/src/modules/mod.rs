// Feature modules — Handler → Service → Repo 三层架构
// 逐步迁移，每个模块就绪后取消注释

pub mod agent;
pub mod alarm;
pub mod auth;
pub mod batch;
pub mod chat;
pub mod cron;
pub mod device;
pub mod driver_health;
pub mod drivers;
pub mod event;
pub mod gateway;
pub mod heartbeat;
pub mod jobs;
pub mod marketplace;
pub mod mcp;
pub mod monitoring;
pub mod notification;
pub mod open;
pub mod permission;
pub mod plugin;
pub mod role;
pub mod system;
pub mod tag;
pub mod template;
pub mod tenant;
pub mod user;
pub mod workspace;

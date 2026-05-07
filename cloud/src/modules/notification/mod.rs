// Notification module — 告警通知规则 + 通知渠道管理
// 迁移自 domain/event/aggregates/notification_aggregate.rs,
//   domain/event/services/notification_service.rs,
//   domain/event/services/notification_channel.rs,
//   domain/event/specifications/notification_specifications.rs,
//   infrastructure/persistence/repositories/notification_*_repository_impl.rs,
//   api/notifications/management.rs, api/notification_channels/mod.rs

pub mod handler;
pub mod repo;
pub mod service;
pub mod types;

pub use repo::*;
pub use service::*;
pub use types::*;

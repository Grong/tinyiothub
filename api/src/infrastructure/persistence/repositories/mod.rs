// Infrastructure implementations of domain repository interfaces

pub mod alarm_repository_impl;
pub mod device_memory_repository_impl;
pub mod event_repository_impl;
pub mod notification_history_repository_impl;
pub mod notification_rule_repository_impl;
pub mod real_time_event_repository_impl;

// Re-export implementations
pub use alarm_repository_impl::{AlarmRepositoryImpl, AlarmRuleRepositoryImpl};
pub use device_memory_repository_impl::{DeviceMemoryRepository, SqliteDeviceMemoryRepository};
pub use event_repository_impl::SqliteEventRepository;
pub use notification_history_repository_impl::NotificationHistoryRepositoryImpl;
pub use real_time_event_repository_impl::SqliteRealTimeEventRepository;

// Infrastructure implementations of domain repository interfaces

pub mod alarm_repository_impl;
pub mod device_memory_repository_impl;
pub mod device_query_service_impl;
pub mod device_repository_impl;
pub mod event_repository_impl;
pub mod notification_history_repository_impl;
pub mod notification_rule_repository_impl;
pub mod real_time_event_repository_impl;

// Re-export implementations
pub use alarm_repository_impl::{AlarmRepositoryImpl, AlarmRuleRepositoryImpl};
pub use device_memory_repository_impl::SqliteDeviceMemoryRepository;
pub use device_query_service_impl::SqliteDeviceQueryService;
pub use device_repository_impl::SqliteDeviceRepository;
pub use event_repository_impl::SqliteEventRepository;
pub use notification_history_repository_impl::NotificationHistoryRepositoryImpl;
pub use real_time_event_repository_impl::SqliteRealTimeEventRepository;

// Infrastructure implementations of domain repository interfaces

// Migrated to tinyiothub-storage
pub use tinyiothub_storage::sqlite::{
    device::SqliteDeviceRepository,
    cron_job::SqliteCronJobRepository,
    cron_run::SqliteCronRunRepository,
    permission::{SqlitePermissionGroupRepository, SqlitePermissionRepository},
    product::SqliteProductRepository,
    role::SqliteRoleRepository,
    tag::{SqliteTagBindingRepository, SqliteTagRepository},
    tenant::SqliteTenantRepository,
    user::SqliteUserRepository,
    workspace::SqliteWorkspaceRepository,
};

// Still in cloud (not migrated — depend on cloud-internal types)
pub mod alarm_repository_impl;
pub mod device_memory_repository_impl;
pub mod device_query_service_impl;
pub mod event_repository_impl;
pub mod notification_history_repository_impl;
pub mod notification_rule_repository_impl;
pub mod real_time_event_repository_impl;
pub mod session_repository_impl;
pub mod device_row_mapper;

// Re-export cloud-local implementations
pub use alarm_repository_impl::{AlarmRepositoryImpl, AlarmRuleRepositoryImpl};
pub use device_memory_repository_impl::SqliteDeviceMemoryRepository;
pub use device_query_service_impl::SqliteDeviceQueryService;
pub use event_repository_impl::SqliteEventRepository;
pub use notification_history_repository_impl::NotificationHistoryRepositoryImpl;
pub use notification_rule_repository_impl::NotificationRuleRepositoryImpl;
pub use real_time_event_repository_impl::SqliteRealTimeEventRepository;
pub use session_repository_impl::SqliteSessionRepository;

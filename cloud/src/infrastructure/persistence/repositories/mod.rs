// Infrastructure implementations of domain repository interfaces

// SaaS-specific repository implementations (moved from tinyiothub-storage)
pub mod device_query;
pub mod permission_repository_impl;
pub mod product_repository_impl;
pub mod role_repository_impl;
pub mod tag_repository_impl;
pub mod tenant_repository_impl;
pub mod user_repository_impl;
pub mod workspace_repository_impl;

pub use device_query::*;
pub use permission_repository_impl::{SqlitePermissionGroupRepository, SqlitePermissionRepository};
pub use product_repository_impl::SqliteProductRepository;
pub use role_repository_impl::SqliteRoleRepository;
pub use tag_repository_impl::{SqliteTagBindingRepository, SqliteTagRepository};
pub use tenant_repository_impl::SqliteTenantRepository;
pub use user_repository_impl::SqliteUserRepository;
pub use workspace_repository_impl::SqliteWorkspaceRepository;

// Migrated to tinyiothub-storage (IoT models)
pub use tinyiothub_storage::sqlite::{
    device::SqliteDeviceRepository,
    device_command::*,
    device_property::*,
    cron_job::SqliteCronJobRepository,
    cron_run::SqliteCronRunRepository,
    notification_channel::*,
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

// Re-export cloud-local implementations
pub use alarm_repository_impl::{AlarmRepositoryImpl, AlarmRuleRepositoryImpl};
pub use device_memory_repository_impl::SqliteDeviceMemoryRepository;
pub use device_query_service_impl::SqliteDeviceQueryService;
pub use event_repository_impl::SqliteEventRepository;
pub use notification_history_repository_impl::NotificationHistoryRepositoryImpl;
pub use notification_rule_repository_impl::NotificationRuleRepositoryImpl;
pub use real_time_event_repository_impl::SqliteRealTimeEventRepository;
pub use session_repository_impl::SqliteSessionRepository;

// Re-export migrated row mapper
pub use tinyiothub_storage::sqlite::device_row_mapper::*;

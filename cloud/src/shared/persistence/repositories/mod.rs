// Infrastructure implementations of domain repository interfaces

// SaaS-specific repository implementations (moved from tinyiothub-storage)
pub mod device_query;
// permission_repository_impl — 已迁移至 modules/permission/repo.rs
// product_repository_impl — 已迁移至 modules/product/repo.rs
// role_repository_impl — 已迁移至 modules/role/repo.rs
// tag_repository_impl — 已迁移至 modules/tag/repo.rs
// tenant_repository_impl — 已迁移至 modules/tenant/repo.rs
// user_repository_impl — 已迁移至 modules/user/repo.rs
// workspace_repository_impl — 已迁移至 modules/workspace/repo.rs

pub use device_query::*;
// SqlitePermissionRepository / SqlitePermissionGroupRepository — 已迁移至 modules::permission
// SqliteProductRepository — 已迁移至 modules::product
// SqliteRoleRepository — 已迁移至 modules::role
// SqliteTagRepository / SqliteTagBindingRepository — 已迁移至 modules::tag
// SqliteTenantRepository — 已迁移至 modules::tenant
// SqliteUserRepository — 已迁移至 modules::user
// SqliteWorkspaceRepository — 已迁移至 modules::workspace

// Migrated to tinyiothub-storage (IoT models)
pub use tinyiothub_storage::sqlite::{
    cron_job::SqliteCronJobRepository, cron_run::SqliteCronRunRepository,
    device::SqliteDeviceRepository, device_command::*, device_property::*, notification_channel::*,
};

// Still in cloud (not migrated — depend on cloud-internal types)
pub mod alarm_repository_impl;
pub mod device_memory_repository_impl;
pub mod device_query_service_impl;
pub mod device_trace_repository_impl;
pub mod event_repository_impl;
pub mod notification_history_repository_impl;
pub mod notification_rule_repository_impl;
pub mod real_time_event_repository_impl;
pub mod session_repository_impl;

// Re-export cloud-local implementations
pub use alarm_repository_impl::{AlarmRepositoryImpl, AlarmRuleRepositoryImpl};
pub use device_memory_repository_impl::SqliteDeviceMemoryRepository;
pub use device_query_service_impl::SqliteDeviceQueryService;
pub use device_trace_repository_impl::DeviceTraceRepository;
pub use event_repository_impl::SqliteEventRepository;
pub use notification_history_repository_impl::NotificationHistoryRepositoryImpl;
pub use notification_rule_repository_impl::NotificationRuleRepositoryImpl;
pub use real_time_event_repository_impl::SqliteRealTimeEventRepository;
pub use session_repository_impl::SqliteSessionRepository;
// Re-export migrated row mapper
pub use tinyiothub_storage::sqlite::device_row_mapper::*;

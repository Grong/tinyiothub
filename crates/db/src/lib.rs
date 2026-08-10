//! TinyIoTHub storage layer
//!
//! Repository traits, SQLite implementations, caches, and the unified Storage facade.
//!
//! ## 设计不变量
//! - 只依赖 core，禁止依赖其他任何 workspace crate
//! - 具体 struct、按领域平铺（traits/ 残留待逐领域评估削除）
//! - 测试使用真实 SQLite（test_helpers::run_all_migrations）

/// Alarm + alarm rule persistence and row types.
pub mod alarm;
/// Device cache (in-memory).
pub mod cache;
/// Database connection configuration.
pub mod config;
/// Cron job persistence.
pub mod cron_job;
/// Cron run persistence.
pub mod cron_run;
/// Database facade (connection + domain accessors).
pub mod database;
/// Device persistence.
pub mod device;
/// Device command persistence.
pub mod device_command;
/// Device property persistence.
pub mod device_property;
/// Device row mapping helpers.
pub mod device_row_mapper;
/// Driver installation persistence.
pub mod driver_installation;
/// Database error type.
pub mod error;
/// Repository factory.
pub mod factory;
/// Embedded migrations runner.
pub mod migrations;
/// Shared query model types.
pub mod models;
/// Notification channel persistence.
pub mod notification_channel;
/// Notification rule/history persistence + row types.
pub mod notify;
/// Migrating SQLite pool creation (foreign keys on, runs embedded migrations).
pub mod pool;
/// Unified storage facade.
pub mod storage;
/// Tenant-aware cron repository adapters.
pub mod tenant_cron;
/// Tenant-aware device repository adapters.
pub mod tenant_device;
/// Repository traits (legacy inversion — 逐领域评估削除).
pub mod traits;

/// Test helpers for crates that need a fully-migrated in-memory pool.
pub mod test_helpers {
    /// Run all migrations in chronological order.
    ///
    /// Delegates to the centralized migration runner which handles:
    /// - Skipping deleted-file versions
    /// - Cleaning up orphaned `_sqlx_migrations` records
    /// - Post-migration schema consistency repair
    pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
        crate::migrations::run_migrations(pool).await
    }
}

// Re-export commonly used items
pub use cache::DeviceCache;
pub use config::DatabaseConfig;
pub use cron_job::SqliteCronJobRepository;
pub use cron_run::SqliteCronRunRepository;
pub use database::Database;
pub use device::SqliteDeviceRepository;
pub use device_command::*;
pub use device_property::*;
pub use device_row_mapper::*;
pub use driver_installation::{DriverInstallation, DriverInstallationRepo};
pub use error::{DbError, Result};
pub use factory::DeviceRepositoryFactory;
pub use models::{Filter, FilterOp, Pagination, RowMetadata, SortOrder};
pub use notification_channel::*;
pub use pool::{create_pool, create_pool_without_migrations};
pub use storage::Storage;
pub use tenant_cron::{TenantCronJobRepository, TenantCronRunRepository};
pub use tenant_device::TenantDeviceRepository;
pub use traits::*;

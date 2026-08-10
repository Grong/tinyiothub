//! TinyIoTHub storage layer
//!
//! Repository traits, SQLite implementations, caches, and the unified Storage facade.
//!
//! ## 设计不变量
//! - 只依赖 core，禁止依赖其他任何 workspace crate
//! - 具体 struct、按领域平铺（traits/ 残留待逐领域评估削除）
//! - 测试使用真实 SQLite（test_helpers::run_all_migrations）


pub mod cache;
pub mod driver_installation;
pub mod factory;
pub mod migrations;
pub mod models;
pub mod pool;
pub mod sqlite;
pub mod storage;
pub mod tenant_cron;
pub mod tenant_device;
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
pub use driver_installation::{DriverInstallation, DriverInstallationRepo};
pub use factory::DeviceRepositoryFactory;
pub use models::{Filter, FilterOp, Pagination, RowMetadata, SortOrder};
pub use pool::create_pool;
pub use sqlite::{
    Database, DatabaseConfig, create_pool_from_url, create_pool_with_harmonyos, cron_job::SqliteCronJobRepository,
    cron_run::SqliteCronRunRepository, device::SqliteDeviceRepository, device_command::*, device_property::*,
    device_row_mapper::*, notification_channel::*,
};
pub use storage::Storage;
pub use tenant_cron::{TenantCronJobRepository, TenantCronRunRepository};
pub use tenant_device::TenantDeviceRepository;
pub use traits::*;

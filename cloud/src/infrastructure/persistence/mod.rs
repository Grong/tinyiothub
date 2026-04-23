// Persistence layer - database and storage

pub mod adapters;
pub mod config;
pub mod database;
pub mod factory;
pub mod pool;
pub mod repositories;

pub use tinyiothub_storage::sqlite::database::*;

// Re-export commonly used items
pub use config::DatabaseConfig;
pub use database::Database;
pub use pool::create_pool;

#[cfg(test)]
pub mod test_helpers {
    use std::path::Path;

    /// Run all migrations (cloud + storage) in chronological order.
    ///
    /// Migrations from cloud/migrations/ and crates/tinyiothub-storage/migrations/
    /// have bidirectional dependencies and must be interleaved by version.
    pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
        let cloud_migrator = sqlx::migrate::Migrator::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"),
        ).await?;

        let storage_migrator = sqlx::migrate::Migrator::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/tinyiothub-storage/migrations"),
        ).await?;

        let mut combined: Vec<sqlx::migrate::Migration> = Vec::new();
        combined.extend(cloud_migrator.iter().cloned());
        combined.extend(storage_migrator.iter().cloned());

        sqlx::migrate::Migrator::with_migrations(combined).run(pool).await
    }
}

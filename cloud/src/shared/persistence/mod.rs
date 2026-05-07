// Persistence layer - database and storage

pub mod adapters;
pub mod config;
pub mod database;
pub mod factory;
pub mod pool;
pub mod repositories;

// Re-export commonly used items
pub use config::DatabaseConfig;
pub use database::Database;
pub use pool::create_pool;

#[cfg(test)]
pub mod test_helpers {
    use std::path::Path;
    use std::collections::HashMap;

    /// Run all migrations (cloud + storage) in chronological order.
    ///
    /// Migrations from cloud/migrations/ and crates/tinyiothub-storage/migrations/
    /// are interleaved by version, with cloud taking precedence for duplicates.
    /// Test/seed data migrations referencing non-existent devices are skipped.
    pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
        const SKIP_VERSIONS: &[i64] = &[
            20260107000001, // test data: inserts properties/commands for non-existent devices
            20260114000001, // test data: inserts events referencing non-existent devices
            20260418000001, // storage: add tenant_id to tags — already in cloud base schema
        ];

        let cloud_migrator = sqlx::migrate::Migrator::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"),
        ).await?;

        let storage_migrator = sqlx::migrate::Migrator::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/tinyiothub-storage/migrations"),
        ).await?;

        let mut seen: HashMap<i64, bool> = HashMap::new();
        let mut combined: Vec<sqlx::migrate::Migration> = Vec::new();

        // Cloud migrations first (preferred), skip broken 20260414102323
        for m in cloud_migrator.iter().cloned() {
            if !SKIP_VERSIONS.contains(&m.version) && m.version != 20260414102323 {
                seen.insert(m.version, true);
                combined.push(m);
            }
        }

        // Storage migrations for versions not already covered by cloud
        for m in storage_migrator.iter().cloned() {
            if !seen.contains_key(&m.version) && !SKIP_VERSIONS.contains(&m.version) {
                combined.push(m);
            }
        }

        combined.sort_by_key(|m| m.version);
        sqlx::migrate::Migrator::with_migrations(combined).run(pool).await
    }
}

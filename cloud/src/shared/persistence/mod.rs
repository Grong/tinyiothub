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
    /// Run all cloud migrations in chronological order.
    ///
    /// Test/seed data migrations referencing non-existent devices are skipped.
    pub async fn run_all_migrations(
        pool: &sqlx::SqlitePool,
    ) -> Result<(), sqlx::migrate::MigrateError> {
        const SKIP_VERSIONS: &[i64] = &[
            20260107000001, // test data: inserts properties/commands for non-existent devices
            20260114000001, // test data: inserts events referencing non-existent devices
        ];

        let migrator =
            sqlx::migrate::Migrator::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"))
                .await?;

        let mut combined: Vec<sqlx::migrate::Migration> = Vec::new();
        for m in migrator.iter().cloned() {
            if !SKIP_VERSIONS.contains(&m.version) {
                combined.push(m);
            }
        }

        sqlx::migrate::Migrator::with_migrations(combined).run(pool).await
    }
}

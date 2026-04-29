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

    /// Run cloud migrations in version order.
    pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate::Migrator::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"),
        ).await?.run(pool).await
    }
}

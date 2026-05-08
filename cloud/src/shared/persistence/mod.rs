// Persistence layer - database and storage

pub mod adapters;
pub mod config;
pub mod database;
pub mod factory;
pub mod migrations;
pub mod pool;
pub mod repositories;

// Re-export commonly used items
pub use config::DatabaseConfig;
pub use database::Database;
pub use pool::create_pool;

#[cfg(test)]
pub mod test_helpers {
    /// Run all cloud migrations in chronological order.
    ///
    /// Delegates to the centralized migration runner which handles:
    /// - Skipping deleted-file versions
    /// - Cleaning up orphaned `_sqlx_migrations` records
    /// - Post-migration schema consistency repair
    pub async fn run_all_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
        crate::shared::persistence::migrations::run_migrations(pool).await
    }
}

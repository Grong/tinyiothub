use super::error::EdgeResult;
use std::sync::Arc;
use tinyiothub_storage::{Db, DatabaseConfig, create_pool_without_migrations};

/// Initialize the SQLite database and return an Arc<Db>.
pub async fn init_database(db_path: &str) -> EdgeResult<Arc<Db>> {
    let config = DatabaseConfig::from_file_path(db_path);
    let pool = create_pool_without_migrations(&config).await?;
    let db = Db::new(pool);

    // Ensure core tables exist (edge gateway needs devices locally)
    db.ensure_devices_table().await?;

    Ok(Arc::new(db))
}

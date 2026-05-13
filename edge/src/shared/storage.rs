use std::sync::Arc;
use tinyiothub_storage::sqlite::{Database, DatabaseConfig, create_pool};

/// Initialize the SQLite database and return an Arc<Database>.
pub async fn init_database(db_path: &str) -> Result<Arc<Database>, Box<dyn std::error::Error>> {
    let config = DatabaseConfig::from_file_path(db_path);
    let pool = create_pool(&config).await?;
    Ok(Arc::new(Database::new(pool)))
}

use std::sync::Arc;
use tinyiothub_storage::sqlite::{Database, DatabaseConfig, create_pool};
use super::error::EdgeResult;

/// Initialize the SQLite database and return an Arc<Database>.
pub async fn init_database(db_path: &str) -> EdgeResult<Arc<Database>> {
    let config = DatabaseConfig::from_file_path(db_path);
    let pool = create_pool(&config).await?;
    let db = Database::new(pool);

    // Ensure core tables exist (edge gateway needs devices locally)
    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            display_name TEXT,
            device_type TEXT,
            address TEXT,
            description TEXT,
            position TEXT,
            driver_name TEXT,
            device_model TEXT,
            protocol_type TEXT,
            factory_name TEXT,
            linked_data TEXT,
            driver_options TEXT,
            state INTEGER NOT NULL DEFAULT 0,
            parent_id TEXT,
            product_id TEXT,
            workspace_id TEXT,
            linked_gateway TEXT,
            fingerprint TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .await?;

    Ok(Arc::new(db))
}

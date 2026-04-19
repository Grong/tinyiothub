use std::{str::FromStr, time::Duration};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use super::config::DatabaseConfig;

pub async fn create_pool(config: &DatabaseConfig) -> Result<SqlitePool, sqlx::Error> {
    tracing::info!("Creating database connection pool with config: {:?}", config);

    // Parse connection options
    let connect_options = SqliteConnectOptions::from_str(&config.url)?.create_if_missing(true);

    // For HarmonyOS: Use conservative settings to prevent issues
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_env = "ohos") || crate::infrastructure::config::get().harmonyos.enabled {
            tracing::warn!("HarmonyOS detected: Using conservative SQLite settings");

            // Use conservative settings for HarmonyOS
            let harmonyos_options = connect_options
                .pragma("journal_mode", "DELETE") // Use DELETE instead of WAL
                .pragma("synchronous", "FULL") // Use FULL for safety
                .pragma("cache_size", "-8000") // Smaller cache
                .pragma("temp_store", "MEMORY")
                .shared_cache(false); // Disable shared cache

            let pool = SqlitePoolOptions::new()
                .max_connections(config.max_connections.min(5)) // Limit connections
                .min_connections(1)
                .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
                .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
                .connect_with(harmonyos_options)
                .await?;

            // Run migrations
            tracing::info!("Running database migrations...");
            sqlx::migrate!("./migrations").run(&pool).await?;
            tracing::info!("Database migrations completed successfully");

            return Ok(pool);
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .connect_with(connect_options)
        .await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Database migrations completed successfully");

    Ok(pool)
}

pub async fn create_pool_from_url(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let config = DatabaseConfig { url: database_url.to_string(), ..Default::default() };

    create_pool(&config).await
}

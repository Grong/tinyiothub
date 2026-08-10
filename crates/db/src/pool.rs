//! Migrating SQLite pool creation (foreign keys on, runs embedded migrations).

use std::{str::FromStr, time::Duration};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::config::DatabaseConfig;

pub async fn create_pool(config: &DatabaseConfig, is_harmonyos: bool) -> Result<SqlitePool, sqlx::Error> {
    tracing::info!("Creating database connection pool with config: {:?}", config);

    // Parse connection options
    let connect_options = SqliteConnectOptions::from_str(&config.url)?
        .create_if_missing(true)
        .foreign_keys(true);

    // For HarmonyOS: Use conservative settings to prevent issues
    #[cfg(target_os = "linux")]
    {
        if is_harmonyos {
            tracing::warn!("HarmonyOS detected: Using conservative SQLite settings");

            // Use conservative settings for HarmonyOS
            let harmonyos_options = connect_options
                .pragma("journal_mode", "DELETE") // Use DELETE instead of WAL
                .pragma("synchronous", "FULL") // Use FULL for safety
                .pragma("cache_size", "-8000") // Smaller cache
                .pragma("temp_store", "MEMORY")
                .pragma("foreign_keys", "ON")
                .shared_cache(false); // Disable shared cache

            let pool = SqlitePoolOptions::new()
                .max_connections(config.max_connections.min(5)) // Limit connections
                .min_connections(1)
                .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
                .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
                .connect_with(harmonyos_options)
                .await?;

            // Run migrations via centralized module
            tracing::info!("Running database migrations...");
            crate::migrations::run_migrations(&pool).await?;
            tracing::info!("Database migrations completed successfully");

            return Ok(pool);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = is_harmonyos;

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .connect_with(connect_options)
        .await?;

    // Run migrations via centralized module
    tracing::info!("Running database migrations...");
    crate::migrations::run_migrations(&pool).await?;
    tracing::info!("Database migrations completed successfully");

    Ok(pool)
}

/// Non-migrating pool creation — edge 专用：不跑内嵌迁移、不开 FK pragma
/// （edge 的 schema 由 apps/edge 自行 CREATE TABLE 管理）。
/// 保留原 `sqlite::pool::create_pool` 的行为，勿用于 cloud。
pub async fn create_pool_without_migrations(config: &DatabaseConfig) -> Result<SqlitePool, sqlx::Error> {
    tracing::info!("Creating database connection pool with config: {:?}", config);

    let connect_options = SqliteConnectOptions::from_str(&config.url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .connect_with(connect_options)
        .await?;

    Ok(pool)
}

use std::{collections::HashMap, str::FromStr, time::Duration};

use sqlx::{
    migrate::{Migration, Migrator},
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
};

use super::config::DatabaseConfig;

/// Versions to skip in production (test/seed data referencing non-existent devices).
const SKIP_MIGRATIONS: &[i64] = &[
    20260107000001, // inserts properties/commands for devices that don't exist in prod
    20260114000001, // inserts test events referencing non-existent devices
    20260418000001, // storage: add tenant_id to tags — already in cloud base schema
];

/// Load migrations from cloud/ and storage crate, interleaved by version.
///
/// Cloud migrations take precedence when a version exists in both directories,
/// except for 20260414102323 where the storage version is the canonical one.
async fn load_all_migrations() -> Result<Vec<Migration>, sqlx::migrate::MigrateError> {
    let cloud_migrations =
        Migrator::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations")).await?;

    let storage_migrations = Migrator::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/tinyiothub-storage/migrations"),
    )
    .await?;

    let mut seen: HashMap<i64, bool> = HashMap::new();
    let mut combined: Vec<Migration> = Vec::new();

    // Cloud migrations first (preferred), skip broken cloud version
    for m in cloud_migrations.iter().cloned() {
        if m.version != 20260414102323 && !SKIP_MIGRATIONS.contains(&m.version) {
            seen.insert(m.version, true);
            combined.push(m);
        }
    }

    // Storage migrations for versions not already covered by cloud
    for m in storage_migrations.iter().cloned() {
        if !seen.contains_key(&m.version) && !SKIP_MIGRATIONS.contains(&m.version) {
            combined.push(m);
        }
    }

    // Sort by version to apply chronologically
    combined.sort_by_key(|m| m.version);

    Ok(combined)
}

pub async fn create_pool(config: &DatabaseConfig) -> Result<SqlitePool, sqlx::Error> {
    tracing::info!("Creating database connection pool with config: {:?}", config);

    // Parse connection options
    let connect_options = SqliteConnectOptions::from_str(&config.url)?.create_if_missing(true);

    // For HarmonyOS: Use conservative settings to prevent issues
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_env = "ohos") || crate::shared::config::get().harmonyos.enabled {
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

            // Run migrations (cloud + storage, interleaved by version)
            tracing::info!("Running database migrations...");
            let migrations = load_all_migrations().await.map_err(|e| {
                sqlx::Error::Configuration(format!("Failed to load migrations: {}", e).into())
            })?;
            Migrator::with_migrations(migrations).run(&pool).await?;
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

    // Run migrations (cloud + storage, interleaved by version)
    tracing::info!("Running database migrations...");
    let migrations = load_all_migrations().await.map_err(|e| {
        sqlx::Error::Configuration(format!("Failed to load migrations: {}", e).into())
    })?;
    Migrator::with_migrations(migrations).run(&pool).await?;
    tracing::info!("Database migrations completed successfully");

    Ok(pool)
}

pub async fn create_pool_from_url(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let config = DatabaseConfig { url: database_url.to_string(), ..Default::default() };

    create_pool(&config).await
}

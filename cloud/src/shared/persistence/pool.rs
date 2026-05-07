use std::{str::FromStr, time::Duration};

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

/// Load cloud migrations, filtering out test/seed data versions.
///
/// Migrations are embedded at compile time via `sqlx::migrate!()`, so no
/// runtime file-system access is required. This fixes the Docker issue where
/// `env!("CARGO_MANIFEST_DIR")` points to the build-time path (`/build/cloud`)
/// which does not exist in the runtime image.
fn load_all_migrations() -> Result<Vec<Migration>, sqlx::migrate::MigrateError> {
    // Use `./migrations` so the macro resolves it relative to CARGO_MANIFEST_DIR
    // rather than the source file directory. The macro embeds all .sql files at
    // compile time, so no runtime filesystem access is required.
    let migrator = sqlx::migrate!("./migrations");

    let mut combined: Vec<Migration> = Vec::new();
    for m in migrator.iter().cloned() {
        if !SKIP_MIGRATIONS.contains(&m.version) {
            combined.push(m);
        }
    }

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
            let migrations = load_all_migrations().map_err(|e| {
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
    let migrations = load_all_migrations().map_err(|e| {
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

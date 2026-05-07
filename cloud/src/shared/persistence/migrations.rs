use sqlx::{
    migrate::{Migration, Migrator},
    sqlite::SqlitePool,
};

/// Migration versions whose SQL files have been deleted.
///
/// These are kept here to prevent `VersionMissing` panics from orphaned
/// records in `_sqlx_migrations`.
const SKIP_MIGRATIONS: &[i64] = &[
    20260107000001, // deleted: test data properties/commands for non-existent devices
    20260114000001, // deleted: test data events referencing non-existent devices
    20260418000001, // deleted: storage tenant_id for tags (already in cloud base schema)
];

/// Load migrations embedded at compile time, filtering out deleted versions.
///
/// Uses `sqlx::migrate!("./migrations")` which embeds all `.sql` files
/// relative to `CARGO_MANIFEST_DIR`. No runtime filesystem access is
/// required, fixing the Docker "migrations not found" error.
pub fn load_migrations() -> Result<Vec<Migration>, sqlx::migrate::MigrateError> {
    let migrator = sqlx::migrate!("./migrations");

    let mut combined: Vec<Migration> = Vec::new();
    for m in migrator.iter().cloned() {
        if !SKIP_MIGRATIONS.contains(&m.version) {
            combined.push(m);
        }
    }

    Ok(combined)
}

/// Run migrations with full safety checks.
///
/// 1. Clean up orphaned records in `_sqlx_migrations` for deleted versions.
/// 2. Run the migration set.
/// 3. Repair schema inconsistencies (add missing columns).
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    cleanup_orphaned_migration_records(pool).await?;

    let migrations = load_migrations().map_err(|e| {
        sqlx::Error::Configuration(format!("Failed to load migrations: {}", e).into())
    })?;
    Migrator::with_migrations(migrations)
        .run(pool)
        .await
        .map_err(|e| sqlx::Error::Configuration(format!("Migration failed: {}", e).into()))?;

    ensure_schema_consistency(pool).await?;

    Ok(())
}

/// Delete `_sqlx_migrations` records for versions whose files no longer exist.
///
/// Without this, SQLx's `Migrator` panics with `VersionMissing` when it sees
/// a record for a version not present in the migration set.
async fn cleanup_orphaned_migration_records(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // On fresh databases the `_sqlx_migrations` table does not exist yet.
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = '_sqlx_migrations'
        )",
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        return Ok(());
    }

    for version in SKIP_MIGRATIONS {
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
            .bind(version)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Ensure tables have all expected columns by adding missing ones.
///
/// This repairs databases where a migration's `CREATE TABLE IF NOT EXISTS`
/// was a no-op on an existing table, or where `ALTER TABLE ADD COLUMN`
/// migrations need to be idempotent.
///
/// Uses `PRAGMA table_info()` to check, then `ALTER TABLE ADD COLUMN`.
async fn ensure_schema_consistency(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    add_column_if_missing(pool, "notification_channels", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "notification_rules", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "notification_history", "workspace_id", "TEXT").await?;

    add_column_if_missing(pool, "chat_sessions", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "chat_sessions", "metadata", "TEXT NOT NULL DEFAULT '{}'").await?;

    add_column_if_missing(pool, "chat_messages", "tool_call_id", "TEXT").await?;
    add_column_if_missing(pool, "chat_messages", "tool_name", "TEXT").await?;

    add_column_if_missing(pool, "roles", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "users", "phone", "TEXT").await?;
    add_column_if_missing(pool, "device_alarms", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "device_alarm_rules", "workspace_id", "TEXT").await?;
    add_column_if_missing(pool, "api_keys", "workspace_id", "TEXT").await?;

    Ok(())
}

/// Add a column to a table if it doesn't already exist.
///
/// Safe to call multiple times (idempotent).
async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<(), sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM pragma_table_info(?)
            WHERE name = ?
        )",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await?;

    if !exists {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, column_def);
        sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
    }

    Ok(())
}

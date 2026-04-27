//! Test database setup helpers.
//!
//! Provides utilities to create isolated in-memory databases
//! for integration tests.

/// Create a fresh in-memory SQLite pool for testing.
///
/// TODO: Wire up to `tinyiothub_storage::sqlite::create_pool` once
/// integration test crate is established.
pub async fn create_test_pool() -> Result<sqlx::SqlitePool, sqlx::Error> {
    sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
}

/// Run migrations against a test database.
///
/// TODO: Point to `tinyiothub_storage::sqlite::run_migrations`.
pub async fn run_test_migrations(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    // TODO: execute migrations from `crates/tinyiothub_storage/migrations/`
    let _ = pool;
    Ok(())
}

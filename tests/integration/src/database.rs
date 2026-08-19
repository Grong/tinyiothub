//! Test database setup helpers.

use sqlx::SqlitePool;

/// Create a fresh in-memory SQLite pool with all migrations applied.
pub async fn create_test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create test database");

    tinyiothub_storage::test_helpers::run_all_migrations(&pool)
        .await
        .expect("Failed to run migrations");
    pool
}

/// Seed required reference data (tenant + workspace) for FK constraints.
pub async fn seed_test_workspace(pool: &SqlitePool, tenant_id: &str, workspace_id: &str) {
    // tenants.plan_id → subscription_plans FK。seed_system（Task 3）会预置 plan_free，但此夹具不跑 seed_system，故保留此行。
    sqlx::query("INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free', 'free', 'Free')")
        .execute(pool)
        .await
        .expect("seed plan");
    sqlx::query("INSERT INTO tenants (id, name, slug, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(tenant_id)
        .bind(tenant_id)
        .bind(tenant_id)
        .bind("2025-01-01T00:00:00Z")
        .bind("2025-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(workspace_id)
        .bind(workspace_id)
        .bind(tenant_id)
        .bind("2025-01-01T00:00:00Z")
        .bind("2025-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
}

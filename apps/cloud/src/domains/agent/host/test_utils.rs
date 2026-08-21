//! Test helpers shared by agent-crate test modules (mirrors the subset of
//! `cloud::test_utils` the moved tests use).

/// Seed a tenant + workspace row pair (INSERT OR IGNORE).
pub async fn seed_test_workspace(pool: &sqlx::SqlitePool, tenant_id: &str, workspace_id: &str) {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // tenants.plan_id → subscription_plans FK。seed_system（Task 3）会预置 plan_free，但此夹具不跑 seed_system，故保留此行。
    sqlx::query(
        "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free', 'free', 'Free')",
    )
    .execute(pool)
    .await
    .expect("Failed to seed subscription plan");

    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name, slug, status, plan_id, subscription_status, timezone, locale, created_at, updated_at) VALUES (?, ?, ?, 'active', 'plan_free', 'active', 'UTC', 'zh-CN', ?, ?)",
    )
    .bind(tenant_id)
    .bind(format!("Test Tenant {}", tenant_id))
    .bind(tenant_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .expect("Failed to seed test tenant");

    sqlx::query(
        "INSERT OR IGNORE INTO workspaces (id, name, description, tenant_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(workspace_id)
    .bind(format!("Test Workspace {}", workspace_id))
    .bind("Test workspace for cross-workspace isolation tests")
    .bind(tenant_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .expect("Failed to seed test workspace");
}

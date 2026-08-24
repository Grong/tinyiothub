//! Two-tier seed module — all seed data that used to live in migration
//! history (removed in the Task 2 baseline squash) now lives here as embedded
//! SQL assets executed through the `Db` facade.
//!
//! Tiers:
//! - [`seed_system`] — production-required rows, applied unconditionally at
//!   bootstrap: subscription plans (tenants.plan_id FK target), template
//!   categories + builtin thing templates, admin user (FIX_ME marker password,
//!   fixed by `ensure_default_admin_user` on first boot), RBAC baseline
//!   (roles/permissions/role_permissions), default tenant + membership +
//!   workspace, default notification rules, event security settings,
//!   event performance thresholds, social provider row, event-retention
//!   cron job.
//! - [`seed_demo`] — demo scenario, gated by `[seed] demo_data` (default
//!   true): 8 demo devices + per-thing properties/actions, tags + bindings,
//!   alarm rules + sample alarms, sample jobs, example notification channels.
//!
//! Both tiers are idempotent (`INSERT OR IGNORE` / `WHERE NOT EXISTS` guards)
//! and safe to re-run on every startup.
//!
//! Provenance (pre-baseline migration → tier):
//! - 20260106000002 rebuild seeds: admin/roles/permissions → system;
//!   devices/tags/bindings/alarm rules/alarms → demo (products rows dropped —
//!   the `products` table no longer exists in the baseline schema);
//!   device_properties/device_commands → superseded by 20260818000001's
//!   thing_properties/thing_actions rows (demo).
//! - 20260108000001 template_categories + 5 builtin templates → system
//!   (templates mapped to thing_templates: commands → actions, per the
//!   20260723000001 rebuild mapping).
//! - 20260111000001 notification_rules → system.
//! - 20260112000001 event_security_settings + event permissions → system.
//! - 20260113000001 event_performance_alerts thresholds → system.
//! - 20260312000001 sample jobs → demo; 20260312000002 example channels → demo.
//! - 20260313000001 subscription_plans → system (FK-required).
//! - 20260314000001 social_configs wechat row → system.
//! - 20260329000001 admin-hash fix → folded into the system admin row
//!   (FIX_ME_admin_hash marker).
//! - 20260407000001 default tenant/membership/workspace → system (the
//!   devices-UPDATE steps are folded into the demo devices INSERT).
//! - 20260516044444 8 builtin templates → system.
//! - 20260725000003 generic per-device property seeding → dropped (the
//!   20260727000001 cleanup intentionally deleted those synthetic rows).
//! - 20260727000003 event-retention cron job → system.
//! - 20260818000001 thing_properties/thing_actions restore → demo (verbatim).
//! - Pure data migrations (INSERT…SELECT rebuilds/backfills: resources,
//!   knowledge_*, api_keys, agent_memories, resource tags, alarm table
//!   rebuilds) carry no seed content — nothing to extract.
//!
//! Implementation note: seeds are static SQL assets (no string interpolation)
//! rather than domain-module calls because the domain repositories generate
//! their own ids (`generate_id()`), while these rows have stable, externally
//! referenced ids (tenant-default-001, ws-default-001, plan_free,
//! device-env-01, …). Revisit in Task 4+ if upsert-capable domain functions
//! appear.

use crate::database::Db;

const SYSTEM_SQL: &str = include_str!("seed/system.sql");
const DEMO_SQL: &str = include_str!("seed/demo.sql");

/// Apply the production-required seed tier. Idempotent.
pub async fn seed_system(db: &Db) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SYSTEM_SQL).execute(db.pool()).await?;
    // CEO review F12：seed 行数进日志——"系统种子是否就位"启动即见，
    // 不靠缺日志推断。
    let (tenants, workspaces, plans, templates): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM tenants), (SELECT COUNT(*) FROM workspaces), \
         (SELECT COUNT(*) FROM subscription_plans), (SELECT COUNT(*) FROM thing_templates)",
    )
    .fetch_one(db.pool())
    .await?;
    tracing::info!(tenants, workspaces, plans, templates, "seed_system applied");
    Ok(())
}

/// Apply the demo-scenario seed tier. Idempotent. Requires [`seed_system`]
/// to have run first (demo rows reference the default tenant/workspace/admin).
pub async fn seed_demo(db: &Db) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(DEMO_SQL).execute(db.pool()).await?;
    let (devices, properties, actions): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM devices), (SELECT COUNT(*) FROM thing_properties), \
         (SELECT COUNT(*) FROM thing_actions)",
    )
    .fetch_one(db.pool())
    .await?;
    tracing::info!(devices, properties, actions, "seed_demo applied");
    Ok(())
}

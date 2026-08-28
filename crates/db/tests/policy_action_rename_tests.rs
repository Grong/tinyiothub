//! 策略动作名数据迁移测试(20260828000001_policy_action_rename)。
//! 老库(baseline 终态 + 中间三迁移 + 含旧动作名的策略行)跑迁移后:
//! allowed_actions/denied_actions JSON 数组内 "wipe_device"→"wipe_thing"、
//! "reboot_device"→"reboot_thing";默认行与前缀相似值("wipe_device_extra")
//! 不受影响。迁移 SQL 用 replace() 精确替换带引号 token,此处钉住该语义。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// 直建 baseline 终态、回放中间三迁移,再插入含旧契约动作名的策略样本。
/// 与 thing_contract_data_tests 相同的隔离模式:pid + 原子序号。
async fn baseline_pool_with_samples() -> (SqlitePool, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("policy-action-{}-{seq}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();

    // 1. 直建 baseline 终态(不经 run_migrations)。
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool)
        .await
        .unwrap();

    // 2. 回放中间三迁移(与 run_migrations 一致先 FK OFF;max_connections=1)。
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20260824000001_workspaces_trust_config_updated_at.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260824000002_agent_actions_tick_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260825000001_rename_device_to_thing.sql"))
        .execute(&pool)
        .await
        .unwrap();

    // 3. 旧契约样本(迁移后 schema、迁移前动作名):
    //    ws_old 含两个旧动作名;ws_default 为默认行;ws_prefix 含前缀相似值。
    sqlx::query(
        "INSERT INTO workspace_autonomy_policy (workspace_id, mode, allowed_actions, denied_actions)
         VALUES ('ws_old', 'act', '[\"reboot_device\",\"set_property\"]', '[\"wipe_device\"]')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_autonomy_policy (workspace_id, mode) VALUES ('ws_default', 'off')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspace_autonomy_policy (workspace_id, mode, allowed_actions, denied_actions)
         VALUES ('ws_prefix', 'diagnose', '[\"wipe_device_extra\"]', '[]')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // 4. policy_rules.target 样本:pr_wipe/pr_reboot 为旧名精确值(应翻转),
    //    pr_glob 为 glob 模式、pr_other 为无关值(均不受影响)。
    for (id, target) in [
        ("pr_wipe", "wipe_device"),
        ("pr_reboot", "reboot_device"),
        ("pr_glob", "wipe_*"),
        ("pr_other", "set_property"),
    ] {
        sqlx::query(
            "INSERT INTO policy_rules (id, workspace_id, category, action, target)
             VALUES (?, 'ws_old', 'agent_action', 'block', ?)",
        )
        .bind(id)
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    }

    // 5. workspaces.heartbeat_trust_config 样本:ws_trust 的 blocked_tools /
    //    allowed_destructive_tools 含旧工具名(应翻转);ws_trust_prefix 含前缀
    //    相似值(不动);ws_trust_default 用默认 '' 空串(不动)。
    sqlx::query("INSERT INTO subscription_plans (id, name, display_name) VALUES ('plan_free', 'free', 'Free')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ('t1', 'tenant1', 'tenant1')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at, heartbeat_trust_config)
         VALUES ('ws_trust', 'ws', 't1', datetime('now'), datetime('now'),
                 '{\"trust_level\":\"FullAuto\",\"max_auto_actions_per_tick\":10,\"allowed_tool_categories\":[\"read\"],\"blocked_tools\":[\"wipe_device\",\"reboot_device\"],\"allowed_destructive_tools\":[\"wipe_device\"]}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at, heartbeat_trust_config)
         VALUES ('ws_trust_prefix', 'ws', 't1', datetime('now'), datetime('now'),
                 '{\"trust_level\":\"ReadOnlyAuto\",\"max_auto_actions_per_tick\":10,\"allowed_tool_categories\":[\"read\"],\"blocked_tools\":[\"wipe_device_extra\"]}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
         VALUES ('ws_trust_default', 'ws', 't1', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    (pool, path)
}

/// 把 baseline + 中间三迁移标记为已应用(checksum 取自嵌入迁移集),
/// 再跑 run_migrations——契约数据迁移与策略动作名迁移依次实际执行。
async fn mark_applied_and_migrate(pool: &SqlitePool) {
    sqlx::raw_sql(
        "CREATE TABLE _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .unwrap();
    let migrator = sqlx::migrate!("./migrations");
    for version in [20260819000001i64, 20260824000001, 20260824000002, 20260825000001] {
        let m = migrator
            .iter()
            .find(|m| m.version == version)
            .unwrap_or_else(|| panic!("migration {version} missing from embedded set"));
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, 1, ?, 0)",
        )
        .bind(m.version)
        .bind(m.description.as_ref())
        .bind(m.checksum.as_ref())
        .execute(pool)
        .await
        .unwrap();
    }

    tinyiothub_storage::migrations::run_migrations(pool).await.unwrap();
}

#[tokio::test]
async fn policy_action_names_migrated() {
    let (pool, path) = baseline_pool_with_samples().await;

    mark_applied_and_migrate(&pool).await;

    let (allowed, denied): (String, String) = sqlx::query_as(
        "SELECT allowed_actions, denied_actions FROM workspace_autonomy_policy WHERE workspace_id='ws_old'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(allowed, "[\"reboot_thing\",\"set_property\"]");
    assert_eq!(denied, "[\"wipe_thing\"]");

    let (allowed, denied): (String, String) = sqlx::query_as(
        "SELECT allowed_actions, denied_actions FROM workspace_autonomy_policy WHERE workspace_id='ws_default'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(allowed, "[\"*\"]", "默认行不得受影响");
    assert_eq!(denied, "[]", "默认行不得受影响");

    let allowed: String = sqlx::query_scalar(
        "SELECT allowed_actions FROM workspace_autonomy_policy WHERE workspace_id='ws_prefix'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(allowed, "[\"wipe_device_extra\"]", "前缀相似值不得被误改写");

    // 迁移后 JSON 仍合法(可被 serde 解析为 Vec<String>)。
    let parsed: Vec<String> = serde_json::from_str(
        &sqlx::query_scalar::<_, String>(
            "SELECT allowed_actions FROM workspace_autonomy_policy WHERE workspace_id='ws_old'",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(parsed, vec!["reboot_thing", "set_property"]);

    // policy_rules.target:旧名精确值翻转,glob 与无关值不动。
    let target_of = |id: &str| {
        let pool = pool.clone();
        let id = id.to_string();
        async move {
            sqlx::query_scalar::<_, String>("SELECT target FROM policy_rules WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(target_of("pr_wipe").await, "wipe_thing");
    assert_eq!(target_of("pr_reboot").await, "reboot_thing");
    assert_eq!(target_of("pr_glob").await, "wipe_*", "glob 模式不得受影响");
    assert_eq!(target_of("pr_other").await, "set_property");

    // workspaces.heartbeat_trust_config:旧工具名翻转(含 allowed_destructive_tools),
    // 前缀相似值与默认 '' 空串不动;迁移后 JSON 仍可解析为 TrustConfig。
    let cfg_of = |id: &str| {
        let pool = pool.clone();
        let id = id.to_string();
        async move {
            sqlx::query_scalar::<_, String>("SELECT heartbeat_trust_config FROM workspaces WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let cfg = cfg_of("ws_trust").await;
    let parsed: tinyiothub_core::heartbeat::TrustConfig = serde_json::from_str(&cfg).unwrap();
    assert_eq!(parsed.blocked_tools, vec!["wipe_thing", "reboot_thing"]);
    assert_eq!(parsed.allowed_destructive_tools, vec!["wipe_thing"]);

    let cfg_prefix = cfg_of("ws_trust_prefix").await;
    let parsed: tinyiothub_core::heartbeat::TrustConfig = serde_json::from_str(&cfg_prefix).unwrap();
    assert_eq!(parsed.blocked_tools, vec!["wipe_device_extra"], "前缀相似值不得被误改写");

    assert_eq!(cfg_of("ws_trust_default").await, "", "默认空串不得受影响");

    drop(pool);
    let _ = std::fs::remove_file(path);
}

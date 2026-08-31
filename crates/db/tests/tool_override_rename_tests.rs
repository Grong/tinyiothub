//! agent 工具名数据迁移测试(20260831000001_tool_override_rename)。
//! 老库(baseline 终态 + 中间五迁移 + 含旧工具名的 agent_configs.config 行)
//! 跑迁移后:tool_denylist JSON 数组内 "search_devices"→"search_things"、
//! "get_device"→"get_thing"、"create_device"→"create_thing"、
//! "delete_device"→"delete_thing";无旧名行与前缀相似值("delete_device_extra")
//! 不受影响。迁移 SQL 用 replace() 精确替换带引号 token,此处钉住该语义。
//!
//! 背景:/tools/toggle 把 catalog 工具 id 写入 agent_configs.config 的
//! tool_denylist(config/service.rs toggle_tool);/tools/effective 以 denylist
//! 精确匹配 MCP 注册名(effective_tool_names → filter_by_denylist)。
//! 旧版默认 denylist 含 "delete_device"(81d073fc 前),静态兜底 catalog 残留
//! PR-1 旧名,存量 config 中的旧名会静默失配。
//! (agent_tools.tool_overrides 为死列——仅级联删除、无读写,不在迁移范围。)

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// 直建 baseline 终态、回放中间五迁移,再插入含旧工具名的 agent config 样本。
/// 与 policy_action_rename_tests 相同的隔离模式:pid + 原子序号。
async fn baseline_pool_with_samples() -> (SqlitePool, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("tool-override-{}-{seq}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();

    // 1. 直建 baseline 终态(不经 run_migrations)。
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool)
        .await
        .unwrap();

    // 2. 回放中间五迁移(与 run_migrations 一致先 FK OFF;max_connections=1)。
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    for sql in [
        include_str!("../migrations/20260824000001_workspaces_trust_config_updated_at.sql"),
        include_str!("../migrations/20260824000002_agent_actions_tick_id.sql"),
        include_str!("../migrations/20260825000001_rename_device_to_thing.sql"),
        include_str!("../migrations/20260826000001_thing_contract_data.sql"),
        include_str!("../migrations/20260828000001_policy_action_rename.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }

    // 3. 旧契约样本(迁移后 schema、迁移前工具名):
    //    agent_old 含旧默认 "delete_device" + 用户 toggle 的三个旧名;
    //    agent_prefix 含前缀相似值(不动);agent_clean 已是新名(不动)。
    for (agent_id, config) in [
        (
            "agent_old",
            r#"{"model":"m","tool_denylist":["delete_device","delete_schedule","search_devices","get_device","create_device"]}"#,
        ),
        (
            "agent_prefix",
            r#"{"model":"m","tool_denylist":["delete_device_extra"]}"#,
        ),
        ("agent_clean", r#"{"model":"m","tool_denylist":["delete_thing"]}"#),
    ] {
        sqlx::query("INSERT INTO agents (agent_id, workspace_id, name) VALUES (?, 'ws1', ?)")
            .bind(agent_id)
            .bind(agent_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO agent_configs (agent_id, config, config_hash) VALUES (?, ?, 'h')")
            .bind(agent_id)
            .bind(config)
            .execute(&pool)
            .await
            .unwrap();
    }

    (pool, path)
}

/// 把 baseline + 中间五迁移标记为已应用(checksum 取自嵌入迁移集),
/// 再跑 run_migrations——只有工具名迁移实际执行。
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
    for version in [
        20260819000001i64,
        20260824000001,
        20260824000002,
        20260825000001,
        20260826000001,
        20260828000001,
    ] {
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
async fn tool_override_names_migrated() {
    let (pool, path) = baseline_pool_with_samples().await;

    mark_applied_and_migrate(&pool).await;

    let config_of = |id: &str| {
        let pool = pool.clone();
        let id = id.to_string();
        async move {
            sqlx::query_scalar::<_, String>("SELECT config FROM agent_configs WHERE agent_id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    // 旧名全部翻转;未涉及 token(如 delete_schedule)保持原样。
    let config = config_of("agent_old").await;
    let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
    let denylist: Vec<String> = serde_json::from_value(parsed["tool_denylist"].clone()).unwrap();
    assert_eq!(
        denylist,
        vec![
            "delete_thing",
            "delete_schedule",
            "search_things",
            "get_thing",
            "create_thing"
        ]
    );
    assert!(!config.contains("\"delete_device\""));
    assert!(!config.contains("\"search_devices\""));

    // 前缀相似值不得被误改写。
    let config = config_of("agent_prefix").await;
    assert!(config.contains("\"delete_device_extra\""), "前缀相似值不得被误改写");

    // 已是新名的行不动。
    let config = config_of("agent_clean").await;
    assert!(config.contains("\"delete_thing\""));

    let _ = std::fs::remove_file(&path);
}

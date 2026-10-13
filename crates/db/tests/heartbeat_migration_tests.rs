//! 心跳历史迁移测试（20261013000001_heartbeat_runs_backfill，AI 子系统梳理
//! Phase 2 Task 9）。
//!
//! 老库（baseline + 全部既有递增迁移 + 手工插入的心跳历史行）跑迁移后：
//! 1. 每个 anchor 行（action_type IN ('summary','error') 且
//!    agent_id LIKE '__heartbeat__:%'）合成 agent_runs 行
//!    （id='hbm_'||tick_id，report 含 trace 与 action_count）；
//! 2. 同 tick 的 proposal/auto_executed 行 tick_id 改写为 'hbm_'||tick_id；
//! 3. heartbeat_tick_map 视图可查 tick_id→run_id；
//! 4. 迁移幂等（重跑零副作用）。
//!
//! 顺序=埋伏③：先合成 run 行再改锚——反过来提案行的 EXISTS 锚检查找不到
//! 目标 run，改写全军覆没。本测试的"合成+改锚同时成立"断言钉住该顺序。

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tinyiothub_core::agent_runs::{Outcome, RunReport};

const NEW_VERSION: i64 = 20261013000001;

/// 直建 baseline + 回放全部先于本迁移的递增迁移并标记已应用（checksum 取
/// 自嵌入迁移集），返回尚未执行本迁移的 pool。与 policy_action_rename_tests
/// 相同的隔离模式：临时文件库 + max_connections(1) + FK OFF 回放。
async fn pre_migration_pool(tag: &str) -> (SqlitePool, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("hbm-{tag}-{}-{seq}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.unwrap();

    let migrator = sqlx::migrate!("./migrations");
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
    for m in migrator.iter().filter(|m| m.version < NEW_VERSION) {
        sqlx::raw_sql(m.sql.clone()).execute(&pool).await.unwrap();
    }
    sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();

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
    .execute(&pool)
    .await
    .unwrap();
    for m in migrator.iter().filter(|m| m.version < NEW_VERSION) {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, 1, ?, 0)",
        )
        .bind(m.version)
        .bind(m.description.as_ref())
        .bind(m.checksum.as_ref())
        .execute(&pool)
        .await
        .unwrap();
    }

    (pool, path)
}

/// 夹具行：agent_actions 最小列集（event_type NOT NULL 填 'heartbeat'）。
#[allow(clippy::too_many_arguments)]
async fn insert_action(
    pool: &SqlitePool,
    id: &str,
    workspace_id: &str,
    agent_id: &str,
    action_type: &str,
    content: &str,
    tick_id: Option<&str>,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at, tick_id)
         VALUES (?, ?, ?, 'heartbeat', ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(agent_id)
    .bind(action_type)
    .bind(content)
    .bind(created_at)
    .bind(tick_id)
    .execute(pool)
    .await
    .expect("insert fixture agent_actions row");
}

async fn tick_id_of(pool: &SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT tick_id FROM agent_actions WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("tick_id lookup")
}

#[tokio::test]
async fn migration_backfills_runs_then_relinks_proposals() {
    let (pool, path) = pre_migration_pool("main").await;

    // ── 夹具 ──
    // ws1：summary anchor（含 trace）+ 同 tick 提案/动作行。
    insert_action(
        &pool,
        "anc1",
        "ws1",
        "__heartbeat__:ws1",
        "summary",
        r#"{"taskCount":2,"result":"巡检完成，发现温度偏高","trace":[{"id":"c1","name":"read_property","args":{"p":"temp"},"output":"26.5"}]}"#,
        Some("t1"),
        "2026-10-01 10:00:00",
    )
    .await;
    insert_action(
        &pool,
        "p1",
        "ws1",
        "__heartbeat__:ws1",
        "proposal",
        r#"{"proposalId":"pp1","status":"pending","toolName":"set_property","thingId":"t1","summary":"调低设定值","reason":"温度偏高","risk":"low","parameters":{}}"#,
        Some("t1"),
        "2026-10-01 10:00:00",
    )
    .await;
    insert_action(
        &pool,
        "ae1",
        "ws1",
        "__heartbeat__:ws1",
        "auto_executed",
        r#"{"tool":"set_property","thingId":"t1","summary":"调低设定值"}"#,
        Some("t1"),
        "2026-10-01 10:00:00",
    )
    .await;
    // ws2：error anchor。
    insert_action(
        &pool,
        "anc2",
        "ws2",
        "__heartbeat__:ws2",
        "error",
        r#"{"taskCount":0,"error":"LLM 超时","trace":[]}"#,
        Some("t9"),
        "2026-10-01 11:00:00",
    )
    .await;
    // 负对照：非心跳 agent（tick_id 撞名 t1 也不动）/ NULL tick_id 提案 /
    // 无 anchor 的孤儿 tick / NULL tick_id 的 legacy anchor（无锚可合成）。
    insert_action(&pool, "nb1", "ws1", "agent_other", "proposal", r#"{"proposalId":"x"}"#, Some("t1"), "2026-10-01 10:00:00")
        .await;
    insert_action(&pool, "nb2", "ws1", "__heartbeat__:ws1", "proposal", r#"{"proposalId":"y"}"#, None, "2026-10-02 10:00:00")
        .await;
    insert_action(
        &pool,
        "nb3",
        "ws1",
        "__heartbeat__:ws1",
        "proposal",
        r#"{"proposalId":"z"}"#,
        Some("t_orphan"),
        "2026-10-03 10:00:00",
    )
    .await;
    insert_action(
        &pool,
        "nb4",
        "ws1",
        "__heartbeat__:ws1",
        "summary",
        r#"{"taskCount":0,"result":"legacy","trace":[]}"#,
        None,
        "2026-09-01 10:00:00",
    )
    .await;

    // ── 跑迁移（只有本迁移未标记，run_migrations 只执行它）──
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run_migrations");

    // ── Step 1：anchor 合成 agent_runs 行 ──
    let row: (String, String, String, String, String, String) = sqlx::query_as(
        "SELECT id, trigger_type, trigger_context, outcome, summary, created_at
         FROM agent_runs WHERE id = 'hbm_t1'",
    )
    .fetch_one(&pool)
    .await
    .expect("synthesized run row hbm_t1");
    assert_eq!(row.1, "heartbeat");
    assert_eq!(row.2, "heartbeat:ws1", "trigger='heartbeat:'||workspace_id");
    assert_eq!(row.3, "acted", "同 tick 有提案/动作行 → acted");
    assert_eq!(row.4, "巡检完成，发现温度偏高");
    assert_eq!(row.5, "2026-10-01 10:00:00", "created_at 沿用 anchor 时间戳");

    // report JSON：trace 回填 + RunReport 形状可解析（trace 端点 live:false
    // 分支的反序列化契约）。
    let (report,): (String,) = sqlx::query_as("SELECT report FROM agent_runs WHERE id = 'hbm_t1'")
        .fetch_one(&pool)
        .await
        .expect("report json");
    let parsed: RunReport = serde_json::from_str(&report).expect("report 必须可反序列化为 RunReport");
    assert_eq!(parsed.run_id, "hbm_t1");
    assert_eq!(parsed.workspace_id, "ws1");
    assert_eq!(parsed.trigger, "heartbeat:ws1");
    assert_eq!(parsed.outcome, Outcome::Acted);
    assert!(!parsed.verified);
    assert_eq!(parsed.trace.len(), 1, "anchor content 的 trace 必须回填");
    assert_eq!(parsed.trace[0].name, "read_property");
    assert_eq!(parsed.trace[0].output.as_deref(), Some("26.5"));

    // action_count 补写（T4 count_actions_last_hour 依赖
    // json_extract(report,'$.action_count')；提案+动作共 2 行）。
    let (action_count,): (i64,) =
        sqlx::query_as("SELECT json_extract(report, '$.action_count') FROM agent_runs WHERE id = 'hbm_t1'")
            .fetch_one(&pool)
            .await
            .expect("action_count");
    assert_eq!(action_count, 2);

    // error anchor → failed，summary 取 content.error。
    let (outcome, summary): (String, String) =
        sqlx::query_as("SELECT outcome, summary FROM agent_runs WHERE id = 'hbm_t9'")
            .fetch_one(&pool)
            .await
            .expect("synthesized run row hbm_t9");
    assert_eq!(outcome, "failed");
    assert_eq!(summary, "LLM 超时");

    // 只为两个带 tick 的 anchor 合成行；NULL tick 的 legacy anchor 不动。
    let (runs_total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs_total, 2);

    // ── Step 2：提案/动作行改锚 run_id（顺序断言：改锚成功即证明合成在先）──
    assert_eq!(tick_id_of(&pool, "p1").await.as_deref(), Some("hbm_t1"));
    assert_eq!(tick_id_of(&pool, "ae1").await.as_deref(), Some("hbm_t1"));
    // anchor 行自身的 tick_id 不改写（它是视图的数据源）。
    assert_eq!(tick_id_of(&pool, "anc1").await.as_deref(), Some("t1"));
    // 负对照全部不动。
    assert_eq!(tick_id_of(&pool, "nb1").await.as_deref(), Some("t1"), "非心跳 agent 不动");
    assert_eq!(tick_id_of(&pool, "nb2").await, None, "NULL tick_id 不动");
    assert_eq!(
        tick_id_of(&pool, "nb3").await.as_deref(),
        Some("t_orphan"),
        "无 anchor 的孤儿 tick 不动（EXISTS 锚检查）"
    );

    // ── Step 3：读侧兼容视图 tick_id→run_id ──
    let (run_id,): (String,) = sqlx::query_as("SELECT run_id FROM heartbeat_tick_map WHERE tick_id = 't1'")
        .fetch_one(&pool)
        .await
        .expect("heartbeat_tick_map 视图可查");
    assert_eq!(run_id, "hbm_t1");
    let (map_rows,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM heartbeat_tick_map")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(map_rows, 2, "视图只含带 tick 的 anchor（t1/t9）");

    // ── 幂等：重跑迁移 SQL 零副作用 ──
    let migrator = sqlx::migrate!("./migrations");
    let m = migrator
        .iter()
        .find(|m| m.version == NEW_VERSION)
        .expect("迁移必须在嵌入迁移集内");
    sqlx::raw_sql(m.sql.clone())
        .execute(&pool)
        .await
        .expect("迁移重跑必须成功（幂等形态）");

    let (runs_total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs_total, 2, "INSERT OR IGNORE：重跑不重复合成");
    assert_eq!(tick_id_of(&pool, "p1").await.as_deref(), Some("hbm_t1"), "重跑不得二次加前缀");
    let (double_prefixed,): (i64,) =
        sqlx::query_as(r"SELECT COUNT(*) FROM agent_actions WHERE tick_id LIKE 'hbm\_hbm\_%' ESCAPE '\'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(double_prefixed, 0, "任何行不得出现 hbm_hbm_ 双前缀");
    let (action_count,): (i64,) =
        sqlx::query_as("SELECT json_extract(report, '$.action_count') FROM agent_runs WHERE id = 'hbm_t1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(action_count, 2, "重跑后 action_count 不变");

    drop(pool);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn migration_skips_trace_less_anchor_gracefully() {
    let (pool, path) = pre_migration_pool("traceless").await;

    // 无 trace 字段的 anchor（早期 HeartbeatResult 形状）+ 无提案/动作。
    insert_action(
        &pool,
        "anc3",
        "ws3",
        "__heartbeat__:ws3",
        "summary",
        r#"{"taskCount":1,"result":"一切正常"}"#,
        Some("t2"),
        "2026-10-01 12:00:00",
    )
    .await;

    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run_migrations");

    // "历史无 trace 不补"=不伪造过程记录：行仍合成，trace 为空数组，
    // 前端"无过程记录"的展示由读侧决定。
    let (report,): (String,) = sqlx::query_as("SELECT report FROM agent_runs WHERE id = 'hbm_t2'")
        .fetch_one(&pool)
        .await
        .expect("synthesized run row hbm_t2");
    let parsed: RunReport = serde_json::from_str(&report).expect("report 必须可反序列化为 RunReport");
    assert!(parsed.trace.is_empty(), "无 trace 的历史行 trace=[]，不伪造");
    assert_eq!(parsed.outcome, Outcome::NoActionNeeded, "无提案/动作的 summary → no_action_needed");
    assert_eq!(parsed.summary, "一切正常");

    let (trace_json,): (String,) =
        sqlx::query_as("SELECT json_extract(report, '$.trace') FROM agent_runs WHERE id = 'hbm_t2'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(trace_json, "[]");
    let (action_count,): (i64,) =
        sqlx::query_as("SELECT json_extract(report, '$.action_count') FROM agent_runs WHERE id = 'hbm_t2'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(action_count, 0);

    drop(pool);
    let _ = std::fs::remove_file(path);
}

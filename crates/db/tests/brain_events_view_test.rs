//! brain_events 只读投影视图（P0）：UNION judgments ∪ agent_actions(提案)
//! ∪ agent_runs(指令) ∪ agent_actions(tick 聚合)；状态改名只在视图层
//! （noise_archived→self_closed；patrol rejected→dismissed）。

#[sqlx::test]
async fn brain_events_view_unions_three_sources(pool: sqlx::SqlitePool) {
    tinyiothub_storage::test_helpers::run_all_migrations(&pool)
        .await
        .unwrap();

    // 各插一行：judgment / agent_actions(proposal) / agent_runs(指令 run)
    // （用最小 NOT NULL 列插行；judgment 列参照 judgments 表 schema）
    sqlx::query("INSERT INTO judgments (id, workspace_id, status) VALUES ('j1', 'ws1', 'noise_archived')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content) \
         VALUES ('a1', 'ws1', 'agent1', 'patrol', 'proposal', \
         '{\"proposalId\":\"p1\",\"status\":\"pending\",\"toolName\":\"restart\",\"thingId\":\"t1\",\"summary\":\"重启网关\",\"reason\":\"离线\",\"risk\":\"low\",\"parameters\":{}}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary) \
         VALUES ('r1', 'ws1', 'user', 'completed', '用户指令执行')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM brain_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 3);

    // 状态改名映射在视图层生效
    let (status,): (String,) = sqlx::query_as("SELECT status FROM brain_events WHERE source='alarm'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // judgments.status='noise_archived' 的行应映射为 self_closed
    assert_eq!(status, "self_closed");
}

/// P0「零漂移」基线：视图是投影，必须与三源逐类相等（行数 + 状态改名映射
/// 逐条对得上）。P1 引入双写落表后，本测试即对照组——落表行与视图投影
/// 的任何漂移都会在这里现形。
#[sqlx::test]
async fn projection_matches_sources_exactly(pool: sqlx::SqlitePool) {
    tinyiothub_storage::test_helpers::run_all_migrations(&pool)
        .await
        .unwrap();

    // ---- 种源数据（每类 ≥2 行，各含一行被改名的状态）----

    // judgments ×2：一行 noise_archived（→self_closed），一行 resolved（不改名）
    sqlx::query("INSERT INTO judgments (id, workspace_id, status) VALUES ('j1', 'ws1', 'noise_archived')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO judgments (id, workspace_id, status) VALUES ('j2', 'ws1', 'resolved')")
        .execute(&pool)
        .await
        .unwrap();

    // agent_actions 提案 ×2：一行 rejected（→dismissed），一行 pending（→awaiting_approval）
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content) \
         VALUES ('a1', 'ws1', 'agent1', 'patrol', 'proposal', \
         '{\"proposalId\":\"p1\",\"status\":\"pending\",\"toolName\":\"restart\",\"thingId\":\"t1\",\"summary\":\"重启网关\",\"reason\":\"离线\",\"risk\":\"low\",\"parameters\":{}}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content) \
         VALUES ('a2', 'ws1', 'agent1', 'patrol', 'proposal', \
         '{\"proposalId\":\"p2\",\"status\":\"rejected\",\"toolName\":\"restart\",\"thingId\":\"t2\",\"summary\":\"重启传感器\",\"reason\":\"抖动\",\"risk\":\"medium\",\"parameters\":{}}')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // agent_runs 指令 ×2 命中（一行 failed→investigation_failed）+ 2 行应被过滤
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary) \
         VALUES ('r1', 'ws1', 'user', 'completed', '用户指令执行')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary) \
         VALUES ('r2', 'ws1', 'user', 'failed', '用户指令失败')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // problem_key 非 NULL → 是问题排查 run，不属于 directive，应被视图排除
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, problem_key) \
         VALUES ('r3', 'ws1', 'user', 'completed', '问题排查', 'pk1')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // trigger_type 非 user → 应被视图排除
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary) \
         VALUES ('r4', 'ws1', 'cron', 'completed', '定时任务')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // summary 行跨 2 个不同小时（10 点档 2 个 tick、11 点档 1 个 tick）→ 2 条 tick 聚合行
    for (id, tick, at) in [
        ("s1", "tick-a", "2026-09-17 10:05:00"),
        ("s2", "tick-b", "2026-09-17 10:25:00"),
        ("s3", "tick-c", "2026-09-17 11:10:00"),
    ] {
        sqlx::query(
            "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at) \
             VALUES (?, 'ws1', 'agent1', 'patrol', 'summary', '{}', ?, ?)",
        )
        .bind(id)
        .bind(tick)
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();
    }

    // ---- 行数逐类相等（视图 vs 源表直查）----
    async fn view_count(pool: &sqlx::SqlitePool, source: &str) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM brain_events WHERE source = ?")
            .bind(source)
            .fetch_one(pool)
            .await
            .unwrap();
        n
    }

    // alarm == judgments 全表
    let (src,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM judgments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(view_count(&pool, "alarm").await, src);
    assert_eq!(src, 2);

    // patrol == agent_actions 提案行
    let (src,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_actions WHERE action_type = 'proposal'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(view_count(&pool, "patrol").await, src);
    assert_eq!(src, 2);

    // directive == agent_runs 中 problem_key IS NULL 且 trigger_type='user'
    let (src,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM agent_runs WHERE problem_key IS NULL AND trigger_type = 'user'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(view_count(&pool, "directive").await, src);
    assert_eq!(src, 2);

    // tick 聚合 == summary 行的 (workspace, 小时) 分组数
    let (src,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (SELECT workspace_id, strftime('%Y-%m-%dT%H', created_at) \
         FROM agent_actions WHERE action_type = 'summary' \
         GROUP BY workspace_id, strftime('%Y-%m-%dT%H', created_at))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(view_count(&pool, "patrol_tick").await, src);
    assert_eq!(src, 2); // 2 个不同小时 → 2 组

    // 总行数 = 四类之和（UNION ALL 无去重损失）
    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM brain_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 2 + 2 + 2 + 2);

    // ---- 状态改名映射逐条对得上 ----
    async fn status_of(pool: &sqlx::SqlitePool, id: &str) -> String {
        let (s,): (String,) = sqlx::query_as("SELECT status FROM brain_events WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        s
    }

    // judgments: noise_archived→self_closed；其余原样
    assert_eq!(status_of(&pool, "alarm:j1").await, "self_closed");
    assert_eq!(status_of(&pool, "alarm:j2").await, "resolved");
    // patrol: pending→awaiting_approval；rejected→dismissed
    assert_eq!(status_of(&pool, "patrol:p1").await, "awaiting_approval");
    assert_eq!(status_of(&pool, "patrol:p2").await, "dismissed");
    // directive: failed→investigation_failed；其余→resolved
    assert_eq!(status_of(&pool, "directive:r1").await, "resolved");
    assert_eq!(status_of(&pool, "directive:r2").await, "investigation_failed");

    // ---- tick 聚合分组正确：id 按 workspace+小时，同小时多 tick 合并计数 ----
    let (title,): (String,) = sqlx::query_as("SELECT title FROM brain_events WHERE id = 'tick:ws1:2026-09-17T10'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "巡检正常 · 本小时 2 次巡检无发现"); // tick-a + tick-b
    let (title,): (String,) = sqlx::query_as("SELECT title FROM brain_events WHERE id = 'tick:ws1:2026-09-17T11'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "巡检正常 · 本小时 1 次巡检无发现"); // tick-c
    let (status,): (String,) = sqlx::query_as("SELECT status FROM brain_events WHERE id = 'tick:ws1:2026-09-17T11'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "patrol_ok");
}

/// 回归（2026-09-19 live 实测）：提案行 tick_id 恒 NULL，证据锚行连接必须用
/// created_at（同一 tick 的锚行/提案行共享）；patrol 的 suggested_action 置
/// NULL（title 即建议，英文工具名不上界面）。
#[sqlx::test]
async fn patrol_evidence_joins_on_created_at_and_suggestion_is_null(pool: sqlx::SqlitePool) {
    tinyiothub_storage::test_helpers::run_all_migrations(&pool)
        .await
        .unwrap();

    // 同一 tick：锚行（summary，有 tick_id）+ 提案行（tick_id NULL——写入方实情）
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at) \
         VALUES ('s1', 'ws1', 'agent1', 'patrol', 'summary', '{\"result\":\"巡检发现仓库湿度偏高\"}', 'tick-a', '2026-09-19 03:34:37')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at) \
         VALUES ('a1', 'ws1', 'agent1', 'patrol', 'proposal', \
         '{\"proposalId\":\"p1\",\"status\":\"pending\",\"toolName\":\"update_threshold\",\"thingId\":\"t1\",\"summary\":\"建议调整湿度阈值\",\"reason\":\"偏高\",\"risk\":\"low\",\"parameters\":{}}', \
         NULL, '2026-09-19 03:34:37')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (evidence, suggested): (String, Option<String>) =
        sqlx::query_as("SELECT evidence_json, suggested_action FROM brain_events WHERE id = 'patrol:p1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        evidence.contains("巡检发现仓库湿度偏高"),
        "证据必须命中同 tick 锚行（created_at 连接），实际: {evidence}"
    );
    assert!(
        suggested.is_none(),
        "patrol 的 suggested_action 必须为 NULL（title 即建议），实际: {suggested:?}"
    );
}

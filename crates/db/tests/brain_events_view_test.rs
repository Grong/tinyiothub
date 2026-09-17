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
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM brain_events WHERE source='alarm'")
            .fetch_one(&pool)
            .await
            .unwrap();
    // judgments.status='noise_archived' 的行应映射为 self_closed
    assert_eq!(status, "self_closed");
}

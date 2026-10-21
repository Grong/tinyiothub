//! E3 端到端（spec 2026-10-21-e3 §11）：
//! watch/job/state_change 账本 → Flow 回放读取 → 截止时刻状态重建；
//! 场景包 watch/composite 引用解析 → 落库（模板库应用的持久结果）。

use std::collections::HashMap;

use serde_json::json;
use sqlx::SqlitePool;

use crate::domains::agent::host::handler::ledger::fold_state;
use crate::domains::marketplace::template_rules::resolve_template_rules;
use tinyiothub_storage::watch::{LedgerRow, append_ledger, latest_state_changes_at, list_ledger_range};

async fn pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    tinyiothub_storage::test_helpers::run_all_migrations(&pool)
        .await
        .unwrap();
    crate::domains::agent::host::test_utils::seed_test_workspace(&pool, "tenant-e3", "ws-e3").await;
    for id in ["t1", "t2", "window", "panel"] {
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, 'ws-e3', 'device')")
            .bind(id)
            .bind(format!("Thing {id}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    pool
}

async fn ledger(pool: &SqlitePool) -> Vec<LedgerRow> {
    list_ledger_range(
        pool,
        "ws-e3",
        "2026-10-21T10:00:00Z",
        "2026-10-21T10:02:00Z",
        &[],
        None,
        None,
        100,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn watch_job_replay_visible_and_state_rebuilds_at_cutoff() {
    let pool = pool().await;
    for (ts, kind, thing, payload) in [
        (
            "2026-10-21T10:00:00Z",
            "state_change",
            Some("t1"),
            json!({"v":1,"field":"温度","new":"20"}),
        ),
        (
            "2026-10-21T10:01:00Z",
            "state_change",
            Some("t1"),
            json!({"v":1,"field":"温度","new":"22"}),
        ),
        (
            "2026-10-21T10:01:10Z",
            "watch_hit",
            Some("t1"),
            json!({"v":1,"rule_id":"wr_1","matched":true}),
        ),
        (
            "2026-10-21T10:01:11Z",
            "job",
            Some("t1"),
            json!({"v":1,"compositeId":"guard","stepIndex":0,"status":"accepted"}),
        ),
        (
            "2026-10-21T10:01:12Z",
            "job",
            Some("t2"),
            json!({"v":1,"compositeId":"guard","stepIndex":1,"status":"result"}),
        ),
        (
            "2026-10-21T10:01:13Z",
            "state_change",
            Some("t2"),
            json!({"v":1,"field":"状态","new":"closed"}),
        ),
    ] {
        append_ledger(&pool, "ws-e3", ts, kind, thing, &payload.to_string())
            .await
            .unwrap();
    }

    // Flow 数据面：6 条事件按 id 升序，watch 与 composite 两步 job 都在。
    let rows = ledger(&pool).await;
    assert_eq!(rows.len(), 6);
    assert!(rows.windows(2).all(|w| w[0].id < w[1].id));
    assert_eq!(rows.iter().filter(|r| r.kind == "watch_hit").count(), 1);
    let jobs: Vec<&LedgerRow> = rows.iter().filter(|r| r.kind == "job").collect();
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|r| r.payload.contains("guard")));

    // 状态重建：截止 10:01:12 → t1 温度=22；t2 的 10:01:13 变更必须被排除。
    let state_rows = latest_state_changes_at(&pool, "ws-e3", "2026-10-21T10:01:12Z")
        .await
        .unwrap();
    let devices = fold_state(&state_rows);
    let t1 = devices.iter().find(|d| d["thingId"] == "t1").expect("t1 state");
    assert_eq!(t1["state"]["温度"], "22");
    assert!(
        devices.iter().all(|d| d["thingId"] != "t2"),
        "截止后的变更不得出现在历史状态中: {devices:?}"
    );
}

#[tokio::test]
async fn template_rules_resolve_then_land_as_watch_and_composite() {
    let pool = pool().await;
    let mapping = HashMap::from([
        ("wind".to_string(), "t1".to_string()),
        ("window".to_string(), "window".to_string()),
        ("panel".to_string(), "panel".to_string()),
    ]);
    let template = json!({
        "watch_rules": [
            {"ref":"wind_high","thing":"wind","kind":"property","property":"风速","op":"gt","value":20,"persistent":true,"timeout_secs":3600}
        ],
        "composites": [
            {"name":"storm_guard","description":"","steps":[
                {"op":"call","thing":"window","action":"close","params":{}},
                {"op":"call","thing":"panel","action":"retract","params":{},
                 "rollback":{"op":"call","thing":"panel","action":"extend","params":{}}}
            ]}
        ]
    });
    let (resolved, report) = resolve_template_rules(&template, "ws-e3", &mapping, &[]);
    assert!(report.ok, "{report:?}");
    assert_eq!(report.resolved.watch_rules, 1);
    assert_eq!(report.resolved.composites, 1);

    // 模拟应用端点的事务落库结果（同一 schema / 同一序列化）。
    let rule = &resolved.watch_rules[0];
    let condition = serde_json::to_value(&rule.condition).unwrap();
    let row = tinyiothub_storage::watch::WatchRuleRow {
        id: rule.id.clone(),
        workspace_id: "ws-e3".into(),
        kind: condition["type"].as_str().unwrap_or("property").into(),
        condition: condition.to_string(),
        persistent: i64::from(rule.persistent),
        timeout_secs: i64::from(rule.timeout_secs),
        created_at: rule.created_at.clone(),
    };
    tinyiothub_storage::watch::insert_rule(&pool, &row).await.unwrap();

    let def = &resolved.composites[0];
    tinyiothub_storage::composite::insert_composite(
        &pool,
        &tinyiothub_storage::composite::CompositeRow {
            id: def.id.clone(),
            workspace_id: "ws-e3".into(),
            name: def.name.clone(),
            description: def.description.clone(),
            definition: serde_json::to_string(def).unwrap(),
            created_at: "2026-10-21T10:00:00Z".into(),
        },
    )
    .await
    .unwrap();

    let rules = tinyiothub_storage::watch::list_rules(&pool).await.unwrap();
    assert_eq!(rules.iter().filter(|r| r.workspace_id == "ws-e3").count(), 1);
    let composites = tinyiothub_storage::composite::list_composites(&pool, "ws-e3")
        .await
        .unwrap();
    assert_eq!(composites.len(), 1);
    let parsed: tinyiothub_agent::runtime::thing_agent::seams::composite::CompositeDef =
        serde_json::from_str(&composites[0].definition).unwrap();
    assert_eq!(parsed.name, "storm_guard");
    assert_eq!(parsed.steps.len(), 2);
    assert!(parsed.steps[1].rollback.is_some(), "回滚声明必须落库");
}

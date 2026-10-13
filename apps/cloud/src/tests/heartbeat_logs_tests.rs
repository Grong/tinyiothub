//! Task 10（C6 读侧兼容视角）：get_logs 读侧切换 agent_runs 的 handler 级测试。
//!
//! - 切换后主查询读 agent_runs（trigger 前缀 heartbeat:），RunReport 行经
//!   纯函数适配器映射为 ExecutionRecord（前端 ai-ops 契约形状不变）；
//!   提案/auto_executed 明细仍按 tick_id=run_id 从 agent_actions 联结。
//! - 兼容期 fallback：agent_runs 查空且旧 agent_actions 锚行仍在时回旧查询。

use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_admin_role, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-logs";

fn get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .body(Body::empty())
        .unwrap()
}

async fn app() -> (axum::Router, sqlx::SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    seed_admin_role(&pool, "user-1").await;
    let api = crate::api::create_router(&app_state);
    (axum::Router::new().nest("/api", api).with_state(app_state), pool)
}

async fn get_logs(pool_app: axum::Router) -> Value {
    let token = create_test_token("user-1", "tenant-1");
    let response = pool_app
        .oneshot(get(&format!("/api/v1/workspaces/{WS}/heartbeat/logs"), &token))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(json["code"], 0, "get_logs 必须成功: {status} {json}");
    json["result"]["logs"].clone()
}

/// 种一条心跳 run 行（T8 起统一内核落库形态：trigger_context='heartbeat:{ws}'）。
async fn seed_run(pool: &sqlx::SqlitePool, id: &str, outcome: &str, summary: &str, report: Value, created_at: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, trigger_context, outcome, summary, report, created_at) \
         VALUES (?, ?, 'heartbeat', ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(WS)
    .bind(format!("heartbeat:{WS}"))
    .bind(outcome)
    .bind(summary)
    .bind(report.to_string())
    .bind(created_at)
    .execute(pool)
    .await
    .expect("seed run");
}

/// 种一条 tick 明细行（提案/自动执行；tick_id=run_id，C3 联结）。
async fn seed_detail(pool: &sqlx::SqlitePool, tick_id: &str, action_type: &str, content: Value, created_at: &str) {
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at, tick_id) \
         VALUES (?, ?, ?, 'heartbeat', ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(WS)
    .bind(format!("__heartbeat__:{WS}"))
    .bind(action_type)
    .bind(content.to_string())
    .bind(created_at)
    .bind(tick_id)
    .execute(pool)
    .await
    .expect("seed detail");
}

/// 种一条旧锚行（兼容期 fallback 数据源；tick_id 无对应 run 行）。
async fn seed_legacy_anchor(pool: &sqlx::SqlitePool, action_type: &str, content: Value, created_at: &str) {
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at) \
         VALUES (?, ?, ?, 'heartbeat', ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(WS)
    .bind(format!("__heartbeat__:{WS}"))
    .bind(action_type)
    .bind(content.to_string())
    .bind(created_at)
    .execute(pool)
    .await
    .expect("seed legacy anchor");
}

/// 切换后：get_logs 从 agent_runs 组装——trace 进条目，提案/动作明细按
/// tick_id=run_id 联结进同一条目。
#[tokio::test]
async fn get_logs_reads_from_agent_runs_after_switch() {
    let (app, pool) = app().await;
    seed_run(
        &pool,
        "run_1",
        "acted",
        "巡检完成：发现 1 个待审批提案",
        json!({
            "run_id": "run_1",
            "trace": [{"id": "c1", "name": "read_property", "args": {"p": "temp"}, "output": "26.5"}],
            "action_count": 2,
            "task_count": 2,
        }),
        "2026-10-13 01:00:00",
    )
    .await;
    seed_detail(
        &pool,
        "run_1",
        "proposal",
        json!({
            "proposalId": "p1", "status": "pending", "level": "high",
            "toolName": "firmware_update", "thingId": "dev_1", "deviceName": "Thermostat",
            "summary": "升级固件", "reason": "安全补丁", "risk": "high",
        }),
        "2026-10-13 01:00:00",
    )
    .await;
    seed_detail(
        &pool,
        "run_1",
        "auto_executed",
        json!({"tool": "device_control", "thingId": "dev_2", "summary": "restarted"}),
        "2026-10-13 01:00:00",
    )
    .await;

    let logs = get_logs(app).await;
    let arr = logs.as_array().expect("logs array");
    assert_eq!(arr.len(), 1, "只有一条 run → 一条日志: {logs}");
    let log = &arr[0];
    assert_eq!(log["timestamp"], "2026-10-13 01:00:00");
    assert_eq!(log["status"], "success");
    assert_eq!(log["result"], "巡检完成：发现 1 个待审批提案");
    assert_eq!(log["taskCount"], 2, "修复轮 1：report.task_count 必须透出为 taskCount");
    assert_eq!(log["trace"].as_array().unwrap().len(), 1, "trace 必须进条目");
    assert_eq!(log["trace"][0]["name"], "read_property");
    assert_eq!(log["pendingProposals"][0]["toolName"], "firmware_update");
    assert_eq!(log["pendingProposals"][0]["status"], "pending");
    assert_eq!(log["autoExecuted"][0]["tool"], "device_control");
    assert_eq!(log["autoExecuted"][0]["thingId"], "dev_2");
}

/// 统一内核心跳 run 的 summary 是 LLM 自愈报告原文（带 ```json 围栏）——
/// 适配器抽出其中 summary 字段作为人读 result，而不是把整段 JSON 糊上前端。
#[tokio::test]
async fn get_logs_extracts_summary_from_healing_report_text() {
    let (app, pool) = app().await;
    seed_run(
        &pool,
        "run_fenced",
        "acted",
        "巡检结论如下：\n```json\n{\"summary\": \"温度偏高已处理\", \"proposals\": []}\n```",
        json!({"trace": []}),
        "2026-10-13 02:00:00",
    )
    .await;

    let logs = get_logs(app).await;
    assert_eq!(logs[0]["result"], "温度偏高已处理", "应抽自愈报告 summary 字段: {logs}");
}

/// error 行三态之一：outcome=failed → status=error + errorMessage；历史无
/// trace 行（report 缺 trace 字段）→ trace=[]，前端显示「无过程记录」不炸。
#[tokio::test]
async fn get_logs_maps_failed_run_and_empty_trace() {
    let (app, pool) = app().await;
    seed_run(&pool, "run_err", "failed", "LLM 超时", json!({}), "2026-10-13 03:00:00").await;

    let logs = get_logs(app).await;
    let log = &logs[0];
    assert_eq!(log["status"], "error");
    assert_eq!(log["errorMessage"], "LLM 超时");
    assert!(log["result"].is_null(), "error 行 result 必须为空: {log}");
    assert_eq!(log["trace"], json!([]), "无 trace → 空数组不炸: {log}");
}

/// 兼容期 fallback：agent_runs 查空（旧 tick 未迁移/无 run 行）且
/// agent_actions 有锚行时，回旧查询组装（双读，切换完成后删）。
#[tokio::test]
async fn get_logs_falls_back_to_legacy_agent_actions_when_no_runs() {
    let (app, pool) = app().await;
    let ts = "2026-07-20 10:00:00";
    seed_legacy_anchor(&pool, "summary", json!({"taskCount": 2, "result": "legacy ok"}), ts).await;
    // 旧明细行按 created_at 分组（旧装配逻辑）
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at) \
         VALUES (?, ?, ?, 'heartbeat', 'proposal', ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(WS)
    .bind(format!("__heartbeat__:{WS}"))
    .bind(
        json!({"proposalId": "lp1", "status": "pending", "toolName": "reboot", "thingId": "dev_9",
               "deviceName": "", "summary": "重启", "reason": "卡顿", "risk": "low"})
        .to_string(),
    )
    .bind(ts)
    .execute(&pool)
    .await
    .expect("seed legacy proposal");

    let logs = get_logs(app).await;
    let arr = logs.as_array().unwrap();
    assert_eq!(arr.len(), 1, "fallback 必须给出旧锚行组装结果: {logs}");
    assert_eq!(arr[0]["result"], "legacy ok");
    assert_eq!(arr[0]["taskCount"], 2);
    assert_eq!(arr[0]["pendingProposals"][0]["toolName"], "reboot");
}

/// 双读优先级：agent_runs 有行时旧锚行不再出现（新源为准，不重复）。
#[tokio::test]
async fn get_logs_prefers_agent_runs_when_both_sources_exist() {
    let (app, pool) = app().await;
    seed_legacy_anchor(
        &pool,
        "summary",
        json!({"taskCount": 1, "result": "legacy should not show"}),
        "2026-07-20 10:00:00",
    )
    .await;
    seed_run(
        &pool,
        "run_new",
        "no_action_needed",
        "新源巡检正常",
        json!({"trace": []}),
        "2026-10-13 04:00:00",
    )
    .await;

    let logs = get_logs(app).await;
    let arr = logs.as_array().unwrap();
    assert_eq!(arr.len(), 1, "新源非空时旧源不得混入: {logs}");
    assert_eq!(arr[0]["result"], "新源巡检正常");
}

/// 非心跳 run（trigger 前缀非 heartbeat:）不得混入心跳日志。
#[tokio::test]
async fn get_logs_excludes_non_heartbeat_runs() {
    let (app, pool) = app().await;
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, trigger_context, outcome, summary, report) \
         VALUES ('run_thing', ?, 'thing', 'thing:t1:event:temp_high', 'acted', '事件处理', '{}')",
    )
    .bind(WS)
    .execute(&pool)
    .await
    .expect("seed thing run");

    let logs = get_logs(app).await;
    assert_eq!(logs.as_array().unwrap().len(), 0, "thing 事件 run 不得混入: {logs}");
}

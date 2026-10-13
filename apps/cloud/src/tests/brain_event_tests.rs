//! AI 大脑 P0 Task 3：/brain-events 只读端点 handler 测试（HTTP 层，仿 judgment_handler_tests）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

fn req(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token));
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder
        .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
        .unwrap()
}

/// 插入一条 judgment（investigating）；verdict 给了就判定（self_healable → 待审批，
/// noise → 已消化 self_closed）。evidence 可选（默认 "{}"）。
/// thing_id 逐条区分——alarm 源同 thing+rule 会折叠（Task 2 语义），同 thing 的
/// 多条测试数据会被折成一行。
async fn seed_judgment(
    app_state: &crate::state::AppState,
    ws: &str,
    thing_id: &str,
    verdict: Option<&str>,
    evidence: Option<&str>,
) -> String {
    let jid = app_state
        .db
        .insert_judgment(ws, None, None, Some(thing_id), "annotate")
        .await
        .unwrap();
    if let Some(v) = verdict {
        let verdict = match v {
            "noise" => tinyiothub_storage::judgment::JudgmentVerdict::Noise,
            "self_healable" => tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            _ => tinyiothub_storage::judgment::JudgmentVerdict::NeedsHuman,
        };
        app_state
            .db
            .judge_judgment(
                &jid,
                verdict,
                "测试理由",
                evidence.unwrap_or("{}"),
                Some("重连"),
                Some("connection_recovery"),
                None,
                None,
            )
            .await
            .unwrap();
    }
    jid
}

async fn insert_ticket(pool: &sqlx::SqlitePool, state: &str, failure_hash: &str) -> i64 {
    let r = sqlx::query(
        "INSERT INTO tickets (workspace_id, agent_run_id, title, briefing, failure_hash, state)
         VALUES ('ws-default-001','run-x','t','{}',?,?)",
    )
    .bind(failure_hash)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
    r.last_insert_rowid()
}

async fn escalate(app_state: &crate::state::AppState, jid: &str, ticket_id: i64) {
    app_state
        .db
        .transit_judgment(
            jid,
            tinyiothub_storage::judgment::JudgmentStatus::Investigating,
            tinyiothub_storage::judgment::JudgmentStatus::Escalated,
            Some(ticket_id),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn brain_events_list_needs_you_tab() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let ws = "ws-default-001";
    // 待审批（算需要你）
    let _j1 = seed_judgment(&app_state, ws, "t1", Some("self_healable"), None).await;
    // 升级 + 工单 open（未认领，算需要你）
    let j2 = seed_judgment(&app_state, ws, "t2", None, None).await;
    let open_ticket = insert_ticket(&pool, "open", "h-open").await;
    escalate(&app_state, &j2, open_ticket).await;
    // 升级 + 工单 claimed（已认领，不算）
    let j3 = seed_judgment(&app_state, ws, "t3", None, None).await;
    let claimed_ticket = insert_ticket(&pool, "claimed", "h-claimed").await;
    escalate(&app_state, &j3, claimed_ticket).await;
    // 已消化（不算）
    let _j4 = seed_judgment(&app_state, ws, "t4", Some("noise"), None).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .clone()
        .oneshot(req("GET", "/api/v1/brain-events?tab=needs_you", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let data = json["result"].as_array().expect("data array");
    assert_eq!(data.len(), 2, "只含待审批 + 未认领升级: {data:?}");
    let statuses: Vec<&str> = data.iter().map(|e| e["status"].as_str().unwrap()).collect();
    assert!(statuses.contains(&"awaiting_approval"));
    assert!(statuses.contains(&"escalated"));

    // 默认 tab = needs_you
    let response = app
        .clone()
        .oneshot(req("GET", "/api/v1/brain-events", &token, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["result"].as_array().unwrap().len(), 2, "默认 tab 为 needs_you");

    // 未知 tab → 响亮 400（fail-closed，同 /judgments）
    let response = app
        .oneshot(req("GET", "/api/v1/brain-events?tab=bogus", &token, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 400);
}

#[tokio::test]
async fn brain_events_detail_carries_evidence() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let evidence = r#"{"run":"r1","steps":[]}"#;
    let jid = seed_judgment(&app_state, "ws-default-001", "t1", Some("noise"), Some(evidence)).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    // list 行不带证据
    let response = app
        .clone()
        .oneshot(req("GET", "/api/v1/brain-events?tab=all", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let data = json["result"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    assert!(data[0].get("evidence").is_none(), "list 行不得含证据: {data:?}");
    assert!(data[0].get("evidenceJson").is_none());
    let event_id = data[0]["id"].as_str().unwrap().to_string();
    assert_eq!(event_id, format!("alarm:{jid}"));

    // detail 带完整证据
    let response = app
        .clone()
        .oneshot(req("GET", &format!("/api/v1/brain-events/{event_id}"), &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["result"]["id"], event_id);
    assert_eq!(
        json["result"]["evidence"],
        serde_json::from_str::<Value>(evidence).unwrap()
    );

    // 不存在的 id → 404
    let response = app
        .oneshot(req("GET", "/api/v1/brain-events/alarm:no-such", &token, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 404);
}

/// Task 10 评审 I-1 修复契约：detail HTTP 响应必须透出 trace + narrative——
/// judgment 类含 trace（trace_run_id → report）与 narrative；patrol 提案类含
/// trace（tick_id → 锚行 content.trace）；降级时字段为空不炸。
#[tokio::test]
async fn brain_events_detail_carries_trace_and_narrative() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let ws = "ws-default-001";

    // ── judgment 类：trace（evidence.trace_run_id → report）+ narrative ──
    let report = tinyiothub_storage::agent_runs::RunReport {
        run_id: "r-http".to_string(),
        workspace_id: ws.to_string(),
        trigger: "alarm".to_string(),
        outcome: tinyiothub_storage::agent_runs::Outcome::NoActionNeeded,
        summary: "调查完毕".to_string(),
        actions: vec![],
        verified: false,
        duration_ms: 100,
        tool_calls: 1,
        tokens: 10,
        end_reason: None,
        thing_id: Some("t1".to_string()),
        trace: vec![tinyiothub_storage::agent_runs::ToolTraceEntry {
            id: "c1".to_string(),
            name: "read_property".to_string(),
            args: serde_json::json!({"p": "temp"}),
            output: Some("26.5".to_string()),
        }],
        task_count: None,
    };
    app_state
        .db
        .insert_agent_run(&report, Some("alarm:t1:-"), None)
        .await
        .unwrap();
    let jid = app_state
        .db
        .insert_judgment(ws, None, None, Some("t1"), "annotate")
        .await
        .unwrap();
    let evidence = serde_json::json!({"trace_run_id": "r-http", "summary": "调查完毕"}).to_string();
    app_state
        .db
        .judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::Noise,
            "波动",
            &evidence,
            None,
            None,
            None,
            Some("温度短暂波动后自行恢复"),
        )
        .await
        .unwrap();

    // ── patrol 提案类：trace（tick_id → 锚行 content.trace）──
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
         VALUES ('act-sum','ws-default-001','agent1','patrol','summary',?,'tk1',?)",
    )
    .bind(
        serde_json::json!({
            "taskCount": 1, "result": "ok",
            "trace": [{"id":"c9","name":"list_things","args":{},"output":"3 devices"}],
        })
        .to_string(),
    )
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
         VALUES ('act-prop','ws-default-001','agent1','patrol','proposal',?,'tk1',?)",
    )
    .bind("{\"proposalId\":\"p1\",\"thingId\":\"t1\",\"summary\":\"s-p1\",\"status\":\"pending\"}")
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();
    // 无 tick_id 提案（降级路径）
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at)
         VALUES ('act-prop2','ws-default-001','agent1','patrol','proposal',?,?)",
    )
    .bind("{\"proposalId\":\"p2\",\"thingId\":\"t1\",\"summary\":\"s-p2\",\"status\":\"pending\"}")
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    // judgment detail：trace + narrative 都透出
    let response = app
        .clone()
        .oneshot(req("GET", &format!("/api/v1/brain-events/alarm:{jid}"), &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let result = &json["result"];
    assert_eq!(
        result["trace"].as_array().map(|t| t.len()),
        Some(1),
        "judgment detail 透出 trace: {result:?}"
    );
    assert_eq!(result["trace"][0]["name"], "read_property");
    assert_eq!(result["trace"][0]["output"], "26.5");
    assert_eq!(
        result["narrative"], "温度短暂波动后自行恢复",
        "judgment detail 透出 narrative"
    );

    // patrol 提案 detail：trace 透出
    let response = app
        .clone()
        .oneshot(req("GET", "/api/v1/brain-events/patrol:p1", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let result = &json["result"];
    assert_eq!(
        result["trace"].as_array().map(|t| t.len()),
        Some(1),
        "提案 detail 透出锚 trace: {result:?}"
    );
    assert_eq!(result["trace"][0]["name"], "list_things");
    assert!(result["narrative"].is_null(), "非 judgment 行 narrative 为 null");
    // T12 结转：tick_id 透出（前端提案卡片「查看完整轨迹」链接兜底依据）
    assert_eq!(result["tickId"], "tk1", "提案行透出 tickId: {result:?}");

    // 降级：无 tick_id 提案 → trace 空数组、narrative null，200 不炸
    let response = app
        .oneshot(req("GET", "/api/v1/brain-events/patrol:p2", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let result = &json["result"];
    assert_eq!(
        result["trace"].as_array().map(|t| t.len()),
        Some(0),
        "降级为空 trace 不炸: {result:?}"
    );
    assert!(result["narrative"].is_null());
    assert!(result["tickId"].is_null(), "无 tick_id 提案降级 null: {result:?}");
}

#[tokio::test]
async fn brain_events_summary_counts() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let ws = "ws-default-001";
    // 1 条已消化（noise → self_closed，digested_today）
    let j_noise = seed_judgment(&app_state, ws, "t1", Some("noise"), None).await;
    // 1 条待审批（needs_you）
    let _j_pending = seed_judgment(&app_state, ws, "t2", Some("self_healable"), None).await;
    // 1 条反馈（right）
    app_state
        .db
        .add_judgment_feedback(&j_noise, ws, "user-1", "right", None)
        .await
        .unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(req("GET", "/api/v1/brain-events/summary", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let result = &json["result"];
    assert_eq!(result["digestedToday"], 1);
    assert_eq!(result["needsYou"], 1);
    assert!(result["latencyP50Secs"].is_number(), "已判定 alarm 源有 p50");
    assert_eq!(result["feedbackRight"], 1);
    assert_eq!(result["feedbackWrong"], 0);
}

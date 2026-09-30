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

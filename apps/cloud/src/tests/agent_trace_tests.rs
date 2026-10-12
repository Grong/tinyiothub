//! AI 运行透明化 Task 5：GET /agent-runs/{run_id}/trace 三分支 handler 测试
//! + JudgmentDto.runId 断面断言（HTTP 层，仿 brain_event_tests）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use tinyiothub_agent::runtime::thing_agent::runner::MAX_TOOL_CALLS_PER_RUN;
use tinyiothub_agent::runtime::thing_agent::{ActiveRunRegistry, RunContext, ToolTraceEntry};

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

fn req(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .body(Body::empty())
        .unwrap()
}

fn entry(id: &str, name: &str, output: Option<&str>) -> ToolTraceEntry {
    ToolTraceEntry {
        id: id.to_string(),
        name: name.to_string(),
        args: serde_json::json!({"k": 1}),
        output: output.map(str::to_owned),
    }
}

fn ended_report(
    run_id: &str,
    workspace_id: &str,
    trace: Vec<ToolTraceEntry>,
) -> tinyiothub_core::agent_runs::RunReport {
    tinyiothub_core::agent_runs::RunReport {
        run_id: run_id.to_string(),
        workspace_id: workspace_id.to_string(),
        trigger: "timer:ws".to_string(),
        outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
        summary: "巡检无异常".to_string(),
        actions: vec![],
        verified: false,
        duration_ms: 12,
        tool_calls: trace.len() as u32,
        tokens: 5,
        end_reason: None,
        thing_id: None,
        trace,
    }
}

/// live 分支：registry 放一条进行中 run → 200 live:true，trace 来自内存。
#[tokio::test]
async fn trace_live_from_registry() {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;

    let registry = ActiveRunRegistry::new(8);
    let ctx = RunContext::new("run_live_1".into(), "ws-default-001".into(), "timer".into());
    ctx.inner
        .write()
        .await
        .trace
        .push(entry("c1", "read_property", Some("26.5")));
    ctx.inner.write().await.trace.push(entry("c2", "invoke_action", None));
    let _guard = registry.register(ctx).expect("容量内");
    app_state.set_active_runs(registry);

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(req("GET", "/api/v1/agent-runs/run_live_1/trace", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let result = &json["result"];
    assert_eq!(result["live"], true);
    assert_eq!(result["budgetMax"], MAX_TOOL_CALLS_PER_RUN);
    assert!(result["summary"].is_null(), "进行中 run 无结论——summary 为 null");
    let entries = result["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["id"], "c1");
    assert_eq!(entries[0]["name"], "read_property");
    assert_eq!(entries[0]["args"], serde_json::json!({"k": 1}));
    assert_eq!(entries[0]["output"], "26.5");
    assert!(entries[1]["output"].is_null(), "未配对 ToolResult → output:null");
}

/// ended 分支：agent_runs.report 带 trace → 200 live:false（不写 SQL JSON 函数，
/// 整体反序列化取 trace）。
#[tokio::test]
async fn trace_ended_from_report() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let report = ended_report(
        "run_done_1",
        "ws-default-001",
        vec![entry("c9", "query_events", Some("[]"))],
    );
    app_state.db.insert_agent_run(&report, None, None).await.unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(req("GET", "/api/v1/agent-runs/run_done_1/trace", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let result = &json["result"];
    assert_eq!(result["live"], false);
    assert_eq!(result["budgetMax"], MAX_TOOL_CALLS_PER_RUN);
    // 终审 I-1：ended 分支透出 report.summary——详情页主路径（trace 命中）
    // 的 verdict/think/正文段靠它渲染。
    assert_eq!(result["summary"], "巡检无异常");
    let entries = result["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], "c9");
    assert_eq!(entries[0]["output"], "[]");
}

/// 404 分支：registry 未命中且 report 无此 run_id → code 404。
#[tokio::test]
async fn trace_unknown_404() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(req("GET", "/api/v1/agent-runs/run_nope/trace", &token))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 404);
}

/// workspace 隔离：run 存在但属其他 workspace（内存/落库两源）→ 404，不泄漏存在性。
#[tokio::test]
async fn trace_cross_workspace_404() {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;

    let registry = ActiveRunRegistry::new(8);
    let ctx = RunContext::new("run_live_x".into(), "ws-other-999".into(), "timer".into());
    let _guard = registry.register(ctx).expect("容量内");
    app_state.set_active_runs(registry);
    let report = ended_report("run_done_x", "ws-other-999", vec![entry("c1", "t", None)]);
    app_state.db.insert_agent_run(&report, None, None).await.unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    for run_id in ["run_live_x", "run_done_x"] {
        let response = app
            .clone()
            .oneshot(req("GET", &format!("/api/v1/agent-runs/{run_id}/trace"), &token))
            .await
            .unwrap();
        let (_s, json) = response_parts(response).await;
        assert_eq!(json["code"], 404, "跨 workspace 命中必须 404: {run_id}");
    }
}

/// JudgmentDto.runId 断面：/judgments 列表行携带 runId（camelCase 自动）。
#[tokio::test]
async fn judgments_list_carries_run_id() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    app_state
        .db
        .insert_judgment("ws-default-001", None, Some("run-j-1"), Some("t1"), "annotate")
        .await
        .unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(req("GET", "/api/v1/judgments?tab=investigating", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let data: &Value = &json["result"];
    let rows = data.as_array().expect("judgment rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["runId"], "run-j-1", "JudgmentDto 必须带 runId");
}

/// 心跳执行历史的 trace 透出（investigate 断口修复）：anchor content 含
/// trace（T9 起落库）→ get_logs 必须透到 HeartbeatLogEntry，不得丢弃。
#[tokio::test]
async fn heartbeat_logs_carry_anchor_trace() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;

    let content = serde_json::json!({
        "taskCount": 2,
        "result": "巡检完成，温度正常",
        "trace": [{
            "id": "c1",
            "name": "read_property",
            "args": {"thingId": "t1", "p": "temperature"},
            "output": "26.5"
        }],
    });
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, tick_id, created_at)
         VALUES ('hb-trace-1', 'ws-default-001', 'agent1', 'heartbeat', 'summary', ?, 'tick-1', '2026-10-12 10:00:00')",
    )
    .bind(content.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    let response = app
        .oneshot(req("GET", "/api/v1/workspaces/ws-default-001/heartbeat/logs", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let logs = json["result"]["logs"].as_array().expect("logs array");
    assert_eq!(logs.len(), 1);
    let trace = logs[0]["trace"].as_array().expect("trace 必须透出");
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0]["name"], "read_property");
    assert_eq!(trace[0]["output"], "26.5");
}

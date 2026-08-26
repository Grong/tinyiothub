//! Agent tasks API integration tests (T14)
//!
//! 用户指令入口四端点：POST /agent/tasks、GET /agent/runs、
//! POST /agent/runs/{id}/ack、GET/PUT /agent/policy。
//! 全部 workspace 隔离 + admin 角色；队列满 → 429；ack 幂等。

use std::sync::Arc;

use crate::domains::agent::host::directive_sink::StubDirectiveSink;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tinyiothub_agent::runtime::thing_agent::{EnqueueError, TriggerSource};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-agent-t14";

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    builder.body(Body::from(body_str)).unwrap()
}

/// 给测试用户授予 admin 角色（roles.role-admin 由基础迁移 seed）。
async fn grant_admin(pool: &sqlx::SqlitePool, user_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO roles (id, name, description, is_administrator) \
         VALUES ('role-admin', '系统管理员', '拥有系统所有权限', 1)",
    )
    .execute(pool)
    .await
    .expect("seed admin role");
    sqlx::query("INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES (?, ?, 'role-admin')")
        .bind(format!("ur-{user_id}"))
        .bind(user_id)
        .execute(pool)
        .await
        .expect("grant admin");
}

/// 构造注入 stub sink 的完整路由（生产同款路由树）。
async fn app_with_sink(sink: Arc<StubDirectiveSink>) -> (Router, sqlx::SqlitePool) {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    app_state.set_directive_sink(sink);
    seed_test_workspace(&pool, "tenant-1", WS).await;
    grant_admin(&pool, "user-1").await;
    let api_router = crate::api::create_router(&app_state);
    (Router::new().nest("/api", api_router).with_state(app_state), pool)
}

/// 无 sink（生产未接线状态）的路由。
async fn app_without_sink() -> (Router, sqlx::SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    grant_admin(&pool, "user-1").await;
    let api_router = crate::api::create_router(&app_state);
    (Router::new().nest("/api", api_router).with_state(app_state), pool)
}

async fn insert_run(pool: &sqlx::SqlitePool, id: &str, workspace_id: &str, summary: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, summary, report) \
         VALUES (?, ?, 'user', 'acted', ?, '{}')",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(summary)
    .execute(pool)
    .await
    .expect("insert agent_run");
}

// ── POST /agent/tasks ──────────────────────────────

#[tokio::test]
async fn create_task_dispatches_user_directive() {
    let stub = Arc::new(StubDirectiveSink::default());
    let (app, _pool) = app_with_sink(stub.clone()).await;
    let token = create_test_token("user-1", "tenant-1");

    let (status, json) = response_parts(
        app.oneshot(auth_request(
            "POST",
            &format!("/api/v1/workspaces/{WS}/agent/tasks"),
            &token,
            Some(json!({"text": "  把 3 号产线温度调到 25 度 "})),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "dispatch must succeed: {json}");
    assert!(json["result"]["taskId"].as_str().is_some_and(|t| !t.is_empty()));

    let signals = stub.signals();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].workspace_id, WS);
    match &signals[0].source {
        TriggerSource::UserDirective {
            user_id, text, source, ..
        } => {
            assert_eq!(user_id, "user-1");
            assert_eq!(text, "把 3 号产线温度调到 25 度", "text trimmed before dispatch");
            assert!(source.is_none());
        }
        other => panic!("expected UserDirective, got {other:?}"),
    }
}

#[tokio::test]
async fn create_task_empty_text_400() {
    let (app, _pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = response_parts(
        app.oneshot(auth_request(
            "POST",
            &format!("/api/v1/workspaces/{WS}/agent/tasks"),
            &token,
            Some(json!({"text": "   "})),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 400);
}

#[tokio::test]
async fn create_task_queue_full_429() {
    let stub = Arc::new(StubDirectiveSink::failing(EnqueueError::Rejected));
    let (app, _pool) = app_with_sink(stub).await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = response_parts(
        app.oneshot(auth_request(
            "POST",
            &format!("/api/v1/workspaces/{WS}/agent/tasks"),
            &token,
            Some(json!({"text": "重启网关"})),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 429, "queue full must map to 429: {json}");
}

#[tokio::test]
async fn create_task_without_sink_503() {
    let (app, _pool) = app_without_sink().await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = response_parts(
        app.oneshot(auth_request(
            "POST",
            &format!("/api/v1/workspaces/{WS}/agent/tasks"),
            &token,
            Some(json!({"text": "重启网关"})),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 503, "sink 未接线（T15 前）必须 503: {json}");
}

#[tokio::test]
async fn endpoints_require_admin_role() {
    let (app, pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    insert_run(&pool, "run-1", WS, "s").await;
    let non_admin = create_test_token("user-2", "tenant-1");

    for (method, uri, body) in [
        (
            "POST",
            format!("/api/v1/workspaces/{WS}/agent/tasks"),
            Some(json!({"text": "x"})),
        ),
        ("GET", format!("/api/v1/workspaces/{WS}/agent/runs"), None),
        ("POST", format!("/api/v1/workspaces/{WS}/agent/runs/run-1/ack"), None),
        ("GET", format!("/api/v1/workspaces/{WS}/agent/policy"), None),
        (
            "PUT",
            format!("/api/v1/workspaces/{WS}/agent/policy"),
            Some(json!({"mode": "act"})),
        ),
    ] {
        let app = app.clone();
        let (_status, json) =
            response_parts(app.oneshot(auth_request(method, &uri, &non_admin, body)).await.unwrap()).await;
        assert_eq!(json["code"], 403, "{method} {uri} non-admin must be 403: {json}");
    }
}

#[tokio::test]
async fn endpoints_enforce_workspace_isolation() {
    let (app, pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    // 工作空间属于 tenant-2，token 属于 tenant-1 → 403 越权
    seed_test_workspace(&pool, "tenant-2", "ws-other-tenant").await;
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/workspaces/ws-other-tenant/agent/tasks",
                &token,
                Some(json!({"text": "x"})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 403, "cross-tenant workspace must be 403: {json}");

    // 工作空间不存在 → 404
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "POST",
                "/api/v1/workspaces/ws-missing/agent/tasks",
                &token,
                Some(json!({"text": "x"})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 404, "unknown workspace must be 404: {json}");
}

// ── GET /agent/runs ──────────────────────────────

#[tokio::test]
async fn list_runs_paginates_clamps_and_isolates_workspace() {
    let (app, pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    for i in 1..=3 {
        insert_run(&pool, &format!("run-{i}"), WS, &format!("摘要{i}")).await;
    }
    insert_run(&pool, "run-other", "ws-other", "别区").await;

    // 默认分页：仅本工作区 3 条，最新在前
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/workspaces/{WS}/agent/runs"),
                &token,
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 0);
    let runs = json["result"]["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 3, "other workspace runs must not leak");
    assert_eq!(runs[0]["id"], "run-3", "latest first");
    assert_eq!(json["result"]["limit"], 50, "default limit 50");

    // limit clamp：500 → 200
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/workspaces/{WS}/agent/runs?limit=500"),
                &token,
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["result"]["limit"], 200, "limit clamped to max 200");

    // 分页窗口：limit=1&offset=1 → 第二新
    let (_status, json) = response_parts(
        app.oneshot(auth_request(
            "GET",
            &format!("/api/v1/workspaces/{WS}/agent/runs?limit=1&offset=1"),
            &token,
            None,
        ))
        .await
        .unwrap(),
    )
    .await;
    let runs = json["result"]["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], "run-2");
}

// ── POST /agent/runs/{id}/ack ──────────────────────────────

#[tokio::test]
async fn ack_run_is_idempotent_and_workspace_scoped() {
    let (app, pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");
    insert_run(&pool, "run-1", WS, "s").await;
    insert_run(&pool, "run-foreign", "ws-other", "s").await;

    // 首认
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/workspaces/{WS}/agent/runs/run-1/ack"),
                &token,
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 0, "first ack: {json}");
    assert_eq!(json["result"]["firstAck"], true);

    // 重复确认：幂等 200，firstAck=false
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "POST",
                &format!("/api/v1/workspaces/{WS}/agent/runs/run-1/ack"),
                &token,
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 0, "repeat ack stays 200 (idempotent): {json}");
    assert_eq!(json["result"]["firstAck"], false);

    let (acked_by,): (String,) = sqlx::query_as("SELECT acked_by FROM agent_runs WHERE id = 'run-1'")
        .fetch_one(&pool)
        .await
        .expect("acked_by");
    assert_eq!(acked_by, "user-1");

    // 不存在 / 其他工作区的 run → 404（不泄露存在性）
    for run_id in ["run-missing", "run-foreign"] {
        let (_status, json) = response_parts(
            app.clone()
                .oneshot(auth_request(
                    "POST",
                    &format!("/api/v1/workspaces/{WS}/agent/runs/{run_id}/ack"),
                    &token,
                    None,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(json["code"], 404, "ack {run_id} must be 404: {json}");
    }
}

// ── GET/PUT /agent/policy ──────────────────────────────

#[tokio::test]
async fn policy_round_trip_with_updated_by() {
    let (app, pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    // 默认策略（无行）：mode=off
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/workspaces/{WS}/agent/policy"),
                &token,
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["mode"], "off");

    // PUT 三态策略
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "PUT",
                &format!("/api/v1/workspaces/{WS}/agent/policy"),
                &token,
                Some(json!({
                    "mode": "act",
                    "allowedActions": ["*"],
                    "deniedActions": ["wipe_device"],
                    "maxActionsPerRun": 5,
                    "maxActionsPerHour": 10
                })),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 0, "PUT policy: {json}");

    // GET 读回
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "GET",
                &format!("/api/v1/workspaces/{WS}/agent/policy"),
                &token,
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["result"]["mode"], "act");
    assert_eq!(json["result"]["deniedActions"], json!(["wipe_device"]));
    assert_eq!(json["result"]["maxActionsPerRun"], 5);
    assert_eq!(json["result"]["maxActionsPerHour"], 10);

    // updated_by 记录操作者
    let (updated_by,): (String,) =
        sqlx::query_as("SELECT updated_by FROM workspace_autonomy_policy WHERE workspace_id = ?")
            .bind(WS)
            .fetch_one(&pool)
            .await
            .expect("updated_by");
    assert_eq!(updated_by, "user-1");

    // 非法 mode → 400
    let (_status, json) = response_parts(
        app.oneshot(auth_request(
            "PUT",
            &format!("/api/v1/workspaces/{WS}/agent/policy"),
            &token,
            Some(json!({"mode": "yolo"})),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 400, "invalid mode must be 400: {json}");
}

// O26 kill switch：PUT policy mode→off 必须 drain 该工作区待处理队列；
// 其他 mode 不触发。
#[tokio::test]
async fn policy_mode_to_off_drains_pending_queue() {
    async fn put_policy(app: &Router, token: &str, mode: &str) -> Value {
        let (_status, json) = response_parts(
            app.clone()
                .oneshot(auth_request(
                    "PUT",
                    &format!("/api/v1/workspaces/{WS}/agent/policy"),
                    token,
                    Some(json!({"mode": mode, "allowedActions": ["*"]})),
                ))
                .await
                .unwrap(),
        )
        .await;
        json
    }

    let stub = Arc::new(StubDirectiveSink::default());
    let (app, _pool) = app_with_sink(stub.clone()).await;
    let token = create_test_token("user-1", "tenant-1");

    // act / diagnose：不 drain。
    let json = put_policy(&app, &token, "act").await;
    assert_eq!(json["code"], 0, "PUT act: {json}");
    let json = put_policy(&app, &token, "diagnose").await;
    assert_eq!(json["code"], 0, "PUT diagnose: {json}");
    assert!(stub.drained().is_empty(), "non-off modes must not drain");

    // off：drain 一次，目标为本工作区。
    let json = put_policy(&app, &token, "off").await;
    assert_eq!(json["code"], 0, "PUT off: {json}");
    assert_eq!(
        stub.drained(),
        vec![WS.to_string()],
        "mode→off must drain the workspace queue"
    );
}

/// F9 覆盖补钉：find_agent_run_owner 数据库故障 → 500 + 错误日志，
/// 不得塌缩为 404（事故响应时"运行记录不存在"会主动误导）。
#[tokio::test]
async fn ack_run_db_error_returns_500_not_fake_404() {
    let (app, pool) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    // user-1 已由 app_with_sink 夹具授予 admin。
    let token = create_test_token("user-1", "tenant-1");

    // 故障注入：删掉 agent_runs 表，所有 owner 查询必失败。
    sqlx::query("ALTER TABLE agent_runs RENAME TO agent_runs_hidden")
        .execute(&pool)
        .await
        .expect("hide agent_runs");

    let response = app
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/workspaces/{WS}/agent/runs/run-whatever/ack"),
            &token,
            None,
        ))
        .await
        .expect("ack response");
    let (status, json) = response_parts(response).await;
    assert!(status.is_success(), "envelope returns HTTP 200: {json}");
    assert_eq!(
        json["code"], 500,
        "DB failure must surface as code 500, not a misleading 404: {json}"
    );
}

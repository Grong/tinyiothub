//! Agent tasks API integration tests (T14)
//!
//! 用户指令入口四端点：POST /agent/tasks（持久 Task，需 Idempotency-Key）、
//! GET /agent/runs、POST /agent/runs/{id}/ack、GET/PUT /agent/policy。
//! 全部 workspace 隔离 + admin 角色；ack 幂等。

use std::sync::Arc;

use crate::domains::agent::host::trigger::directive_sink::StubDirectiveSink;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
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
async fn app_with_sink(sink: Arc<StubDirectiveSink>) -> (Router, sqlx::SqlitePool, Arc<tinyiothub_storage::Db>) {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    app_state.set_directive_sink(sink);
    seed_test_workspace(&pool, "tenant-1", WS).await;
    grant_admin(&pool, "user-1").await;
    let db = app_state.db.clone();
    db.create_workspace_store(WS).await.expect("workspace store");
    let api_router = crate::api::create_router(&app_state);
    (Router::new().nest("/api", api_router).with_state(app_state), pool, db)
}

/// 无 sink（生产未接线状态）的路由。
async fn app_without_sink() -> (Router, sqlx::SqlitePool, Arc<tinyiothub_storage::Db>) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    grant_admin(&pool, "user-1").await;
    let db = app_state.db.clone();
    db.create_workspace_store(WS).await.expect("workspace store");
    let api_router = crate::api::create_router(&app_state);
    (Router::new().nest("/api", api_router).with_state(app_state), pool, db)
}

fn create_task_request(token: &str, body: Value, key: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workspaces/{WS}/agent/tasks"))
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        builder = builder.header("Idempotency-Key", key);
    }
    builder.body(Body::from(body.to_string())).unwrap()
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
async fn create_task_creates_persistent_task_without_directive() {
    let stub = Arc::new(StubDirectiveSink::default());
    let (app, _pool, db) = app_with_sink(stub.clone()).await;
    let token = create_test_token("user-1", "tenant-1");

    let (status, json) = response_parts(
        app.oneshot(create_task_request(
            &token,
            json!({"text": "  把 3 号产线温度调到 25 度 "}),
            Some("t14-key"),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["code"], 0, "persistent create must succeed: {json}");
    let task_id = json["result"]["taskId"].as_str().expect("task id");
    assert!(!task_id.is_empty());
    assert!(
        stub.signals().is_empty(),
        "persistent create must not dispatch a UserDirective"
    );

    let workspace_pool = db.workspace_pool_for_testing(WS).await.expect("workspace pool");
    let objective: String = sqlx::query_scalar("SELECT runtime_objective FROM agent_tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("persisted task");
    assert_eq!(
        objective, "把 3 号产线温度调到 25 度",
        "text trimmed before persistence"
    );
}

#[tokio::test]
async fn create_task_requires_idempotency_key() {
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = response_parts(
        app.oneshot(create_task_request(&token, json!({"text": "重启网关"}), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 400, "missing Idempotency-Key must be 400: {json}");
    let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_tasks")
        .fetch_one(&pool)
        .await
        .expect("task count");
    assert_eq!(tasks, 0);
}

#[tokio::test]
async fn create_task_empty_text_400() {
    let (app, _pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = response_parts(
        app.oneshot(create_task_request(
            &token,
            json!({"text": "   "}),
            Some("blank-text-key"),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 400);
}

#[tokio::test]
async fn create_task_works_without_directive_sink() {
    let (app, _pool, _db) = app_without_sink().await;
    let token = create_test_token("user-1", "tenant-1");

    let (_status, json) = response_parts(
        app.oneshot(create_task_request(
            &token,
            json!({"text": "重启网关"}),
            Some("no-sink-key"),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(
        json["code"], 0,
        "persistent create must not depend on the directive sink: {json}"
    );
}

#[tokio::test]
async fn endpoints_require_admin_role() {
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
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
            Some(json!({"permissionMode": "ask"})),
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
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
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
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
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
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
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
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
    let token = create_test_token("user-1", "tenant-1");

    // 默认策略（无行）：没有策略行时 gate 拒绝写动作 → read_only
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
    assert_eq!(json["result"]["permissionMode"], "read_only");

    // PUT 权限策略
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "PUT",
                &format!("/api/v1/workspaces/{WS}/agent/policy"),
                &token,
                Some(json!({
                    "permissionMode": "ask",
                    "allowedActions": ["*"],
                    "deniedActions": ["wipe_thing"],
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
    assert_eq!(json["result"]["permissionMode"], "ask");
    assert_eq!(json["result"]["deniedActions"], json!(["wipe_thing"]));
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

    // 非法 permissionMode → 400
    let (_status, json) = response_parts(
        app.clone()
            .oneshot(auth_request(
                "PUT",
                &format!("/api/v1/workspaces/{WS}/agent/policy"),
                &token,
                Some(json!({"permissionMode": "yolo"})),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 400, "invalid permissionMode must be 400: {json}");

    // full_access 仅会话级：工作区策略必须拒绝。
    let (_status, json) = response_parts(
        app.oneshot(auth_request(
            "PUT",
            &format!("/api/v1/workspaces/{WS}/agent/policy"),
            &token,
            Some(json!({"permissionMode": "full_access"})),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(json["code"], 400, "workspace full_access must be 400: {json}");
}

/// F9 覆盖补钉：find_agent_run_owner 数据库故障 → 500 + 错误日志，
/// 不得塌缩为 404（事故响应时"运行记录不存在"会主动误导）。
#[tokio::test]
async fn ack_run_db_error_returns_500_not_fake_404() {
    let (app, pool, _db) = app_with_sink(Arc::new(StubDirectiveSink::default())).await;
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

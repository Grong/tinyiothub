use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tinyiothub_core::agent_task::TaskStatus;
use tinyiothub_storage::Db;
use tower::ServiceExt;

use crate::domains::agent::host::trigger::directive_sink::StubDirectiveSink;
use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-runtime-t1";

struct RuntimeFixture {
    app: Router,
    db: Arc<Db>,
    legacy_sink: Arc<StubDirectiveSink>,
    token: String,
}

async fn runtime_test_app(tasks_enabled: bool) -> RuntimeFixture {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    let legacy_sink = Arc::new(StubDirectiveSink::default());
    app_state.set_directive_sink(legacy_sink.clone());
    seed_test_workspace(&pool, "tenant-1", WS).await;
    sqlx::query(
        "INSERT OR IGNORE INTO roles (id, name, description, is_administrator) VALUES ('role-admin', 'admin', 'admin', 1)",
    )
    .execute(&pool)
    .await
    .expect("seed admin role");
    sqlx::query(
        "INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES ('ur-runtime-admin', 'user-1', 'role-admin')",
    )
    .execute(&pool)
    .await
    .expect("grant admin");
    set_task_flag(&pool, tasks_enabled).await;
    let api_router = crate::api::create_router(&app_state);
    let app = Router::new().nest("/api", api_router).with_state(app_state);
    let db = Arc::new(Db::new(pool));
    RuntimeFixture {
        app,
        db,
        legacy_sink,
        token: create_test_token("user-1", "tenant-1"),
    }
}

async fn set_task_flag(pool: &sqlx::SqlitePool, enabled: bool) {
    sqlx::query("UPDATE workspaces SET heartbeat_config = ? WHERE id = ?")
        .bind(json!({"runtime_tasks_enabled": enabled}).to_string())
        .bind(WS)
        .execute(pool)
        .await
        .expect("set runtime task flag");
}

async fn post_task(app: &Router, token: &str, text: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workspaces/{WS}/agent/tasks"))
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        builder = builder.header("Idempotency-Key", key);
    }
    let request = builder
        .body(Body::from(json!({"text": text}).to_string()))
        .expect("request");
    let (status, body) = response_parts(app.clone().oneshot(request).await.expect("response")).await;
    (status, body)
}

async fn get_json(app: &Router, token: &str, uri: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .body(Body::empty())
        .expect("request");
    let (_, body) = response_parts(app.clone().oneshot(request).await.expect("response")).await;
    body
}

async fn count_rows(pool: &sqlx::SqlitePool, table: &str) -> i64 {
    let query = match table {
        "agent_tasks" => "SELECT COUNT(*) FROM agent_tasks",
        "task_wake_outbox" => "SELECT COUNT(*) FROM task_wake_outbox",
        _ => panic!("unsupported test table"),
    };
    sqlx::query_scalar(query).fetch_one(pool).await.expect("row count")
}

async fn create_persistent(fixture: &RuntimeFixture, text: &str, key: &str) -> Value {
    let (_, body) = post_task(&fixture.app, &fixture.token, text, Some(key)).await;
    assert_eq!(body["code"], 0, "persistent create failed: {body}");
    body
}

#[test]
fn db_task_store_implements_task_store_port() {
    fn assert_task_store<T: tinyiothub_agent::port::task::TaskStore>() {}
    assert_task_store::<crate::domains::agent::runtime::DbTaskStore>();
}

#[tokio::test]
async fn persistent_create_requires_idempotency_key() {
    let fixture = runtime_test_app(true).await;
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect pump", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], 400);
    assert_eq!(count_rows(fixture.db.pool(), "agent_tasks").await, 0);
    assert!(fixture.legacy_sink.signals().is_empty());
}

#[tokio::test]
async fn persistent_create_rejects_blank_key_and_objective_without_writes() {
    let fixture = runtime_test_app(true).await;
    for (text, key) in [("inspect", Some("  ")), ("  ", Some("key"))] {
        let (status, body) = post_task(&fixture.app, &fixture.token, text, key).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "text={text:?}, key={key:?}: {body}");
        assert_eq!(body["code"], 400, "text={text:?}, key={key:?}: {body}");
    }
    assert_eq!(count_rows(fixture.db.pool(), "agent_tasks").await, 0);
    assert!(fixture.legacy_sink.signals().is_empty());
}

#[tokio::test]
async fn persistent_create_replays_task_and_wake_without_provider_call() {
    let fixture = runtime_test_app(true).await;
    let first = create_persistent(&fixture, "  inspect both pumps  ", "key-1").await;
    let second = create_persistent(&fixture, "inspect both pumps", "key-1").await;
    assert_eq!(first["result"]["deduplicated"], false);
    assert_eq!(second["result"]["deduplicated"], true);
    assert_eq!(first["result"]["taskId"], second["result"]["taskId"]);
    assert_eq!(second["result"]["status"], "accepted");
    assert_eq!(count_rows(fixture.db.pool(), "agent_tasks").await, 1);
    assert_eq!(count_rows(fixture.db.pool(), "task_wake_outbox").await, 1);
    assert!(fixture.legacy_sink.signals().is_empty());

    let task_id = first["result"]["taskId"].as_str().expect("task id");
    let stored = fixture.db.get_task(WS, task_id).await.expect("get task").expect("task");
    assert_eq!(stored.objective, "inspect both pumps");
    assert_eq!(stored.created_by, "user-1");
    assert_eq!(stored.status, TaskStatus::Active);
}

#[tokio::test]
async fn persistent_create_same_key_different_objective_returns_409_without_new_wake() {
    let fixture = runtime_test_app(true).await;
    create_persistent(&fixture, "inspect A", "key-1").await;
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect B", Some("key-1")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], 409);
    assert_eq!(count_rows(fixture.db.pool(), "agent_tasks").await, 1);
    assert_eq!(count_rows(fixture.db.pool(), "task_wake_outbox").await, 1);
    assert!(fixture.legacy_sink.signals().is_empty());
}

#[tokio::test]
async fn task_flag_off_preserves_legacy_directive_and_existing_task() {
    let fixture = runtime_test_app(true).await;
    let created = create_persistent(&fixture, "inspect", "existing-key").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    fixture
        .db
        .transition_task(WS, task_id, 0, TaskStatus::WaitingJob, None)
        .await
        .expect("move existing task to waiting_job");
    set_task_flag(fixture.db.pool(), false).await;

    let (_, body) = post_task(&fixture.app, &fixture.token, "legacy", None).await;
    assert_eq!(body["code"], 0);
    assert_eq!(fixture.legacy_sink.signals().len(), 1);
    let stored = fixture.db.get_task(WS, task_id).await.expect("get").expect("task");
    assert_eq!(stored.status, TaskStatus::WaitingJob);
    assert_eq!(stored.state_version, 1);
    assert_eq!(count_rows(fixture.db.pool(), "agent_tasks").await, 1);
    assert_eq!(count_rows(fixture.db.pool(), "task_wake_outbox").await, 1);
}

#[tokio::test]
async fn task_list_and_detail_return_persisted_projections_with_read_access() {
    let fixture = runtime_test_app(true).await;
    let created = create_persistent(&fixture, "inspect", "key-1").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    let reader = create_test_token("user-2", "tenant-1");

    let list = get_json(&fixture.app, &reader, &format!("/api/v1/workspaces/{WS}/agent/tasks")).await;
    assert_eq!(list["code"], 0, "list: {list}");
    assert_eq!(list["result"]["tasks"].as_array().expect("tasks").len(), 1);
    assert_eq!(list["result"]["tasks"][0]["objective"], "inspect");
    let detail = get_json(
        &fixture.app,
        &reader,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}"),
    )
    .await;
    assert_eq!(detail["code"], 0, "detail: {detail}");
    assert_eq!(detail["result"]["task"]["id"], task_id);
    assert_eq!(detail["result"]["task"]["status"], "active");
}

#[tokio::test]
async fn persistent_create_storage_failure_returns_503() {
    let fixture = runtime_test_app(true).await;
    sqlx::query("ALTER TABLE agent_tasks RENAME TO agent_tasks_hidden")
        .execute(fixture.db.pool())
        .await
        .expect("hide task table");
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect", Some("key-1")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], 503);
    assert!(fixture.legacy_sink.signals().is_empty());
}

#[tokio::test]
async fn persistent_create_flag_load_failure_returns_503() {
    let fixture = runtime_test_app(true).await;
    sqlx::query("ALTER TABLE workspaces DROP COLUMN heartbeat_config")
        .execute(fixture.db.pool())
        .await
        .expect("remove heartbeat config column");
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect", Some("key-1")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], 503);
    assert!(fixture.legacy_sink.signals().is_empty());
    assert_eq!(count_rows(fixture.db.pool(), "agent_tasks").await, 0);
}

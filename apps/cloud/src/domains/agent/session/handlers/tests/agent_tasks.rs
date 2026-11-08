use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use serde_json::{Value, json};
use tinyiothub_core::agent_task::TaskStatus;
use tinyiothub_storage::Db;
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_test_workspace, setup_test_app_with_pool,
};

const WS: &str = "ws-runtime-t1";

struct RuntimeFixture {
    app: Router,
    db: Arc<Db>,
    token: String,
}

async fn runtime_test_app() -> RuntimeFixture {
    let (app_state, pool) = setup_test_app_with_pool().await;
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
    let db = app_state.db.clone();
    db.create_workspace_store(WS).await.expect("workspace store");
    let api_router = crate::api::create_router(&app_state);
    let app = Router::new().nest("/api", api_router).with_state(app_state);
    RuntimeFixture {
        app,
        db,
        token: create_test_token("user-1", "tenant-1"),
    }
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

async fn count_rows(db: &Db, table: &str) -> i64 {
    let pool = db.workspace_pool_for_testing(WS).await.expect("workspace pool");
    let query = match table {
        "agent_tasks" => "SELECT COUNT(*) FROM agent_tasks WHERE execution_mode = 'production'",
        "task_wake_outbox" => "SELECT COUNT(*) FROM task_wake_outbox",
        "device_jobs" => "SELECT COUNT(*) FROM device_jobs",
        "decisions" => "SELECT COUNT(*) FROM decisions",
        _ => panic!("unsupported test table"),
    };
    sqlx::query_scalar(query).fetch_one(&pool).await.expect("row count")
}

async fn session_events(db: &Db, task_id: &str) -> Vec<(String, String, Value)> {
    let pool = db.workspace_pool_for_testing(WS).await.expect("workspace pool");
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT event_id, type, payload_json FROM session_events
         WHERE task_id = ? AND execution_mode = 'production' ORDER BY seq",
    )
    .bind(task_id)
    .fetch_all(&pool)
    .await
    .expect("session events")
    .into_iter()
    .map(|(id, event_type, payload)| (id, event_type, serde_json::from_str(&payload).expect("payload")))
    .collect()
}

async fn create_persistent(fixture: &RuntimeFixture, text: &str, key: &str) -> Value {
    let (_, body) = post_task(&fixture.app, &fixture.token, text, Some(key)).await;
    assert_eq!(body["code"], 0, "persistent create failed: {body}");
    body
}

async fn seed_running_job(db: &Db, task_id: &str, job_id: &str) {
    let pool = db.workspace_pool_for_testing(WS).await.expect("workspace pool");
    sqlx::query(
        "INSERT INTO device_jobs
            (id, workspace_id, device_id, tool, args_json, state, stop_requested_at, progress_json,
             result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at)
         VALUES (?, ?, 'device-1', 'inspect', '{}', 'running', NULL, NULL, NULL, ?, NULL, NULL,
                 NULL, 5000, ?, NULL)",
    )
    .bind(job_id)
    .bind(WS)
    .bind(task_id)
    .bind(Utc::now())
    .execute(&pool)
    .await
    .expect("job");
}

#[test]
fn harness_task_store_implements_task_store_port() {
    fn assert_task_store<T: tinyiothub_agent::port::task::TaskStore>() {}
    assert_task_store::<crate::domains::agent::runtime::harness_store::HarnessTaskStore>();
}

#[tokio::test]
async fn persistent_create_requires_idempotency_key() {
    let fixture = runtime_test_app().await;
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect pump", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], 400);
    assert_eq!(count_rows(&fixture.db, "agent_tasks").await, 0);
}

#[tokio::test]
async fn persistent_create_rejects_blank_key_and_objective_without_writes() {
    let fixture = runtime_test_app().await;
    for (text, key) in [("inspect", Some("  ")), ("  ", Some("key"))] {
        let (status, body) = post_task(&fixture.app, &fixture.token, text, key).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "text={text:?}, key={key:?}: {body}");
        assert_eq!(body["code"], 400, "text={text:?}, key={key:?}: {body}");
    }
    assert_eq!(count_rows(&fixture.db, "agent_tasks").await, 0);
}

#[tokio::test]
async fn persistent_create_replays_task_and_wake_without_provider_call() {
    let fixture = runtime_test_app().await;
    let first = create_persistent(&fixture, "  inspect both pumps  ", "key-1").await;
    let second = create_persistent(&fixture, "inspect both pumps", "key-1").await;
    assert_eq!(first["result"]["deduplicated"], false);
    assert_eq!(second["result"]["deduplicated"], true);
    assert_eq!(first["result"]["taskId"], second["result"]["taskId"]);
    assert_eq!(second["result"]["status"], "accepted");
    assert_eq!(count_rows(&fixture.db, "agent_tasks").await, 1);
    assert_eq!(count_rows(&fixture.db, "task_wake_outbox").await, 1);

    let task_id = first["result"]["taskId"].as_str().expect("task id");
    let stored = fixture
        .db
        .harness_get_task(WS, task_id)
        .await
        .expect("get task")
        .expect("task");
    assert_eq!(stored.objective, "inspect both pumps");
    assert_eq!(stored.created_by, "user-1");
    assert_eq!(stored.status, TaskStatus::Active);
}

#[tokio::test]
async fn persistent_create_records_one_stable_harness_event() {
    let fixture = runtime_test_app().await;
    let first = create_persistent(&fixture, "inspect", "event-key").await;
    let second = create_persistent(&fixture, "inspect", "event-key").await;
    assert_eq!(second["result"]["deduplicated"], true);
    let task_id = first["result"]["taskId"].as_str().expect("task id");
    let events = session_events(&fixture.db, task_id).await;
    assert_eq!(events.len(), 1, "create replay must not append a duplicate event");
    assert_eq!(events[0].0, format!("task:{task_id}:created"));
    assert_eq!(events[0].1, "task.created");
    assert_eq!(events[0].2["stateVersion"], 0);
}

#[tokio::test]
async fn task_store_records_versioned_transition_events() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "transition-event").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    let store = crate::domains::agent::runtime::harness_store::HarnessTaskStore::new(fixture.db.clone());
    let paused =
        tinyiothub_agent::port::task::TaskStore::transition_task(&store, WS, task_id, 0, TaskStatus::Paused, None)
            .await
            .expect("pause");
    assert_eq!(paused.state_version, 1);
    let events = session_events(&fixture.db, task_id).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].0, format!("task:{task_id}:v1:paused"));
    assert_eq!(events[1].1, "task.paused");
    assert_eq!(events[1].2["stateVersion"], 1);
}

#[tokio::test]
async fn task_store_records_current_run_binding() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "bind-event").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    let store = crate::domains::agent::runtime::harness_store::HarnessTaskStore::new(fixture.db.clone());
    let bound = tinyiothub_agent::port::task::TaskStore::bind_current_run(&store, task_id, 0, Some("run-bound"))
        .await
        .expect("bind run");
    assert_eq!(bound.state_version, 1);
    let events = session_events(&fixture.db, task_id).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].0, format!("task:{task_id}:v1:run_bound"));
    assert_eq!(events[1].1, "task.run_bound");
    assert_eq!(events[1].2["stateVersion"], 1);
}

#[tokio::test]
async fn task_store_records_run_start_and_finish_events() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "run-event").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    let task = fixture
        .db
        .harness_get_task(WS, task_id)
        .await
        .expect("task")
        .expect("present");
    let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("pool");
    let wake_id = sqlx::query_scalar::<_, String>("SELECT event_id FROM task_wake_outbox WHERE task_id = ?")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("wake id");
    let store = crate::domains::agent::runtime::harness_store::HarnessTaskStore::new(fixture.db.clone());
    assert!(
        tinyiothub_agent::port::task::TaskStore::begin_run(&store, &task, &wake_id, "run-event-1")
            .await
            .expect("begin run")
    );
    tinyiothub_agent::port::task::TaskStore::finish_run(
        &store,
        &tinyiothub_core::agent_runs::RunReport {
            run_id: "run-event-1".into(),
            workspace_id: WS.into(),
            trigger: format!("task:{wake_id}"),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "healthy".into(),
            actions: vec![],
            verified: true,
            duration_ms: 1,
            tool_calls: 0,
            tokens: 0,
            end_reason: None,
            thing_id: None,
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: Some(task_id.into()),
            wake_event_id: Some(wake_id),
        },
    )
    .await
    .expect("finish run");
    let events = session_events(&fixture.db, task_id).await;
    assert_eq!(events.len(), 3);
    assert_eq!(events[1].0, "run:run-event-1:started");
    assert_eq!(events[1].1, "run.started");
    assert_eq!(events[2].0, "run:run-event-1:finished");
    assert_eq!(events[2].1, "run.finished");
}

#[tokio::test]
async fn persistent_create_same_key_different_objective_returns_409_without_new_wake() {
    let fixture = runtime_test_app().await;
    create_persistent(&fixture, "inspect A", "key-1").await;
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect B", Some("key-1")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], 409);
    assert_eq!(count_rows(&fixture.db, "agent_tasks").await, 1);
    assert_eq!(count_rows(&fixture.db, "task_wake_outbox").await, 1);
}

#[tokio::test]
async fn persistent_create_leaves_existing_task_state_untouched() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "existing-key").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    fixture
        .db
        .harness_transition_task(WS, task_id, 0, TaskStatus::WaitingJob, None)
        .await
        .expect("move existing task to waiting_job");
    fixture
        .db
        .harness_bind_current_run(task_id, 1, Some("run-existing"))
        .await
        .expect("bind existing run");
    seed_running_job(&fixture.db, task_id, "job-existing").await;
    let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("pool");
    sqlx::query(
        "INSERT INTO decisions
            (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
             options_json, created_at, updated_at, expires_at, answer, answer_actor,
             answer_event_id, answer_evidence_json, consumed_at, consumed_by)
         VALUES ('approval-existing', 'tool_approval', 'requested', 'session-1', 'run-existing',
                 ?, NULL, '{}', '[]', ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(task_id)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&pool)
    .await
    .expect("existing approval");

    let created = create_persistent(&fixture, "created-after", "later-key").await;
    assert_eq!(created["result"]["status"], "accepted");
    let stored = fixture
        .db
        .harness_get_task(WS, task_id)
        .await
        .expect("get")
        .expect("task");
    assert_eq!(stored.status, TaskStatus::WaitingJob);
    assert_eq!(stored.state_version, 2);
    assert_eq!(stored.current_run_id.as_deref(), Some("run-existing"));
    assert_eq!(count_rows(&fixture.db, "agent_tasks").await, 2);
    assert_eq!(count_rows(&fixture.db, "task_wake_outbox").await, 2);
    assert_eq!(count_rows(&fixture.db, "device_jobs").await, 1);
    assert_eq!(count_rows(&fixture.db, "decisions").await, 1);
    let job = fixture
        .db
        .harness_task_job(WS, task_id, "job-existing")
        .await
        .expect("get job")
        .expect("job");
    assert_eq!(job.job.state, tinyiothub_core::device_job::DeviceJobState::Running);
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_wake_outbox
         WHERE task_id = ? AND dispatch_state = 'pending' AND resolved_at IS NULL",
    )
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .expect("pending wake");
    assert_eq!(pending, 1);
}

#[tokio::test]
async fn task_list_and_detail_return_persisted_projections_with_read_access() {
    let fixture = runtime_test_app().await;
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
async fn persistent_task_control_routes_reach_the_harness_runtime() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "control-key").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    for (action, expected) in [("pause", "paused"), ("resume", "active"), ("cancel", "cancelled")] {
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/{action}"))
            .header("Authorization", auth_header(&fixture.token))
            .body(Body::empty())
            .expect("request");
        let (status, body) = response_parts(fixture.app.clone().oneshot(request).await.expect("response")).await;
        assert_eq!(status, StatusCode::OK, "{action}: {body}");
        assert_eq!(body["result"]["task"]["status"], expected, "{action}: {body}");
    }
    // Task 19C4: create/transition/cancel are workspace Harness facts and must
    // never write the retired platform Task tables or the legacy runtime.v1
    // Task ledger.
    let platform_tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_tasks")
        .fetch_one(fixture.db.pool())
        .await
        .expect("platform task count");
    assert_eq!(platform_tasks, 0, "platform agent_tasks must stay empty");
    let legacy = fixture
        .db
        .runtime_events_after(WS, None, Some(task_id), 50)
        .await
        .expect("legacy ledger");
    assert!(legacy.is_empty(), "no runtime.v1 Task rows may be produced: {legacy:?}");
}

#[tokio::test]
async fn runtime_controls_use_http_auth_statuses_and_expose_scoped_job_stop() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "control-auth").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    seed_running_job(&fixture.db, task_id, "job-1").await;
    let reader = create_test_token("user-2", "tenant-1");
    let pause = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/pause"))
        .header("Authorization", auth_header(&reader))
        .body(Body::empty())
        .expect("request");
    let (status, body) = response_parts(fixture.app.clone().oneshot(pause).await.expect("response")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "reader pause: {body}");
    assert_eq!(body["code"], 403);

    let stop = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}/jobs/job-1/stop"))
        .header("Authorization", auth_header(&fixture.token))
        .body(Body::empty())
        .expect("request");
    let (status, body) = response_parts(fixture.app.clone().oneshot(stop).await.expect("response")).await;
    assert_eq!(status, StatusCode::OK, "scoped job stop: {body}");
}

#[tokio::test]
async fn task_detail_uses_the_single_runtime_projection_surface() {
    let fixture = runtime_test_app().await;
    let created = create_persistent(&fixture, "inspect", "projection").await;
    let task_id = created["result"]["taskId"].as_str().expect("task id");
    let detail = get_json(
        &fixture.app,
        &fixture.token,
        &format!("/api/v1/workspaces/{WS}/agent/tasks/{task_id}"),
    )
    .await;
    assert_eq!(detail["code"], 0, "detail: {detail}");
    assert_eq!(detail["result"]["task"]["id"], task_id);
    assert!(detail["result"]["runs"].is_array(), "runs: {detail}");
    assert!(detail["result"]["jobs"].is_array(), "jobs: {detail}");
    assert!(detail["result"]["approvals"].is_array(), "approvals: {detail}");
    assert!(detail["result"]["watches"].is_array(), "watches: {detail}");
    assert!(detail["result"]["evidence"].is_array(), "evidence: {detail}");
    assert!(detail["result"].get("events").is_none(), "events: {detail}");
    assert!(detail["result"]["cursor"].is_i64(), "cursor: {detail}");
}

#[tokio::test]
async fn persistent_create_storage_failure_returns_503() {
    let fixture = runtime_test_app().await;
    let pool = fixture.db.workspace_pool_for_testing(WS).await.expect("pool");
    sqlx::query("ALTER TABLE agent_tasks RENAME TO agent_tasks_hidden")
        .execute(&pool)
        .await
        .expect("hide task table");
    let (status, body) = post_task(&fixture.app, &fixture.token, "inspect", Some("key-1")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], 503);
}

use std::collections::{HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tinyiothub_storage::Db;
use tinyiothub_storage::runtime_event::{RuntimeEvent, RuntimeEventInput, TaskProjection};
use tinyiothub_web::security::Claims;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::{Receiver, Sender, channel};

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeFrame {
    Snapshot {
        snapshot: Box<TaskProjection>,
        cursor: i64,
    },
    Event {
        event: Box<RuntimeEvent>,
    },
    Resync {
        snapshot: Box<TaskProjection>,
        cursor: i64,
        reason: String,
    },
}

impl RuntimeFrame {
    fn name(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Event { .. } => "event",
            Self::Resync { .. } => "resync",
        }
    }
}

#[derive(Deserialize)]
pub struct StreamQuery {
    pub after: Option<i64>,
}

pub async fn stream_task(
    State(state): State<crate::domains::agent::AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
    Query(query): Query<StreamQuery>,
) -> Response {
    match state.workspace_access.workspace_tenant_id(&workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => {}
        Ok(Some(_)) => {
            return super::tasks::error_response(axum::http::StatusCode::FORBIDDEN, "无权访问此工作空间");
        }
        Ok(None) => {
            return super::tasks::error_response(axum::http::StatusCode::NOT_FOUND, "工作空间不存在");
        }
        Err(error) => {
            tracing::error!(%workspace_id, %error, "check runtime stream workspace failed");
            return super::tasks::error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败");
        }
    }
    if let Err(response) = super::projection::load(&state, &workspace_id, &task_id).await {
        return response;
    }
    let mut connection = match state
        .runtime_stream
        .connect(state.db.clone(), &workspace_id, &task_id, query.after)
        .await
    {
        Ok(connection) => connection,
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %error, "open runtime stream failed");
            return super::tasks::error_response(axum::http::StatusCode::SERVICE_UNAVAILABLE, "Task 事件流不可用");
        }
    };
    let stream = async_stream::stream! {
        while let Some(frame) = connection.next_frame().await {
            let data = match serde_json::to_string(&frame) {
                Ok(data) => data,
                Err(error) => {
                    tracing::error!(%error, "serialize runtime stream frame failed");
                    break;
                }
            };
            yield Ok::<_, Infallible>(Event::default().event(frame.name()).data(data));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

#[derive(Debug)]
pub struct RuntimeStreamError(String);

impl std::fmt::Display for RuntimeStreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RuntimeStreamError {}

#[derive(Clone)]
pub struct RuntimeRecorder {
    db: Arc<Db>,
    hub: Arc<RuntimeStreamHub>,
}

impl RuntimeRecorder {
    pub fn new(db: Arc<Db>, hub: Arc<RuntimeStreamHub>) -> Self {
        Self { db, hub }
    }

    pub async fn record(&self, input: RuntimeEventInput) -> Result<RuntimeEvent, RuntimeStreamError> {
        let event = self
            .db
            .append_runtime_event(input)
            .await
            .map_err(|error| RuntimeStreamError(error.to_string()))?;
        self.hub.publish(event.clone());
        Ok(event)
    }

    pub async fn publish_persisted(&self, workspace_id: &str, event_id: &str) {
        match self.db.runtime_event_by_id(workspace_id, event_id).await {
            Ok(Some(event)) => self.hub.publish(event),
            Ok(None) => tracing::error!(workspace_id, event_id, "persisted runtime event missing during publish"),
            Err(error) => tracing::error!(workspace_id, event_id, %error, "load persisted runtime event failed"),
        }
    }
}

pub struct RuntimeStreamHub {
    senders: DashMap<String, Sender<RuntimeEvent>>,
    connections: Arc<DashMap<String, usize>>,
    capacity: usize,
}

impl RuntimeStreamHub {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            senders: DashMap::new(),
            connections: Arc::new(DashMap::new()),
            capacity: capacity.max(1),
        }
    }

    fn sender(&self, workspace_id: &str) -> Sender<RuntimeEvent> {
        self.senders
            .entry(workspace_id.to_string())
            .or_insert_with(|| channel(self.capacity).0)
            .clone()
    }

    pub fn publish(&self, event: RuntimeEvent) {
        if let Some(sender) = self.senders.get(&event.workspace_id) {
            let _ = sender.send(event);
        }
    }

    pub fn active_connections(&self, workspace_id: &str) -> usize {
        self.connections.get(workspace_id).map_or(0, |count| *count)
    }

    pub async fn connect(
        self: &Arc<Self>,
        db: Arc<Db>,
        workspace_id: &str,
        task_id: &str,
        after: Option<i64>,
    ) -> Result<RuntimeConnection, RuntimeStreamError> {
        // Subscribe before reading the snapshot; live rows queued during replay
        // are deduplicated by their persisted cursor below.
        let events = self.sender(workspace_id).subscribe();
        let projection = db
            .runtime_projection(workspace_id, task_id)
            .await
            .map_err(|error| RuntimeStreamError(error.to_string()))?
            .ok_or_else(|| RuntimeStreamError("task not found".into()))?;
        let cursor = projection.cursor;
        let mut pending = VecDeque::new();
        let unknown = match after {
            Some(after) => !db
                .runtime_cursor_known(workspace_id, after)
                .await
                .map_err(|error| RuntimeStreamError(error.to_string()))?,
            None => false,
        };
        if unknown {
            pending.push_back(RuntimeFrame::Resync {
                snapshot: Box::new(projection),
                cursor,
                reason: "unknown_cursor".into(),
            });
        } else {
            pending.push_back(RuntimeFrame::Snapshot {
                snapshot: Box::new(projection),
                cursor,
            });
            if after.is_some() {
                let mut replay_after = after;
                loop {
                    let rows = db
                        .runtime_events_after(workspace_id, replay_after, Some(task_id), 256)
                        .await
                        .map_err(|error| RuntimeStreamError(error.to_string()))?;
                    if rows.is_empty() {
                        break;
                    }
                    let count = rows.len();
                    replay_after = rows.last().map(|event| event.cursor);
                    pending.extend(
                        rows.into_iter()
                            .map(|event| RuntimeFrame::Event { event: Box::new(event) }),
                    );
                    if count < 256 {
                        break;
                    }
                }
            }
        }
        let guard = ConnectionGuard::new(self.connections.clone(), workspace_id.to_string());
        Ok(RuntimeConnection {
            db,
            workspace_id: workspace_id.to_string(),
            task_id: task_id.to_string(),
            events,
            pending,
            last_cursor: cursor,
            seen: HashSet::new(),
            _guard: guard,
        })
    }
}

impl Default for RuntimeStreamHub {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

struct ConnectionGuard {
    connections: Arc<DashMap<String, usize>>,
    workspace_id: String,
}

impl ConnectionGuard {
    fn new(connections: Arc<DashMap<String, usize>>, workspace_id: String) -> Self {
        *connections.entry(workspace_id.clone()).or_insert(0) += 1;
        Self {
            connections,
            workspace_id,
        }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(mut count) = self.connections.get_mut(&self.workspace_id) {
            *count = count.saturating_sub(1);
        }
        self.connections.remove_if(&self.workspace_id, |_, count| *count == 0);
    }
}

pub struct RuntimeConnection {
    db: Arc<Db>,
    workspace_id: String,
    task_id: String,
    events: Receiver<RuntimeEvent>,
    pending: VecDeque<RuntimeFrame>,
    last_cursor: i64,
    seen: HashSet<String>,
    _guard: ConnectionGuard,
}

impl RuntimeConnection {
    pub async fn next_frame(&mut self) -> Option<RuntimeFrame> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                if let RuntimeFrame::Event { event } = &frame {
                    self.seen.insert(event.event_id.clone());
                }
                return Some(frame);
            }
            match self.events.recv().await {
                Ok(event) => {
                    if event.task_id.as_deref() != Some(self.task_id.as_str())
                        || event.cursor <= self.last_cursor
                        || self.seen.contains(&event.event_id)
                    {
                        continue;
                    }
                    if self.projection_mismatch(&event).await? {
                        let frame = self.resync("projection_mismatch").await?;
                        self.pending.push_back(frame);
                        continue;
                    }
                    self.last_cursor = event.cursor;
                    self.seen.insert(event.event_id.clone());
                    return Some(RuntimeFrame::Event { event: Box::new(event) });
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "runtime stream lagged; sending resync");
                    let frame = self.resync("lagged").await?;
                    self.pending.push_back(frame);
                }
                Err(RecvError::Closed) => return None,
            }
        }
    }

    async fn projection_mismatch(&self, event: &RuntimeEvent) -> Option<bool> {
        let Some(expected) = event.data.get("stateVersion").and_then(serde_json::Value::as_i64) else {
            return Some(false);
        };
        let projection = self
            .db
            .runtime_projection(&self.workspace_id, &self.task_id)
            .await
            .ok()??;
        Some(projection.task.state_version != expected)
    }

    async fn resync(&mut self, reason: &str) -> Option<RuntimeFrame> {
        let projection = self
            .db
            .runtime_projection(&self.workspace_id, &self.task_id)
            .await
            .ok()??;
        self.last_cursor = projection.cursor;
        self.seen.clear();
        Some(RuntimeFrame::Resync {
            cursor: projection.cursor,
            snapshot: Box::new(projection),
            reason: reason.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Utc;
    use serde_json::json;
    use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
    use tinyiothub_storage::Db;
    use tinyiothub_storage::runtime_event::RuntimeEventInput;

    use super::{RuntimeFrame, RuntimeRecorder, RuntimeStreamHub};

    async fn fixture(capacity: usize) -> (Arc<Db>, Arc<RuntimeStreamHub>) {
        let pool = tinyiothub_storage::test_helpers::test_pool().await;
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(&pool)
        .await
        .expect("plan");
        sqlx::query("INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
            .execute(&pool)
            .await
            .expect("tenant");
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-1','workspace','t1','now','now')")
            .execute(&pool)
            .await
            .expect("workspace");
        let db = Db::new(pool);
        let now = Utc::now();
        db.create_task_with_wake(
            &AgentTask {
                id: "task-1".into(),
                workspace_id: "ws-1".into(),
                idempotency_key: "key-1".into(),
                objective: "inspect".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 1,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &TaskWake {
                task_id: "task-1".into(),
                event_id: "create-1".into(),
                signal_json: json!({"workspace_id":"ws-1"}).to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            },
        )
        .await
        .expect("task");
        (Arc::new(db), Arc::new(RuntimeStreamHub::with_capacity(capacity)))
    }

    fn input(id: &str) -> RuntimeEventInput {
        RuntimeEventInput {
            event_id: id.into(),
            event_type: "task.updated".into(),
            occurred_at: Utc::now(),
            workspace_id: "ws-1".into(),
            task_id: Some("task-1".into()),
            run_id: None,
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: json!({"stateVersion":1}),
        }
    }

    #[tokio::test]
    async fn reconnect_replays_persisted_cursor_without_duplicate_or_lost_events() {
        let (db, hub) = fixture(16).await;
        let first = db.append_runtime_event(input("event-1")).await.expect("first");
        let second = db.append_runtime_event(input("event-2")).await.expect("second");
        let mut connection = hub
            .connect(db.clone(), "ws-1", "task-1", Some(first.cursor))
            .await
            .expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        let Some(RuntimeFrame::Event { event }) = connection.next_frame().await else {
            panic!("expected replayed event");
        };
        assert_eq!(event.event_id, "event-2");
        assert_eq!(event.cursor, second.cursor);
        hub.publish(second);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), connection.next_frame())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn lag_and_projection_mismatch_emit_fresh_snapshot_resync() {
        let (db, hub) = fixture(1).await;
        let mut connection = hub.connect(db.clone(), "ws-1", "task-1", None).await.expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        hub.publish(db.append_runtime_event(input("event-1")).await.expect("first"));
        hub.publish(db.append_runtime_event(input("event-2")).await.expect("second"));
        let Some(RuntimeFrame::Resync { snapshot, reason, .. }) = connection.next_frame().await else {
            panic!("lag must resync");
        };
        assert_eq!(reason, "lagged");
        assert_eq!(snapshot.task.id, "task-1");

        let mut connection = hub
            .connect(db.clone(), "ws-1", "task-1", None)
            .await
            .expect("fresh connection");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        let mut mismatched = db.append_runtime_event(input("event-3")).await.expect("third");
        mismatched.data["stateVersion"] = json!(99);
        hub.publish(mismatched);
        let Some(RuntimeFrame::Resync { reason, .. }) = connection.next_frame().await else {
            panic!("projection mismatch must resync");
        };
        assert_eq!(reason, "projection_mismatch");
    }

    #[tokio::test]
    async fn unknown_cursor_and_foreign_task_never_continue_with_partial_deltas() {
        let (db, hub) = fixture(16).await;
        let mut connection = hub
            .connect(db.clone(), "ws-1", "task-1", Some(9_999))
            .await
            .expect("connect");
        let Some(RuntimeFrame::Resync { reason, snapshot, .. }) = connection.next_frame().await else {
            panic!("unknown cursor must resync");
        };
        assert_eq!(reason, "unknown_cursor");
        assert_eq!(snapshot.task.id, "task-1");

        let mut foreign = input("foreign-task");
        foreign.task_id = Some("task-other".into());
        hub.publish(db.append_runtime_event(foreign).await.expect("foreign"));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), connection.next_frame())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn reconnect_replay_preserves_commit_order_when_recorders_finish_in_reverse() {
        let (db, hub) = fixture(16).await;
        let created_cursor = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task")
            .cursor;
        let first = db
            .transition_task("ws-1", "task-1", 1, TaskStatus::Paused, None)
            .await
            .expect("commit version two");
        let second = db
            .transition_task("ws-1", "task-1", 2, TaskStatus::Active, None)
            .await
            .expect("commit version three");
        let recorder = RuntimeRecorder::new(db.clone(), hub.clone());
        let mut second_input = input("task:task-1:v3:active");
        second_input.event_type = "task.active".into();
        second_input.data = json!({"stateVersion":second.state_version});
        recorder.record(second_input).await.expect("record newer version first");
        let mut first_input = input("task:task-1:v2:paused");
        first_input.event_type = "task.paused".into();
        first_input.data = json!({"stateVersion":first.state_version});
        recorder.record(first_input).await.expect("record older version second");

        let mut connection = hub
            .connect(db, "ws-1", "task-1", Some(created_cursor))
            .await
            .expect("reconnect");
        let Some(RuntimeFrame::Snapshot { snapshot, .. }) = connection.next_frame().await else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.task.state_version, 3);
        let mut replayed_versions = Vec::new();
        for _ in 0..2 {
            let Some(RuntimeFrame::Event { event }) = connection.next_frame().await else {
                panic!("expected two committed versions");
            };
            replayed_versions.push(event.data["stateVersion"].as_i64());
        }
        assert_eq!(replayed_versions, [Some(2), Some(3)]);
    }

    #[tokio::test]
    async fn fresh_connection_retries_projection_when_task_commits_during_snapshot() {
        let (db, hub) = fixture(16).await;
        let probe = Arc::new(tinyiothub_storage::runtime_event::RuntimeProjectionProbe::default());
        let db = Arc::new(db.with_runtime_projection_probe(probe.clone()));
        probe.arm();
        let connecting = tokio::spawn({
            let db = db.clone();
            let hub = hub.clone();
            async move { hub.connect(db, "ws-1", "task-1", None).await }
        });
        probe.wait_for_task_read().await;
        let paused = db
            .transition_task("ws-1", "task-1", 1, TaskStatus::Paused, None)
            .await
            .expect("commit next Task version");
        let mut correction_input = input("task:task-1:v2:paused");
        correction_input.event_type = "task.paused".into();
        correction_input.data = json!({"stateVersion":paused.state_version});
        let correction = db
            .append_runtime_event(correction_input)
            .await
            .expect("persist correction");
        hub.publish(correction.clone());
        probe.release();
        let mut connection = connecting.await.expect("join connection").expect("connect");
        let Some(RuntimeFrame::Snapshot { snapshot, cursor }) = connection.next_frame().await else {
            panic!("expected initial snapshot");
        };
        assert_eq!(snapshot.task.state_version, paused.state_version);
        assert_eq!(snapshot.task.status, TaskStatus::Paused);
        assert_eq!(cursor, correction.cursor);
    }

    #[tokio::test]
    async fn resync_retries_projection_when_task_commits_during_resync() {
        let (db, hub) = fixture(1).await;
        let probe = Arc::new(tinyiothub_storage::runtime_event::RuntimeProjectionProbe::default());
        let db = Arc::new(db.with_runtime_projection_probe(probe.clone()));
        let mut connection = hub.connect(db.clone(), "ws-1", "task-1", None).await.expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        probe.arm();
        hub.publish(db.append_runtime_event(input("lag-1")).await.expect("first lag event"));
        hub.publish(db.append_runtime_event(input("lag-2")).await.expect("second lag event"));
        let resyncing = tokio::spawn(async move {
            let frame = connection.next_frame().await;
            (connection, frame)
        });
        probe.wait_for_task_read().await;
        let paused = db
            .transition_task("ws-1", "task-1", 1, TaskStatus::Paused, None)
            .await
            .expect("commit next Task version");
        let mut correction_input = input("task:task-1:v2:paused");
        correction_input.event_type = "task.paused".into();
        correction_input.data = json!({"stateVersion":paused.state_version});
        let correction = db
            .append_runtime_event(correction_input)
            .await
            .expect("persist correction");
        hub.publish(correction.clone());
        probe.release();
        let (_connection, frame) = resyncing.await.expect("join resync");
        let Some(RuntimeFrame::Resync {
            snapshot,
            cursor,
            reason,
        }) = frame
        else {
            panic!("expected resync");
        };
        assert_eq!(reason, "lagged");
        assert_eq!(snapshot.task.state_version, paused.state_version);
        assert_eq!(snapshot.task.status, TaskStatus::Paused);
        assert_eq!(cursor, correction.cursor);
    }

    #[tokio::test]
    async fn sse_endpoint_emits_snapshot_after_workspace_read_authorization() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use futures::StreamExt;
        use tower::ServiceExt;

        use crate::test_utils::{
            auth_header, create_test_token, seed_admin_role, seed_test_workspace, setup_test_app_with_pool,
        };

        let (app_state, pool) = setup_test_app_with_pool().await;
        seed_test_workspace(&pool, "tenant-1", "ws-1").await;
        seed_admin_role(&pool, "user-1").await;
        let db = Arc::new(Db::new(pool));
        let now = Utc::now();
        db.create_task_with_wake(
            &AgentTask {
                id: "task-1".into(),
                workspace_id: "ws-1".into(),
                idempotency_key: "stream-http".into(),
                objective: "inspect".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: TaskStatus::Active,
                current_run_id: None,
                state_version: 0,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &TaskWake {
                task_id: "task-1".into(),
                event_id: "stream-create".into(),
                signal_json: json!({"workspace_id":"ws-1"}).to_string(),
                dispatch_state: WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            },
        )
        .await
        .expect("task");
        let token = create_test_token("user-2", "tenant-1");
        let request = Request::builder()
            .uri("/api/v1/workspaces/ws-1/agent/tasks/task-1/stream?after=0")
            .header("Authorization", auth_header(&token))
            .body(Body::empty())
            .expect("request");
        let app = axum::Router::new()
            .nest("/api", crate::api::create_router(&app_state))
            .with_state(app_state);
        let response = app.oneshot(request).await.expect("stream response");
        assert_eq!(response.status(), StatusCode::OK);
        let first = response
            .into_body()
            .into_data_stream()
            .next()
            .await
            .expect("snapshot chunk")
            .expect("body chunk");
        let text = String::from_utf8(first.to_vec()).expect("utf8");
        assert!(text.contains("event: snapshot"), "frame: {text}");
        assert!(text.contains("\"type\":\"snapshot\""), "frame: {text}");
    }

    #[tokio::test]
    async fn recorder_persists_once_and_publishes_stable_event_identity() {
        let (db, hub) = fixture(16).await;
        let recorder = RuntimeRecorder::new(db.clone(), hub.clone());
        let mut connection = hub.connect(db.clone(), "ws-1", "task-1", None).await.expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));

        let first = recorder.record(input("stable-event")).await.expect("first record");
        let replay = recorder.record(input("stable-event")).await.expect("duplicate record");
        assert_eq!(first.cursor, replay.cursor);
        assert_eq!(first.event_id, replay.event_id);
        let Some(RuntimeFrame::Event { event }) = connection.next_frame().await else {
            panic!("expected live event");
        };
        assert_eq!(event.event_id, "stable-event");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), connection.next_frame())
                .await
                .is_err()
        );
        assert_eq!(
            db.runtime_events_after("ws-1", Some(0), Some("task-1"), 10)
                .await
                .expect("events")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn dropping_connection_cleans_up_immediately() {
        let (db, hub) = fixture(16).await;
        let connection = hub.connect(db, "ws-1", "task-1", None).await.expect("connect");
        assert_eq!(hub.active_connections("ws-1"), 1);
        drop(connection);
        assert_eq!(hub.active_connections("ws-1"), 0);
    }
}

//! Workspace-Harness-backed Task SSE tail (Task 19B).
//!
//! The connection never preloads history. It starts from the Task projection's
//! workspace high-watermark (or replays a bounded page after a caller cursor),
//! then tails committed `session_events` by `seq`. Live rows are read after
//! commit, so a write that happened while the connection was waking up is
//! still delivered exactly once. Writers call [`SessionStreamHub::notify`] to
//! wake tails immediately; a short interval keeps tails correct even when a
//! writer in another process cannot signal this hub.

use std::collections::{HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::extract::{Extension, Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use tinyiothub_core::harness::HarnessEvent;
use tinyiothub_storage::Db;
use tinyiothub_storage::runtime_event::TaskProjection;
use tinyiothub_web::security::Claims;
use tokio::sync::Notify;

const SEEN_EVENT_LIMIT: usize = 1024;
const TAIL_PAGE: i64 = 256;
const TAIL_INTERVAL: Duration = Duration::from_millis(500);

/// SSE envelope for the Workspace-Harness Task tail.
///
/// The `event` frame carries a [`HarnessEvent`] directly, so the wire fields
/// are `seq`, `eventType`, `payloadJson`, … (no `HarnessEvent` translation).
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionFrame {
    Snapshot {
        snapshot: Box<TaskProjection>,
        cursor: i64,
        #[serde(rename = "highWatermark")]
        high_watermark: i64,
    },
    Event {
        event: Box<HarnessEvent>,
        #[serde(rename = "highWatermark")]
        high_watermark: i64,
    },
    Resync {
        snapshot: Box<TaskProjection>,
        cursor: i64,
        #[serde(rename = "highWatermark")]
        high_watermark: i64,
        reason: String,
    },
}

impl SessionFrame {
    fn name(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Event { .. } => "event",
            Self::Resync { .. } => "resync",
        }
    }
}

#[derive(Default)]
struct SeenEvents {
    order: VecDeque<String>,
    ids: HashSet<String>,
}

impl SeenEvents {
    fn contains(&self, event_id: &str) -> bool {
        self.ids.contains(event_id)
    }

    fn insert(&mut self, event_id: String) {
        if !self.ids.insert(event_id.clone()) {
            return;
        }
        self.order.push_back(event_id);
        while self.order.len() > SEEN_EVENT_LIMIT {
            if let Some(expired) = self.order.pop_front() {
                self.ids.remove(&expired);
            }
        }
    }

    fn clear(&mut self) {
        self.order.clear();
        self.ids.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.ids.len()
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessStreamMetrics {
    pub active_connections: u64,
    pub connections_opened: u64,
    pub connections_closed: u64,
    pub resyncs: u64,
    pub duplicates_dropped: u64,
    pub first_frames: u64,
    pub first_frame_ms_total: u64,
}

#[derive(Default)]
pub struct SessionStreamHub {
    notifications: DashMap<String, Arc<Notify>>,
    connections: DashMap<String, usize>,
    connections_opened: AtomicU64,
    connections_closed: AtomicU64,
    resyncs: AtomicU64,
    duplicates_dropped: AtomicU64,
    first_frames: AtomicU64,
    first_frame_ms_total: AtomicU64,
}

impl SessionStreamHub {
    fn notifier(&self, workspace_id: &str) -> Arc<Notify> {
        self.notifications
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Wake tails after a committed write. This carries no data; the tail
    /// re-reads `session_events` so a lost notification can never lose a fact.
    pub fn notify(&self, workspace_id: &str) {
        self.notifier(workspace_id).notify_waiters();
    }

    pub fn metrics(&self) -> HarnessStreamMetrics {
        HarnessStreamMetrics {
            active_connections: self.connections.iter().map(|entry| *entry.value() as u64).sum(),
            connections_opened: self.connections_opened.load(Ordering::Relaxed),
            connections_closed: self.connections_closed.load(Ordering::Relaxed),
            resyncs: self.resyncs.load(Ordering::Relaxed),
            duplicates_dropped: self.duplicates_dropped.load(Ordering::Relaxed),
            first_frames: self.first_frames.load(Ordering::Relaxed),
            first_frame_ms_total: self.first_frame_ms_total.load(Ordering::Relaxed),
        }
    }

    pub fn active_connections(&self, workspace_id: &str) -> usize {
        self.connections.get(workspace_id).map_or(0, |count| *count)
    }

    /// Where the tail resumes. A known caller cursor newer than the projection
    /// snapshot means an Event committed between the projection read and the
    /// cursor check; resuming after the newer of the two avoids re-delivering
    /// Events the caller already acknowledged.
    fn resume_cursor(cursor: i64, after: Option<i64>, unknown: bool) -> i64 {
        if unknown {
            cursor
        } else {
            after.map_or(cursor, |after| cursor.max(after))
        }
    }

    pub async fn connect(
        self: &Arc<Self>,
        db: Arc<Db>,
        workspace_id: &str,
        task_id: &str,
        after: Option<i64>,
    ) -> Result<HarnessConnection, String> {
        let projection = db
            .harness_runtime_projection(workspace_id, task_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task not found".to_string())?;
        let cursor = projection.cursor;
        let unknown = match after {
            Some(after) => !db
                .harness_cursor_known(workspace_id, after)
                .await
                .map_err(|error| error.to_string())?,
            None => false,
        };
        let resume_cursor = Self::resume_cursor(cursor, after, unknown);
        let mut pending = VecDeque::new();
        if unknown {
            pending.push_back(SessionFrame::Resync {
                snapshot: Box::new(projection),
                cursor,
                high_watermark: resume_cursor,
                reason: "unknown_cursor".into(),
            });
        } else {
            pending.push_back(SessionFrame::Snapshot {
                snapshot: Box::new(projection),
                cursor,
                high_watermark: resume_cursor,
            });
        }
        *self.connections.entry(workspace_id.to_string()).or_insert(0) += 1;
        self.connections_opened.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            metric = "runtime_sse_connected",
            %workspace_id,
            %task_id,
            active = self.active_connections(workspace_id),
            "harness runtime stream connected"
        );
        Ok(HarnessConnection {
            db,
            workspace_id: workspace_id.to_string(),
            task_id: task_id.to_string(),
            pending,
            replay_after: if unknown { None } else { after },
            replay_page: VecDeque::new(),
            last_cursor: resume_cursor,
            high_watermark: resume_cursor,
            seen: SeenEvents::default(),
            notify: self.notifier(workspace_id),
            ticker: tokio::time::interval(TAIL_INTERVAL),
            hub: self.clone(),
            connected_at: Instant::now(),
            first_frame: false,
            _connected: ConnectionGuard::new(self, workspace_id),
        })
    }
}

struct ConnectionGuard {
    hub: Arc<SessionStreamHub>,
    workspace_id: String,
}

impl ConnectionGuard {
    fn new(hub: &Arc<SessionStreamHub>, workspace_id: &str) -> Self {
        Self {
            hub: hub.clone(),
            workspace_id: workspace_id.to_string(),
        }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(mut count) = self.hub.connections.get_mut(&self.workspace_id) {
            *count = count.saturating_sub(1);
        }
        self.hub
            .connections
            .remove_if(&self.workspace_id, |_, count| *count == 0);
        self.hub.connections_closed.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            metric = "runtime_sse_disconnected",
            workspace_id = %self.workspace_id,
            "harness runtime stream disconnected"
        );
    }
}

pub struct HarnessConnection {
    db: Arc<Db>,
    workspace_id: String,
    task_id: String,
    pending: VecDeque<SessionFrame>,
    replay_after: Option<i64>,
    replay_page: VecDeque<HarnessEvent>,
    last_cursor: i64,
    high_watermark: i64,
    seen: SeenEvents,
    notify: Arc<Notify>,
    ticker: tokio::time::Interval,
    hub: Arc<SessionStreamHub>,
    connected_at: Instant,
    first_frame: bool,
    _connected: ConnectionGuard,
}

impl HarnessConnection {
    fn record_first_frame(&mut self) {
        if !self.first_frame {
            self.first_frame = true;
            self.hub
                .record_first_frame(self.connected_at.elapsed().as_millis() as u64);
        }
    }

    fn record_event(&mut self, event: HarnessEvent) -> SessionFrame {
        self.last_cursor = self.last_cursor.max(event.seq);
        self.high_watermark = self.high_watermark.max(event.seq);
        self.seen.insert(event.event_id.clone());
        SessionFrame::Event {
            event: Box::new(event),
            high_watermark: self.high_watermark,
        }
    }

    /// Advance the public cursors over an event the public tail must not see
    /// (raw `turn.state`). The row is still acknowledged so the tail never
    /// re-reads it.
    fn skip_event(&mut self, event: &HarnessEvent) {
        self.last_cursor = self.last_cursor.max(event.seq);
        self.high_watermark = self.high_watermark.max(event.seq);
        self.seen.insert(event.event_id.clone());
    }

    async fn page(&self, after: i64) -> Result<Vec<HarnessEvent>, String> {
        self.db
            .harness_task_events_after(&self.workspace_id, &self.task_id, after, TAIL_PAGE)
            .await
            .map_err(|error| error.to_string())
    }

    async fn projection_mismatch(&self, event: &HarnessEvent) -> Result<bool, String> {
        let Some(expected) = event
            .payload_json
            .get("stateVersion")
            .and_then(serde_json::Value::as_i64)
        else {
            return Ok(false);
        };
        let projection = self
            .db
            .harness_runtime_projection(&self.workspace_id, &self.task_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "task projection disappeared".to_string())?;
        Ok(projection.task.state_version != expected)
    }

    async fn resync(&mut self, reason: &str) -> Option<SessionFrame> {
        self.hub.resyncs.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(metric = "runtime_sse_resync", reason, "harness runtime stream resync");
        let projection = self
            .db
            .harness_runtime_projection(&self.workspace_id, &self.task_id)
            .await
            .ok()
            .flatten()?;
        self.last_cursor = projection.cursor;
        self.high_watermark = projection.cursor;
        self.replay_after = None;
        self.replay_page.clear();
        self.seen.clear();
        Some(SessionFrame::Resync {
            cursor: projection.cursor,
            high_watermark: projection.cursor,
            snapshot: Box::new(projection),
            reason: reason.to_string(),
        })
    }

    pub async fn next_frame(&mut self) -> Option<SessionFrame> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                self.record_first_frame();
                return Some(frame);
            }
            if let Some(event) = self.replay_page.pop_front() {
                if !crate::domains::agent::session::is_public_session_event(&event.event_type) {
                    self.skip_event(&event);
                    continue;
                }
                // Replay starts after the caller cursor, which may be older than
                // the snapshot high-watermark. Only an id already emitted on
                // this connection is a duplicate.
                if self.seen.contains(&event.event_id) {
                    self.hub.duplicates_dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                self.record_first_frame();
                return Some(self.record_event(event));
            }
            if let Some(after) = self.replay_after {
                match self.page(after).await {
                    Ok(rows) if rows.is_empty() => self.replay_after = None,
                    Ok(rows) => {
                        self.replay_after = rows.last().map(|event| event.seq);
                        self.replay_page.extend(rows);
                    }
                    Err(error) => {
                        tracing::warn!(
                            workspace_id = %self.workspace_id,
                            task_id = %self.task_id,
                            %error,
                            "harness stream replay read failed"
                        );
                        return None;
                    }
                }
                continue;
            }
            // Tail committed facts first, then wait for a writer notification or
            // the bounded retry interval. This is a post-commit read: no event is
            // delivered before it is durable.
            match self.page(self.last_cursor).await {
                Ok(rows) if rows.is_empty() => {}
                Ok(rows) => {
                    self.replay_page.extend(rows);
                    continue;
                }
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %self.workspace_id,
                        task_id = %self.task_id,
                        %error,
                        "harness stream tail read failed"
                    );
                    return None;
                }
            }
            tokio::select! {
                _ = self.ticker.tick() => {}
                _ = self.notify.notified() => {}
            }
            // A projection drift can only come from a fact this tail has not
            // observed yet; re-check before waiting again. Compare the newest
            // pending state-bearing Event: when two transitions commit inside
            // one idle interval, the older row's stateVersion is already
            // stale against the current projection and would fake a resync.
            if let Ok(rows) = self.page(self.last_cursor).await
                && let Some(event) = rows.iter().rev().find(|event| {
                    event
                        .payload_json
                        .get("stateVersion")
                        .and_then(serde_json::Value::as_i64)
                        .is_some()
                })
                && self.projection_mismatch(event).await.ok() == Some(true)
            {
                let frame = self.resync("projection_mismatch").await?;
                self.pending.push_back(frame);
            }
        }
    }
}

impl SessionStreamHub {
    fn record_first_frame(&self, elapsed_ms: u64) {
        self.first_frames.fetch_add(1, Ordering::Relaxed);
        self.first_frame_ms_total.fetch_add(elapsed_ms, Ordering::Relaxed);
        tracing::info!(
            metric = "runtime_sse_first_frame",
            elapsed_ms,
            "harness runtime stream first frame"
        );
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
            tracing::error!(%workspace_id, %error, "check harness stream workspace failed");
            return super::tasks::error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败");
        }
    }
    if let Err(response) = super::projection::load(&state, &workspace_id, &task_id).await {
        return response;
    }
    let mut connection = match state
        .session_stream
        .connect(state.db.clone(), &workspace_id, &task_id, query.after)
        .await
    {
        Ok(connection) => connection,
        Err(error) => {
            tracing::error!(%workspace_id, %task_id, %error, "open harness stream failed");
            return super::tasks::error_response(axum::http::StatusCode::SERVICE_UNAVAILABLE, "Task 事件流不可用");
        }
    };
    let stream = async_stream::stream! {
        while let Some(frame) = connection.next_frame().await {
            let data = match serde_json::to_string(&frame) {
                Ok(data) => data,
                Err(error) => {
                    tracing::error!(%error, "serialize harness stream frame failed");
                    break;
                }
            };
            yield Ok::<_, Infallible>(Event::default().event(frame.name()).data(data));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use serde_json::json;
    use tinyiothub_core::harness::{ExecutionMode, HarnessEventInput};
    use tinyiothub_storage::test_helpers::fixture_workspace;

    use super::{SEEN_EVENT_LIMIT, SeenEvents, SessionStreamHub};

    async fn seed_task(db: &tinyiothub_storage::Db, workspace_id: &str, task_id: &str) {
        let pool = db.workspace_pool_for_testing(workspace_id).await.expect("pool");
        sqlx::query(
            "INSERT INTO agent_tasks
                (id, run_id, state, state_version, execution_mode, runtime_idempotency_key,
                 runtime_objective, runtime_created_by, runtime_created_at, runtime_updated_at)
             VALUES (?, NULL, 'running', 0, 'production', ?, 'inspect', 'user-1', ?, ?)",
        )
        .bind(task_id)
        .bind(format!("key-{task_id}"))
        .bind(Utc::now())
        .bind(Utc::now())
        .execute(&pool)
        .await
        .expect("task");
    }

    async fn append(db: &tinyiothub_storage::Db, workspace_id: &str, task_id: &str, id: &str) -> i64 {
        db.append_session_event(&HarnessEventInput {
            event_id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            session_id: format!("task:{task_id}"),
            execution_mode: ExecutionMode::Production,
            run_id: None,
            task_id: Some(task_id.to_string()),
            event_type: "task.updated".to_string(),
            occurred_at: Utc::now(),
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("digest-{id}"),
            payload_json: json!({"stateVersion": 0}),
        })
        .await
        .expect("append")
        .seq
    }

    #[tokio::test]
    async fn known_cursor_replay_is_bounded_and_unknown_cursor_resyncs() {
        let fixture = fixture_workspace("ws-harness-stream").await;
        seed_task(&fixture.db, "ws-harness-stream", "task-1").await;
        let first = append(&fixture.db, "ws-harness-stream", "task-1", "event-1").await;
        append(&fixture.db, "ws-harness-stream", "task-1", "event-2").await;
        let hub = Arc::new(SessionStreamHub::default());
        let mut connection = hub
            .connect(Arc::new(fixture.db.clone()), "ws-harness-stream", "task-1", Some(first))
            .await
            .expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(super::SessionFrame::Snapshot { .. })
        ));
        let Some(super::SessionFrame::Event { event, .. }) = connection.next_frame().await else {
            panic!("expected replay event");
        };
        assert_eq!(event.event_id, "event-2");

        let mut connection = hub
            .connect(Arc::new(fixture.db.clone()), "ws-harness-stream", "task-1", Some(9_999))
            .await
            .expect("connect");
        let Some(super::SessionFrame::Resync { reason, .. }) = connection.next_frame().await else {
            panic!("unknown cursor must resync");
        };
        assert_eq!(reason, "unknown_cursor");
        fixture.close().await;
    }

    #[tokio::test]
    async fn live_tail_reads_after_commit_without_preloading_history() {
        let fixture = fixture_workspace("ws-harness-stream-live").await;
        seed_task(&fixture.db, "ws-harness-stream-live", "task-1").await;
        for number in 0..300 {
            append(
                &fixture.db,
                "ws-harness-stream-live",
                "task-1",
                &format!("history-{number}"),
            )
            .await;
        }
        let hub = Arc::new(SessionStreamHub::default());
        let mut connection = hub
            .connect(Arc::new(fixture.db.clone()), "ws-harness-stream-live", "task-1", None)
            .await
            .expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(super::SessionFrame::Snapshot { .. })
        ));
        append(&fixture.db, "ws-harness-stream-live", "task-1", "live-1").await;
        hub.notify("ws-harness-stream-live");
        let Some(super::SessionFrame::Event { event, .. }) = connection.next_frame().await else {
            panic!("expected live event");
        };
        assert_eq!(event.event_id, "live-1");
        fixture.close().await;
    }

    async fn append_versioned(
        db: &tinyiothub_storage::Db,
        workspace_id: &str,
        task_id: &str,
        id: &str,
        state_version: i64,
    ) -> i64 {
        db.append_session_event(&HarnessEventInput {
            event_id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            session_id: format!("task:{task_id}"),
            execution_mode: ExecutionMode::Production,
            run_id: None,
            task_id: Some(task_id.to_string()),
            event_type: "task.updated".to_string(),
            occurred_at: Utc::now(),
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("digest-{id}"),
            payload_json: json!({"stateVersion": state_version}),
        })
        .await
        .expect("append")
        .seq
    }

    /// The snapshot cursor is a production seq; a simulation Event committed
    /// later must not move it, or the server would issue a cursor that
    /// `harness_cursor_known` rejects on reconnect.
    #[tokio::test]
    async fn snapshot_cursor_never_names_a_simulation_event() {
        let fixture = fixture_workspace("ws-harness-stream-cursor-mode").await;
        let ws = "ws-harness-stream-cursor-mode";
        seed_task(&fixture.db, ws, "task-1").await;
        let production = append(&fixture.db, ws, "task-1", "event-production").await;
        let simulation = fixture
            .db
            .append_session_event(&HarnessEventInput {
                event_id: "event-simulation".to_string(),
                workspace_id: ws.to_string(),
                session_id: "task:task-1".to_string(),
                execution_mode: ExecutionMode::Simulation,
                run_id: None,
                task_id: Some("task-1".to_string()),
                event_type: "task.updated".to_string(),
                occurred_at: Utc::now(),
                cause_id: None,
                schema_v: 1,
                payload_digest: "digest-simulation".to_string(),
                payload_json: json!({"stateVersion": 9}),
            })
            .await
            .expect("simulation append")
            .seq;
        assert!(simulation > production, "simulation seq must trail the production seq");
        let hub = Arc::new(SessionStreamHub::default());
        let mut connection = hub
            .connect(Arc::new(fixture.db.clone()), ws, "task-1", None)
            .await
            .expect("connect");
        let Some(super::SessionFrame::Snapshot {
            cursor, high_watermark, ..
        }) = connection.next_frame().await
        else {
            panic!("expected snapshot");
        };
        assert_eq!(cursor, production, "snapshot cursor must be a production seq");
        assert_eq!(high_watermark, production);
        assert_eq!(connection.last_cursor, production);
        fixture.close().await;
    }

    /// Two state transitions committing inside one idle interval must not fake
    /// a projection resync: the newest Event's stateVersion already matches the
    /// projection.
    #[tokio::test]
    async fn two_transitions_in_one_interval_do_not_fake_a_projection_resync() {
        let fixture = fixture_workspace("ws-harness-stream-multi-version").await;
        let ws = "ws-harness-stream-multi-version";
        seed_task(&fixture.db, ws, "task-1").await;
        let hub = Arc::new(SessionStreamHub::default());
        let mut connection = hub
            .connect(Arc::new(fixture.db.clone()), ws, "task-1", None)
            .await
            .expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(super::SessionFrame::Snapshot { .. })
        ));
        // Commit only once the tail is parked in its wait: the drift check
        // under test runs in the post-wait path, not in the pre-wait tail read.
        let db = Arc::new(fixture.db.clone());
        let wake = hub.clone();
        let writer = {
            let db = db.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                append_versioned(&db, ws, "task-1", "event-v1", 1).await;
                append_versioned(&db, ws, "task-1", "event-v2", 2).await;
                sqlx::query("UPDATE agent_tasks SET state_version = 2 WHERE id = 'task-1'")
                    .execute(&db.workspace_pool_for_testing(ws).await.expect("pool"))
                    .await
                    .expect("project state version");
                wake.notify(ws);
            })
        };
        for expected in ["event-v1", "event-v2"] {
            let Some(super::SessionFrame::Event { event, .. }) = connection.next_frame().await else {
                panic!("expected the {expected} Event, not a resync");
            };
            assert_eq!(event.event_id, expected);
        }
        writer.await.expect("writer");
        fixture.close().await;
    }

    #[test]
    fn resume_cursor_never_rewinds_behind_a_known_caller_cursor() {
        assert_eq!(SessionStreamHub::resume_cursor(4, Some(7), false), 7);
        assert_eq!(SessionStreamHub::resume_cursor(4, Some(2), false), 4);
        assert_eq!(SessionStreamHub::resume_cursor(4, None, false), 4);
        assert_eq!(SessionStreamHub::resume_cursor(4, Some(7), true), 4);
    }

    #[test]
    fn per_connection_dedupe_window_stays_bounded() {
        let mut seen = SeenEvents::default();
        for index in 0..SEEN_EVENT_LIMIT + 64 {
            seen.insert(format!("event-{index}"));
        }
        assert_eq!(seen.len(), SEEN_EVENT_LIMIT);
        assert!(!seen.contains("event-0"));
    }

    #[test]
    fn hub_metrics_are_exposed() {
        let hub = SessionStreamHub::default();
        assert_eq!(hub.metrics().active_connections, 0);
    }
}

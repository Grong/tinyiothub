use std::collections::{HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

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

/// Per-connection duplicate-suppression window. The cursor check already drops
/// anything at or below `last_cursor`; this set only covers equal-cursor races
/// between replay and live publication, so a bounded window is sufficient and
/// keeps memory flat on long-lived connections.
const SEEN_EVENT_LIMIT: usize = 1024;

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

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeFrame {
    Snapshot {
        snapshot: Box<TaskProjection>,
        cursor: i64,
        #[serde(rename = "highWatermark")]
        high_watermark: i64,
    },
    Event {
        event: Box<RuntimeEvent>,
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

    pub fn publish(&self, event: RuntimeEvent) {
        self.hub.publish(event);
    }
}

pub struct RuntimeStreamHub {
    senders: DashMap<String, Sender<RuntimeEvent>>,
    connections: Arc<DashMap<String, usize>>,
    capacity: usize,
    connections_opened: AtomicU64,
    connections_closed: AtomicU64,
    resyncs: AtomicU64,
    duplicates_dropped: AtomicU64,
    foreign_task_events: AtomicU64,
    task_less_events: AtomicU64,
    first_frames: AtomicU64,
    first_frame_ms_total: AtomicU64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStreamMetrics {
    pub active_connections: u64,
    pub connections_opened: u64,
    pub connections_closed: u64,
    pub resyncs: u64,
    pub duplicates_dropped: u64,
    pub foreign_task_events: u64,
    pub task_less_events: u64,
    pub first_frames: u64,
    pub first_frame_ms_total: u64,
}

impl RuntimeStreamHub {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            senders: DashMap::new(),
            connections: Arc::new(DashMap::new()),
            capacity: capacity.max(1),
            connections_opened: AtomicU64::new(0),
            connections_closed: AtomicU64::new(0),
            resyncs: AtomicU64::new(0),
            duplicates_dropped: AtomicU64::new(0),
            foreign_task_events: AtomicU64::new(0),
            task_less_events: AtomicU64::new(0),
            first_frames: AtomicU64::new(0),
            first_frame_ms_total: AtomicU64::new(0),
        }
    }

    pub fn metrics(&self) -> RuntimeStreamMetrics {
        RuntimeStreamMetrics {
            active_connections: self.connections.iter().map(|entry| *entry.value() as u64).sum(),
            connections_opened: self.connections_opened.load(Ordering::Relaxed),
            connections_closed: self.connections_closed.load(Ordering::Relaxed),
            resyncs: self.resyncs.load(Ordering::Relaxed),
            duplicates_dropped: self.duplicates_dropped.load(Ordering::Relaxed),
            foreign_task_events: self.foreign_task_events.load(Ordering::Relaxed),
            task_less_events: self.task_less_events.load(Ordering::Relaxed),
            first_frames: self.first_frames.load(Ordering::Relaxed),
            first_frame_ms_total: self.first_frame_ms_total.load(Ordering::Relaxed),
        }
    }

    fn record_duplicate(&self) {
        self.duplicates_dropped.fetch_add(1, Ordering::Relaxed);
    }

    fn record_foreign_task_event(&self) {
        self.foreign_task_events.fetch_add(1, Ordering::Relaxed);
    }

    fn record_task_less_event(&self) {
        self.task_less_events.fetch_add(1, Ordering::Relaxed);
    }

    fn record_resync(&self, reason: &str) {
        self.resyncs.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(metric = "runtime_sse_resync", reason, "runtime stream resync");
    }

    fn record_first_frame(&self, elapsed_ms: u64) {
        self.first_frames.fetch_add(1, Ordering::Relaxed);
        self.first_frame_ms_total.fetch_add(elapsed_ms, Ordering::Relaxed);
        tracing::info!(
            metric = "runtime_sse_first_frame",
            elapsed_ms,
            "runtime stream first frame"
        );
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
                high_watermark: cursor,
                reason: "unknown_cursor".into(),
            });
        } else {
            pending.push_back(RuntimeFrame::Snapshot {
                snapshot: Box::new(projection),
                cursor,
                high_watermark: cursor,
            });
        }
        let guard = ConnectionGuard::new(self, workspace_id.to_string());
        self.connections_opened.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            metric = "runtime_sse_connected",
            %workspace_id,
            %task_id,
            active = self.active_connections(workspace_id),
            "runtime stream connected"
        );
        Ok(RuntimeConnection {
            db,
            workspace_id: workspace_id.to_string(),
            task_id: task_id.to_string(),
            events,
            pending,
            replay_after: if unknown { None } else { after },
            replay_page: VecDeque::new(),
            last_cursor: cursor,
            high_watermark: cursor,
            seen: SeenEvents::default(),
            hub: self.clone(),
            connected_at: Instant::now(),
            first_frame: false,
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
    hub: Arc<RuntimeStreamHub>,
    workspace_id: String,
}

impl ConnectionGuard {
    fn new(hub: &Arc<RuntimeStreamHub>, workspace_id: String) -> Self {
        *hub.connections.entry(workspace_id.clone()).or_insert(0) += 1;
        Self {
            hub: hub.clone(),
            workspace_id,
        }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let connections = &self.hub.connections;
        if let Some(mut count) = connections.get_mut(&self.workspace_id) {
            *count = count.saturating_sub(1);
        }
        connections.remove_if(&self.workspace_id, |_, count| *count == 0);
        self.hub.connections_closed.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            metric = "runtime_sse_disconnected",
            workspace_id = %self.workspace_id,
            "runtime stream disconnected"
        );
    }
}

pub struct RuntimeConnection {
    db: Arc<Db>,
    workspace_id: String,
    task_id: String,
    events: Receiver<RuntimeEvent>,
    pending: VecDeque<RuntimeFrame>,
    replay_after: Option<i64>,
    replay_page: VecDeque<RuntimeEvent>,
    last_cursor: i64,
    high_watermark: i64,
    seen: SeenEvents,
    hub: Arc<RuntimeStreamHub>,
    connected_at: Instant,
    first_frame: bool,
    _guard: ConnectionGuard,
}

impl RuntimeConnection {
    fn record_first_frame(&mut self) {
        if !self.first_frame {
            self.first_frame = true;
            self.hub
                .record_first_frame(self.connected_at.elapsed().as_millis() as u64);
        }
    }

    pub async fn next_frame(&mut self) -> Option<RuntimeFrame> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                self.record_first_frame();
                return Some(frame);
            }
            if let Some(event) = self.replay_page.pop_front() {
                self.last_cursor = self.last_cursor.max(event.cursor);
                self.high_watermark = self.high_watermark.max(event.cursor);
                self.seen.insert(event.event_id.clone());
                self.record_first_frame();
                return Some(RuntimeFrame::Event {
                    event: Box::new(event),
                    high_watermark: self.high_watermark,
                });
            }
            if let Some(after) = self.replay_after {
                let rows = match self
                    .db
                    .runtime_events_after(&self.workspace_id, Some(after), Some(&self.task_id), 256)
                    .await
                {
                    Ok(rows) => rows,
                    Err(error) => {
                        self.warn_closing("replay_read", Some(after), &error);
                        return None;
                    }
                };
                self.replay_after = rows.last().map(|event| event.cursor);
                self.replay_page.extend(rows);
                if self.replay_page.is_empty() {
                    self.replay_after = None;
                }
                continue;
            }
            match self.events.recv().await {
                Ok(event) => {
                    self.high_watermark = self.high_watermark.max(event.cursor);
                    match event.task_id.as_deref() {
                        Some(task_id) if task_id == self.task_id => {}
                        Some(_) => {
                            self.hub.record_foreign_task_event();
                            continue;
                        }
                        None => {
                            // Workspace-scoped events (for example a run that
                            // never belonged to a Task) are not "foreign Task"
                            // traffic and must not raise that SSE alarm.
                            self.hub.record_task_less_event();
                            continue;
                        }
                    }
                    if event.cursor <= self.last_cursor || self.seen.contains(&event.event_id) {
                        self.hub.record_duplicate();
                        continue;
                    }
                    if event.cursor > self.last_cursor + 1 && self.missed_task_event_before(&event).await? {
                        let frame = self.resync("out_of_order").await?;
                        self.pending.push_back(frame);
                        continue;
                    }
                    if self.projection_mismatch(&event).await? {
                        let frame = self.resync("projection_mismatch").await?;
                        self.pending.push_back(frame);
                        continue;
                    }
                    self.last_cursor = event.cursor;
                    self.seen.insert(event.event_id.clone());
                    self.record_first_frame();
                    return Some(RuntimeFrame::Event {
                        event: Box::new(event),
                        high_watermark: self.high_watermark,
                    });
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

    fn warn_closing(&self, reason: &'static str, cursor: Option<i64>, error: &dyn std::fmt::Display) {
        tracing::warn!(
            workspace_id = %self.workspace_id,
            task_id = %self.task_id,
            reason,
            cursor,
            %error,
            "runtime stream closing after runtime persistence read failure"
        );
    }

    fn closing_without_projection<T>(&self, reason: &'static str, cursor: Option<i64>) -> Option<T> {
        tracing::warn!(
            workspace_id = %self.workspace_id,
            task_id = %self.task_id,
            reason,
            cursor,
            "runtime stream closing: runtime projection not found"
        );
        None
    }

    async fn missed_task_event_before(&self, event: &RuntimeEvent) -> Option<bool> {
        let rows = match self
            .db
            .runtime_events_after(&self.workspace_id, Some(self.last_cursor), Some(&self.task_id), 2)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                self.warn_closing("checkpoint_read", Some(event.cursor), &error);
                return None;
            }
        };
        Some(rows.first().is_some_and(|first| first.cursor != event.cursor))
    }

    async fn projection_mismatch(&self, event: &RuntimeEvent) -> Option<bool> {
        let Some(expected) = event.data.get("stateVersion").and_then(serde_json::Value::as_i64) else {
            return Some(false);
        };
        let projection = match self.db.runtime_projection(&self.workspace_id, &self.task_id).await {
            Ok(Some(projection)) => projection,
            Ok(None) => return self.closing_without_projection("projection_missing", Some(event.cursor)),
            Err(error) => {
                self.warn_closing("projection_read", Some(event.cursor), &error);
                return None;
            }
        };
        Some(projection.task.state_version != expected)
    }

    async fn resync(&mut self, reason: &str) -> Option<RuntimeFrame> {
        self.hub.record_resync(reason);
        let projection = match self.db.runtime_projection(&self.workspace_id, &self.task_id).await {
            Ok(Some(projection)) => projection,
            Ok(None) => return self.closing_without_projection("resync_projection_missing", Some(self.last_cursor)),
            Err(error) => {
                self.warn_closing("resync_projection_read", Some(self.last_cursor), &error);
                return None;
            }
        };
        self.last_cursor = projection.cursor;
        self.high_watermark = projection.cursor;
        self.replay_after = None;
        self.replay_page.clear();
        self.seen.clear();
        Some(RuntimeFrame::Resync {
            cursor: projection.cursor,
            high_watermark: projection.cursor,
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

    use super::{RuntimeFrame, RuntimeRecorder, RuntimeStreamHub, SEEN_EVENT_LIMIT, SeenEvents};

    async fn fixture(capacity: usize) -> (Arc<Db>, Arc<RuntimeStreamHub>) {
        let pool = tinyiothub_storage::test_helpers::test_pool().await;
        fixture_with_pool(pool, capacity).await
    }

    async fn wal_fixture(capacity: usize) -> (Arc<Db>, Arc<RuntimeStreamHub>) {
        let path = std::env::temp_dir().join(format!("tinyiothub-stream-{}.db", uuid::Uuid::new_v4()));
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .expect("wal pool");
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        fixture_with_pool(pool, capacity).await
    }

    async fn fixture_with_pool(pool: sqlx::SqlitePool, capacity: usize) -> (Arc<Db>, Arc<RuntimeStreamHub>) {
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

    fn owned_watch(id: &str) -> tinyiothub_storage::watch::WatchRuleRow {
        tinyiothub_storage::watch::WatchRuleRow {
            id: id.into(),
            workspace_id: "ws-1".into(),
            kind: "availability".into(),
            condition: json!({"type":"availability","thing_id":"device-1","online":true}).to_string(),
            persistent: 0,
            timeout_secs: 60,
            created_at: Utc::now().to_rfc3339(),
            owner_task_id: Some("task-1".into()),
        }
    }

    #[tokio::test]
    async fn replay_streams_one_bounded_page_instead_of_preloading_history() {
        let (db, hub) = fixture(16).await;
        let created_cursor = db
            .runtime_projection("ws-1", "task-1")
            .await
            .expect("projection")
            .expect("task")
            .cursor;
        for index in 0..513 {
            db.append_runtime_event(input(&format!("page-event-{index}")))
                .await
                .expect("append replay fixture");
        }

        let mut connection = hub
            .connect(db, "ws-1", "task-1", Some(created_cursor))
            .await
            .expect("connect");
        assert!(
            connection.pending.len() <= 1,
            "connect must not buffer the replay: {} frames",
            connection.pending.len()
        );
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        let Some(RuntimeFrame::Event { event, .. }) = connection.next_frame().await else {
            panic!("first replay event");
        };
        assert_eq!(event.event_id, "page-event-0");
        assert!(
            !connection.replay_page.is_empty() && connection.replay_page.len() <= 256,
            "at most one 256-event page may be prefetched, got {}",
            connection.replay_page.len()
        );
    }

    #[tokio::test]
    #[ignore = "million-event fixture; run explicitly for bounded-memory checks"]
    async fn million_event_replay_still_prefetches_only_one_page() {
        let (db, hub) = fixture(16).await;
        let payload = sqlx::query_scalar::<_, String>(
            "SELECT payload FROM hub_ledger
             WHERE workspace_id = 'ws-1' AND kind = 'runtime.v1'
               AND json_extract(payload, '$.eventId') = 'task:task-1:created'",
        )
        .fetch_one(db.pool())
        .await
        .expect("source event payload");
        sqlx::query(
            "WITH RECURSIVE fixture(n) AS (
                 SELECT 1 UNION ALL SELECT n + 1 FROM fixture WHERE n < 1000000
             )
             INSERT INTO hub_ledger (workspace_id, ts, kind, thing_id, payload)
             SELECT 'ws-1', '2026-11-03T00:00:00Z', 'runtime.v1', NULL, ? FROM fixture",
        )
        .bind(payload)
        .execute(db.pool())
        .await
        .expect("million-event fixture");

        let mut connection = hub.connect(db, "ws-1", "task-1", Some(0)).await.expect("connect");
        assert_eq!(connection.pending.len(), 1, "only the snapshot is pending");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Event { .. })
        ));
        assert!(connection.replay_page.len() <= 256);
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
        let Some(RuntimeFrame::Event { event, .. }) = connection.next_frame().await else {
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
        assert_eq!(
            hub.metrics().duplicates_dropped,
            0,
            "a foreign Task event is filtered traffic, not a duplicate"
        );
        assert_eq!(hub.metrics().foreign_task_events, 1);

        let mut task_less = input("task-less");
        task_less.task_id = None;
        hub.publish(db.append_runtime_event(task_less).await.expect("task-less"));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), connection.next_frame())
                .await
                .is_err()
        );
        assert_eq!(
            hub.metrics().foreign_task_events,
            1,
            "task-less events must not inflate the foreign-Task alarm"
        );
        assert_eq!(hub.metrics().task_less_events, 1);

        let duplicate = db.append_runtime_event(input("event-1")).await.expect("first");
        hub.publish(duplicate.clone());
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Event { .. })
        ));
        hub.publish(duplicate);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), connection.next_frame())
                .await
                .is_err()
        );
        assert_eq!(hub.metrics().duplicates_dropped, 1);
        assert_eq!(
            hub.metrics().foreign_task_events,
            1,
            "real duplicates must not be counted as foreign Task traffic"
        );
        assert_eq!(hub.metrics().task_less_events, 1);
    }

    #[test]
    fn per_connection_dedupe_window_stays_bounded() {
        let mut seen = SeenEvents::default();
        let total = SEEN_EVENT_LIMIT + 64;
        for index in 0..total {
            seen.insert(format!("event-{index}"));
        }
        assert_eq!(seen.len(), SEEN_EVENT_LIMIT);
        assert!(!seen.contains("event-0"), "oldest id must be evicted");
        assert!(
            seen.contains(&format!("event-{}", total - 1)),
            "newest id must be retained"
        );
        seen.clear();
        assert_eq!(seen.len(), 0);
    }

    #[tokio::test]
    async fn checkpoint_read_failure_closes_the_connection_instead_of_going_silent() {
        let (db, hub) = fixture(16).await;
        let mut connection = hub.connect(db.clone(), "ws-1", "task-1", None).await.expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        db.append_runtime_event(input("event-1")).await.expect("first");
        let gap = db.append_runtime_event(input("event-2")).await.expect("second");
        sqlx::query("ALTER TABLE hub_ledger RENAME TO hub_ledger_hidden")
            .execute(db.pool())
            .await
            .expect("inject checkpoint read failure");
        hub.publish(gap);
        let closed = tokio::time::timeout(Duration::from_millis(100), connection.next_frame()).await;
        sqlx::query("ALTER TABLE hub_ledger_hidden RENAME TO hub_ledger")
            .execute(db.pool())
            .await
            .expect("restore ledger");
        assert!(
            matches!(closed, Ok(None)),
            "a persistence read failure must end the stream (warned), not hang"
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
            let Some(RuntimeFrame::Event { event, .. }) = connection.next_frame().await else {
                panic!("expected two committed versions");
            };
            replayed_versions.push(event.data["stateVersion"].as_i64());
        }
        assert_eq!(replayed_versions, [Some(2), Some(3)]);
    }

    #[tokio::test]
    async fn established_connection_recovers_when_live_publication_arrives_in_reverse() {
        let (db, hub) = fixture(16).await;
        let mut connection = hub.connect(db.clone(), "ws-1", "task-1", None).await.expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        db.transition_task("ws-1", "task-1", 1, TaskStatus::Paused, None)
            .await
            .expect("commit version two");
        db.transition_task("ws-1", "task-1", 2, TaskStatus::Active, None)
            .await
            .expect("commit version three");
        let second = db
            .runtime_event_by_id("ws-1", "task:task-1:v3:active")
            .await
            .expect("load newer")
            .expect("newer event");
        let first = db
            .runtime_event_by_id("ws-1", "task:task-1:v2:paused")
            .await
            .expect("load older")
            .expect("older event");
        hub.publish(second);
        hub.publish(first);

        let frame = connection.next_frame().await.expect("recovery frame");
        let RuntimeFrame::Resync {
            snapshot,
            cursor,
            reason,
            ..
        } = frame
        else {
            panic!("expected out-of-order live recovery");
        };
        assert_eq!(reason, "out_of_order");
        assert_eq!(snapshot.task.state_version, 3);
        assert!(cursor >= 3);
    }

    #[tokio::test]
    async fn post_commit_publication_read_failure_cannot_leave_live_connection_stale() {
        let (db, hub) = fixture(16).await;
        let recorder = RuntimeRecorder::new(db.clone(), hub.clone());
        let mut connection = hub.connect(db.clone(), "ws-1", "task-1", None).await.expect("connect");
        assert!(matches!(
            connection.next_frame().await,
            Some(RuntimeFrame::Snapshot { .. })
        ));
        let (_, event) = db
            .transition_task_and_event("ws-1", "task-1", 1, TaskStatus::Paused, None)
            .await
            .expect("commit paused Task");
        sqlx::query("ALTER TABLE hub_ledger RENAME TO hub_ledger_hidden")
            .execute(db.pool())
            .await
            .expect("inject publication read failure");
        recorder.publish(event);
        sqlx::query("ALTER TABLE hub_ledger_hidden RENAME TO hub_ledger")
            .execute(db.pool())
            .await
            .expect("restore ledger");
        let frame = tokio::time::timeout(Duration::from_millis(100), connection.next_frame())
            .await
            .expect("live correction despite publication read failure")
            .expect("correction frame");
        assert!(matches!(
            frame,
            RuntimeFrame::Event { .. } | RuntimeFrame::Resync { .. }
        ));
    }

    #[tokio::test]
    async fn fresh_connection_uses_coherent_snapshot_then_receives_committed_watch_deletion() {
        let (db, hub) = wal_fixture(16).await;
        tinyiothub_storage::watch::insert_rule(db.pool(), &owned_watch("watch-race"))
            .await
            .expect("insert watch");
        let probe = Arc::new(tinyiothub_storage::runtime_event::RuntimeProjectionProbe::default());
        let db = Arc::new(db.with_runtime_projection_probe(probe.clone()));
        probe.arm();
        let connecting = tokio::spawn({
            let db = db.clone();
            let hub = hub.clone();
            async move { hub.connect(db, "ws-1", "task-1", None).await }
        });
        probe.wait_for_task_read().await;
        let deleted = tinyiothub_storage::watch::delete_task_watches_and_events(db.pool(), "ws-1", "task-1")
            .await
            .expect("delete during snapshot");
        assert_eq!(deleted.len(), 1);
        hub.publish(deleted[0].clone());
        probe.release();
        let mut connection = connecting.await.expect("join connection").expect("connect");
        let Some(RuntimeFrame::Snapshot { snapshot, cursor, .. }) = connection.next_frame().await else {
            panic!("expected initial snapshot");
        };
        assert_eq!(probe.attempts(), 1);
        assert_eq!(snapshot.watches.len(), 1);
        assert!(cursor < deleted[0].cursor);
        let Some(RuntimeFrame::Event { event, .. }) = connection.next_frame().await else {
            panic!("expected deletion correction");
        };
        assert_eq!(event.event_id, "watch:watch-race:deleted");
        assert_eq!(event.cursor, deleted[0].cursor);
    }

    #[tokio::test]
    async fn resync_uses_coherent_snapshot_then_receives_committed_task_correction() {
        let (db, hub) = wal_fixture(2).await;
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
        hub.publish(db.append_runtime_event(input("lag-3")).await.expect("third lag event"));
        let resyncing = tokio::spawn(async move {
            let frame = connection.next_frame().await;
            (connection, frame)
        });
        probe.wait_for_task_read().await;
        let (paused, correction) = db
            .transition_task_and_event("ws-1", "task-1", 1, TaskStatus::Paused, None)
            .await
            .expect("commit next Task version");
        hub.publish(correction.clone());
        probe.release();
        let (mut connection, frame) = resyncing.await.expect("join resync");
        let Some(RuntimeFrame::Resync {
            snapshot,
            cursor,
            reason,
            ..
        }) = frame
        else {
            panic!("expected resync");
        };
        assert_eq!(reason, "lagged");
        assert_eq!(probe.attempts(), 2);
        assert_eq!(snapshot.task.state_version, 1);
        assert_eq!(snapshot.task.status, TaskStatus::Active);
        assert!(cursor < correction.cursor);
        match connection.next_frame().await {
            Some(RuntimeFrame::Event { event, .. }) => {
                assert_eq!(event.event_id, correction.event_id);
                assert_eq!(event.data["stateVersion"], paused.state_version);
            }
            Some(RuntimeFrame::Resync { snapshot, cursor, .. }) => {
                assert_eq!(snapshot.task.state_version, paused.state_version);
                assert_eq!(snapshot.task.status, TaskStatus::Paused);
                assert!(cursor >= correction.cursor);
            }
            Some(RuntimeFrame::Snapshot { .. }) => panic!("unexpected second initial snapshot"),
            None => panic!("stream closed before committed Task correction"),
        }
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
        let Some(RuntimeFrame::Event { event, .. }) = connection.next_frame().await else {
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

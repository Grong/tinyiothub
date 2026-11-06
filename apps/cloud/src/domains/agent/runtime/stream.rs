use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use serde::Serialize;
use tinyiothub_storage::Db;
use tinyiothub_storage::runtime_event::{RuntimeEvent, RuntimeEventInput, TaskProjection};
use tokio::sync::broadcast::Sender;

// The legacy platform-backed Task SSE (`connect`/`stream_task`) was retired in
// Task 19C4 together with the platform Task fact plane. The routed stream is
// `harness_stream::HarnessStreamHub`; this hub remains the recorder fan-out
// used by device, watch, and media-evidence telemetry.

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
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Event { .. } => "event",
            Self::Resync { .. } => "resync",
        }
    }
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
    #[allow(dead_code)] // only the test-only subscribe path creates channels now
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

    #[cfg(test)]
    fn sender(&self, workspace_id: &str) -> Sender<RuntimeEvent> {
        self.senders
            .entry(workspace_id.to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(self.capacity).0)
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

    #[cfg(test)]
    pub(crate) fn subscribe(&self, workspace_id: &str) -> tokio::sync::broadcast::Receiver<RuntimeEvent> {
        self.sender(workspace_id).subscribe()
    }
}

impl Default for RuntimeStreamHub {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use serde_json::json;
    use tinyiothub_storage::Db;
    use tinyiothub_storage::runtime_event::RuntimeEventInput;

    use super::{RuntimeRecorder, RuntimeStreamHub};

    async fn fixture() -> (Arc<Db>, Arc<RuntimeStreamHub>) {
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
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-1','workspace','t1','now','now')",
        )
        .execute(&pool)
        .await
        .expect("workspace");
        (Arc::new(Db::new(pool)), Arc::new(RuntimeStreamHub::with_capacity(16)))
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
    async fn recorder_persists_once_and_publishes_stable_event_identity() {
        let (db, hub) = fixture().await;
        let recorder = RuntimeRecorder::new(db.clone(), hub.clone());
        let mut events = hub.subscribe("ws-1");

        let first = recorder.record(input("event-1")).await.expect("first");
        let published = events.recv().await.expect("published first");
        assert_eq!(published, first);

        let duplicate = recorder.record(input("event-1")).await.expect("duplicate");
        assert_eq!(duplicate.cursor, first.cursor);
        assert_eq!(duplicate.event_id, first.event_id);
        let persisted = db
            .runtime_event_by_id("ws-1", "event-1")
            .await
            .expect("persisted")
            .expect("event");
        assert_eq!(persisted, first);
    }

    #[tokio::test]
    async fn publish_without_a_subscriber_is_a_noop() {
        let (_db, hub) = fixture().await;
        hub.publish(super::RuntimeEvent {
            cursor: 1,
            event_id: "event-1".into(),
            event_type: "task.updated".into(),
            occurred_at: Utc::now(),
            workspace_id: "ws-1".into(),
            task_id: None,
            run_id: None,
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: json!({}),
        });
    }
}

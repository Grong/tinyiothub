//! Cloud-side `ThingAgentHost` implementation (T6 partial).
//!
//! This task implements the event-plane capabilities:
//! - `subscribe_events` — subscribe to the global [`ThingEventBus`];
//! - `replay_events_since` — cursor compensation against the `events` table
//!   (rowid cursor + `min_level` filter, thing-sourced rows only, O27).
//!
//! The chat/alert/session capabilities are wired in Task 13 and stubbed here.

use std::sync::Arc;

use sqlx::Row;
use tinyiothub_ai::thing_agent::{ThingAgentHost, ThingEventSignal};

use crate::modules::event::bus::ThingEventBus;

pub struct CloudThingAgentHost {
    pool: sqlx::SqlitePool,
    bus: Arc<ThingEventBus>,
}

impl CloudThingAgentHost {
    pub fn new(pool: sqlx::SqlitePool, bus: Arc<ThingEventBus>) -> Self {
        Self { pool, bus }
    }
}

#[async_trait::async_trait]
impl ThingAgentHost for CloudThingAgentHost {
    fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
        self.bus.subscribe()
    }

    async fn replay_events_since(
        &self,
        cursor: i64,
        min_level: i32,
    ) -> anyhow::Result<Vec<ThingEventSignal>> {
        // The UUID `events.id` is not orderable — the cursor is the implicit
        // SQLite rowid, which is monotonic for appends (retention deletes
        // never lower max(rowid)).
        let rows = sqlx::query(
            "SELECT rowid AS rid, workspace_id, device_id, event_subtype, event_level, content, metadata, actor \
             FROM events \
             WHERE rowid > ? AND event_level >= ? AND source_type = 'thing' \
             ORDER BY rowid ASC",
        )
        .bind(cursor)
        .bind(min_level)
        .fetch_all(&self.pool)
        .await?;

        let signals = rows
            .iter()
            .filter_map(|row| {
                let workspace_id: Option<String> = row.get("workspace_id");
                let thing_id: Option<String> = row.get("device_id");
                let metadata: Option<String> = row.get("metadata");
                let is_unknown = metadata
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok())
                    .and_then(|v| v.get("unknown_event")?.as_bool())
                    .unwrap_or(false);
                let content: String = row.get("content");
                Some(ThingEventSignal {
                    workspace_id: workspace_id?,
                    thing_id: thing_id?,
                    event_name: row.get("event_subtype"),
                    event_id: row.get("rid"),
                    level: row.get("event_level"),
                    data: serde_json::from_str(&content).unwrap_or(serde_json::Value::Null),
                    is_unknown,
                    actor: row.get("actor"),
                })
            })
            .collect();
        Ok(signals)
    }

    async fn push_chat_message(
        &self,
        _session_key: &str,
        _content: &str,
        _run_id: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("push_chat_message is wired in Task 13")
    }

    async fn notify_alert(
        &self,
        _workspace_id: &str,
        _payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        anyhow::bail!("notify_alert is wired in Task 13")
    }

    async fn recent_active_admin_session(
        &self,
        _workspace_id: &str,
    ) -> anyhow::Result<Option<String>> {
        anyhow::bail!("recent_active_admin_session is wired in Task 13")
    }
}

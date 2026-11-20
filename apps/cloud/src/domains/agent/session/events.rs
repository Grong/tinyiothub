//! SessionEvent — 唯一 UI 时间线的写入面。
//!
//! 复用 2026-11-03 的 `session_events` 表（每 workspace Harness DB）。本模块只
//! 负责：生成 `HarnessEventInput`、写入 DB、唤醒 SSE tail。事件名必须来自
//! spec §5.4.3 白名单。

use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};
use tinyiothub_core::harness::{ExecutionMode, HarnessEvent, HarnessEventInput};
use tinyiothub_storage::{Db, DbError};
use uuid::Uuid;

use super::context::TurnContext;
use crate::domains::agent::runtime::session_live::{SessionLiveFrame, SessionLiveHub};
use crate::domains::agent::runtime::session_stream::SessionStreamHub;

pub const SESSION_EVENT_SCHEMA_V: i64 = 1;

#[async_trait::async_trait]
pub trait SessionEventSink: Send + Sync {
    async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError>;

    /// Post-commit SSE hint. The database is the high watermark clients resume
    /// from, so a notification failure never changes committed facts.
    async fn notify(&self, _workspace_id: &str, _session_id: &str) -> Result<(), DbError> {
        Ok(())
    }

    /// Process-local presentation delta. This is never persisted and may be
    /// lost on disconnect; durable settlements are written with `emit`.
    async fn live(&self, _session_id: &str, _frame: SessionLiveFrame) {}
}

pub struct DbSessionEventSink {
    db: Arc<Db>,
    hub: Arc<SessionStreamHub>,
    live: Arc<SessionLiveHub>,
}

impl DbSessionEventSink {
    pub fn new(db: Arc<Db>, hub: Arc<SessionStreamHub>, live: Arc<SessionLiveHub>) -> Self {
        Self { db, hub, live }
    }
}

#[async_trait::async_trait]
impl SessionEventSink for DbSessionEventSink {
    async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError> {
        let event = self.db.append_session_event(&input).await?;
        self.hub.notify(&input.workspace_id);
        Ok(event)
    }

    async fn notify(&self, workspace_id: &str, _session_id: &str) -> Result<(), DbError> {
        self.hub.notify(workspace_id);
        Ok(())
    }

    async fn live(&self, session_id: &str, frame: SessionLiveFrame) {
        self.live.publish(session_id, frame);
    }
}

/// 构造一条 Session 事件。`event_id` 每次新生成；同一 payload 的 digest 稳定，
/// 因此重放/重试不会产生冲突（DB 以 event_id 幂等）。
pub fn session_event(ctx: &TurnContext, event_type: &str, payload: serde_json::Value) -> HarnessEventInput {
    let mut payload = payload;
    if payload.get("turnId").is_none() {
        payload["turnId"] = serde_json::json!(ctx.turn_id);
    }
    HarnessEventInput {
        event_id: Uuid::new_v4().to_string(),
        workspace_id: ctx.workspace_id.clone(),
        session_id: ctx.session_key.clone(),
        execution_mode: ExecutionMode::Production,
        run_id: Some(ctx.run_id.clone()),
        task_id: None,
        event_type: event_type.to_string(),
        occurred_at: Utc::now(),
        cause_id: None,
        schema_v: SESSION_EVENT_SCHEMA_V,
        payload_digest: payload_digest(&payload),
        payload_json: payload,
    }
}

fn payload_digest(payload: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(payload).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// spec §5.4.3：`session/start { session_id, kind }` 的统一构造。
pub fn session_started_payload(session_id: &str, kind: &str) -> serde_json::Value {
    serde_json::json!({ "sessionId": session_id, "kind": kind })
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use tokio::sync::Mutex;

    #[derive(Default)]
    pub(crate) struct RecordingSink {
        pub(crate) events: Mutex<Vec<HarnessEventInput>>,
    }

    #[async_trait::async_trait]
    impl SessionEventSink for RecordingSink {
        async fn emit(&self, input: HarnessEventInput) -> Result<HarnessEvent, DbError> {
            let mut events = self.events.lock().await;
            let event = HarnessEvent {
                seq: events.len() as i64 + 1,
                event_id: input.event_id.clone(),
                workspace_id: input.workspace_id.clone(),
                session_id: input.session_id.clone(),
                execution_mode: input.execution_mode,
                run_id: input.run_id.clone(),
                task_id: input.task_id.clone(),
                event_type: input.event_type.clone(),
                occurred_at: input.occurred_at,
                cause_id: input.cause_id,
                schema_v: input.schema_v,
                payload_digest: input.payload_digest.clone(),
                payload_json: input.payload_json.clone(),
            };
            events.push(input);
            Ok(event)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use chrono::Utc;
    use serde_json::json;

    fn ctx() -> TurnContext {
        TurnContext::new(
            "ws".to_string(),
            "agent_main".to_string(),
            "agent:ws:agent_main/s1".to_string(),
            "run1".to_string(),
            0,
        )
    }

    #[test]
    fn event_carries_turn_identity_and_schema() {
        let context = ctx();
        let input = session_event(&context, "todo/write", json!({"todos": []}));
        assert_eq!(input.workspace_id, "ws");
        assert_eq!(input.session_id, "agent:ws:agent_main/s1");
        assert_eq!(input.run_id, Some("run1".to_string()));
        assert_eq!(input.event_type, "todo/write");
        assert_eq!(input.schema_v, 1);
        assert_eq!(input.execution_mode, ExecutionMode::Production);
        assert_eq!(input.task_id, None);
        assert_eq!(input.cause_id, None);
        assert!(!input.event_id.is_empty());
        assert!(input.occurred_at <= Utc::now());
    }

    #[test]
    fn same_payload_has_stable_digest_but_new_event_id() {
        let context = ctx();
        let a = session_event(&context, "todo/write", json!({"todos": []}));
        let b = session_event(&context, "todo/write", json!({"todos": []}));
        assert_eq!(a.payload_digest, b.payload_digest);
        assert_ne!(a.event_id, b.event_id);
    }

    #[tokio::test]
    async fn recording_sink_captures_events() {
        let sink = test_support::RecordingSink::default();
        let context = ctx();
        sink.emit(session_event(&context, "turn/start", json!({})))
            .await
            .unwrap();
        let events = sink.events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "turn/start");
    }

    #[test]
    fn session_started_payload_matches_spec() {
        let payload = session_started_payload("agent:ws:a/s1", "scheduled");
        assert_eq!(payload["sessionId"], "agent:ws:a/s1");
        assert_eq!(payload["kind"], "scheduled");
    }
}

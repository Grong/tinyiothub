//! Host capabilities the thing-agent loop needs from the cloud process
//! (HeartbeatTaskRepository 先例 —— cloud 注入能力的抽象).

/// In-process signal emitted for every persisted thing event.
///
/// `actor == "agent"` marks events produced by agent actions (invoke_action
/// dispatch / heartbeat autonomous actions) — consumers must not wake the
/// loop on those (resonance guard, O21).
#[derive(Debug, Clone)]
pub struct ThingEventSignal {
    pub workspace_id: String,
    pub thing_id: String,
    pub event_name: String,
    /// Monotonic cursor (events.rowid) — NOT the UUID `events.id`, which is
    /// not orderable.
    pub event_id: i64,
    pub level: i32,
    pub data: serde_json::Value,
    pub is_unknown: bool,
    pub actor: String,
}

#[async_trait::async_trait]
pub trait ThingAgentHost: Send + Sync {
    /// 订阅全局事件广播（容量 256，lag 时调用方走 replay 补偿，O27）
    fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal>;
    /// 游标补偿：拉取 event_id > cursor 的事件（lag/重启恢复）
    async fn replay_events_since(&self, cursor: i64, min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>>;
    async fn push_chat_message(&self, session_key: &str, content: &str, run_id: &str) -> anyhow::Result<()>;
    async fn notify_alert(&self, workspace_id: &str, payload: serde_json::Value) -> anyhow::Result<()>;
    /// 工作区 admin 最近活跃会话（30 天内有消息），无则 None（O28）
    async fn recent_active_admin_session(&self, workspace_id: &str) -> anyhow::Result<Option<String>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-level contract check: a minimal host implementation must be
    /// able to satisfy the trait and round-trip a signal through a broadcast
    /// channel exactly as `subscribe_events` exposes it.
    struct NoopHost {
        tx: tokio::sync::broadcast::Sender<ThingEventSignal>,
    }

    #[async_trait::async_trait]
    impl ThingAgentHost for NoopHost {
        fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
            self.tx.subscribe()
        }

        async fn replay_events_since(&self, _cursor: i64, _min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
            Ok(vec![])
        }

        async fn push_chat_message(&self, _session_key: &str, _content: &str, _run_id: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn notify_alert(&self, _workspace_id: &str, _payload: serde_json::Value) -> anyhow::Result<()> {
            Ok(())
        }

        async fn recent_active_admin_session(&self, _workspace_id: &str) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn subscribe_events_round_trips_signal() {
        let (tx, _) = tokio::sync::broadcast::channel(256);
        let host = NoopHost { tx: tx.clone() };
        let mut rx = host.subscribe_events();

        let signal = ThingEventSignal {
            workspace_id: "ws-1".to_string(),
            thing_id: "thing-1".to_string(),
            event_name: "temp_high".to_string(),
            event_id: 42,
            level: 3,
            data: serde_json::json!({"value": 42}),
            is_unknown: false,
            actor: "device".to_string(),
        };
        tx.send(signal).expect("send");

        let got = rx.recv().await.expect("recv");
        assert_eq!(got.workspace_id, "ws-1");
        assert_eq!(got.thing_id, "thing-1");
        assert_eq!(got.event_name, "temp_high");
        assert_eq!(got.event_id, 42);
        assert_eq!(got.level, 3);
        assert_eq!(got.data, serde_json::json!({"value": 42}));
        assert!(!got.is_unknown);
        assert_eq!(got.actor, "device");
    }

    #[tokio::test]
    async fn replay_returns_empty_for_noop_host() {
        let (tx, _) = tokio::sync::broadcast::channel(256);
        let host = NoopHost { tx };
        let events = host.replay_events_since(0, 1).await.expect("replay");
        assert!(events.is_empty());
    }
}

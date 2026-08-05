//! Host capabilities the thing-agent loop needs from the cloud process
//! (HeartbeatTaskRepository 先例 —— cloud 注入能力的抽象).

/// In-process signal emitted for every persisted thing event.
///
/// Defined in `tinyiothub_event::bus` (the producing domain) and re-exported
/// here so existing `thing_agent::ThingEventSignal` paths keep working.
///
/// `actor == "agent"` marks events produced by agent actions (invoke_action
/// dispatch / heartbeat autonomous actions) — consumers must not wake the
/// loop on those (resonance guard, O21).
pub use tinyiothub_event::bus::ThingEventSignal;

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

/// 用户指令投递入口（T14）—— cloud 侧 HTTP 端点与 chat 工具经此向
/// thing-agent loop 投递 [`WakeSignal`]。cloud 持有一个
/// `Arc<dyn DirectiveSink>`：T15 的 ThingAgentManager（进程内
/// per-workspace 注册表）实现本 trait 并按 workspace 路由到对应
/// SchedulerHandle；[`crate::thing_agent::scheduler::SchedulerHandle`]
/// 本身也实现它（单工作区直连，测试/桩用）。
#[async_trait::async_trait]
pub trait DirectiveSink: Send + Sync {
    /// 投递唤醒信号。语义与
    /// [`crate::thing_agent::scheduler::SchedulerHandle::enqueue`] 相同：
    /// 队列满拒绝用户指令（Rejected）、60s 同文去重（Duplicate）等
    /// 由实现方保证。
    fn enqueue(
        &self,
        signal: crate::thing_agent::types::WakeSignal,
    ) -> Result<(), crate::thing_agent::scheduler::EnqueueError>;

    /// O26 kill switch：清空该工作区调度器的待处理队列（ready queue + 未
    /// flush 的合并窗口）。语义与
    /// [`crate::thing_agent::scheduler::SchedulerHandle::drain`] 相同：不取消
    /// 在跑的 run（返回时其已完成），drain 之后新入队的信号不受影响。
    /// 未知工作区为 no-op。默认实现为 no-op（测试桩无需关心）。
    async fn drain(&self, _workspace_id: &str) {}
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

    // T14: SchedulerHandle must satisfy the DirectiveSink trait so cloud
    // endpoints / chat tools can dispatch user directives through
    // Arc<dyn DirectiveSink> (T15 swaps in the ThingAgentManager).
    #[tokio::test]
    async fn scheduler_handle_is_a_directive_sink() {
        use crate::thing_agent::scheduler::Scheduler;
        use crate::thing_agent::types::{Priority, TriggerSource, WakeSignal};

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = Scheduler::spawn("ws_01".to_string(), move |sig| {
            let tx = tx.clone();
            Box::pin(async move {
                let _ = tx.send(sig);
            })
        });

        let sink: &dyn DirectiveSink = &handle;
        sink.enqueue(WakeSignal {
            workspace_id: "ws_01".to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "重启网关".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        })
        .expect("directive accepted");

        let sig = rx.recv().await.expect("directive runs");
        match &sig.source {
            TriggerSource::UserDirective { text, .. } => assert_eq!(text, "重启网关"),
            other => panic!("expected UserDirective, got {other:?}"),
        }

        // Error semantics pass through the trait object: a duplicate
        // same-text directive within 60s is rejected as Duplicate.
        let dup = sink.enqueue(WakeSignal {
            workspace_id: "ws_01".to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "重启网关".to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        });
        assert_eq!(dup, Err(crate::thing_agent::scheduler::EnqueueError::Duplicate));
    }
}

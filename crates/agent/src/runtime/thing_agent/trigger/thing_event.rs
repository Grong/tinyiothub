//! Thing event trigger — filters broadcast thing events into wake signals.
//!
//! Behavior rules (T7 brief):
//! ① `level < min_wake_level` (default 3=warning) → ignore
//! ② `is_unknown` → ignore
//! ③ `actor == "agent"` → ignore (resonance guard, O21)
//! ④ autonomy mode = off (or no policy) → ignore (zero LLM cost, O6/O19)
//! ⑤ `level >= 5` (critical) → emit immediately with `Priority::Critical`
//!    and `dedup_key = "thing:{id}:event:{name}"` — the scheduler skips the
//!    merge window for Critical (O10); this trigger only marks the priority
//! ⑥ everything else → emit `Priority::Normal` signal to join the merge window
//! ⑦ broadcast `RecvError::Lagged` → log `agent_wake_dropped` metric +
//!    `replay_events_since(cursor)` catch-up (O27)
//!
//! Cursor invariant: the cursor is `max(cursor, signal.event_id)`, never
//! "last seen" — concurrent broadcasts may arrive out of rowid order.

use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;

use super::Trigger;
use crate::runtime::thing_agent::traits::{AutonomyPolicyReader, ThingAgentHost, ThingEventSignal};
use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};
use tinyiothub_policy::autonomy::AutonomyMode;

/// Wakes the thing-agent loop on noteworthy thing events from one workspace.
pub struct ThingEventTrigger {
    host: Arc<dyn ThingAgentHost>,
    policy_repo: Arc<dyn AutonomyPolicyReader>,
    workspace_id: String,
    min_wake_level: i32,
}

impl ThingEventTrigger {
    pub fn new(
        host: Arc<dyn ThingAgentHost>,
        policy_repo: Arc<dyn AutonomyPolicyReader>,
        workspace_id: impl Into<String>,
        min_wake_level: i32,
    ) -> Self {
        Self {
            host,
            policy_repo,
            workspace_id: workspace_id.into(),
            min_wake_level,
        }
    }

    /// In-memory filters ①②③ + workspace scoping. Cheap checks only; the
    /// policy check (④) runs per accepted candidate so the kill switch takes
    /// effect without a trigger restart.
    fn passes_filters(&self, ev: &ThingEventSignal) -> bool {
        ev.workspace_id == self.workspace_id && !ev.is_unknown && ev.actor != "agent" && ev.level >= self.min_wake_level
    }

    fn to_wake_signal(&self, ev: &ThingEventSignal) -> WakeSignal {
        // ⑤ critical直通 — scheduler skips the merge window on Critical.
        let priority = if ev.level >= 5 {
            Priority::Critical
        } else {
            Priority::Normal
        };
        WakeSignal {
            workspace_id: self.workspace_id.clone(),
            priority,
            source: TriggerSource::ThingEvent {
                thing_id: ev.thing_id.clone(),
                event_name: ev.event_name.clone(),
                event_id: ev.event_id,
                level: ev.level,
                data: ev.data.clone(),
            },
            dedup_key: Some(format!("thing:{}:event:{}", ev.thing_id, ev.event_name)),
        }
    }

    /// ④ + send. Returns false when the wake channel is closed (agent loop
    /// shut down) so the caller exits the run loop.
    async fn gate_and_send(&self, ev: &ThingEventSignal, tx: &mpsc::Sender<WakeSignal>) -> bool {
        match self.policy_repo.load_autonomy(&self.workspace_id).await {
            Ok(Some(policy)) if policy.mode != AutonomyMode::Off => {}
            // ④ mode=off / no policy row → no wake, zero LLM cost (O6/O19).
            Ok(_) => return true,
            Err(e) => {
                // Fail-closed: a policy read error must not wake the loop.
                tracing::warn!(
                    workspace_id = %self.workspace_id,
                    error = %e,
                    "autonomy policy read failed — skipping wake signal"
                );
                return true;
            }
        }
        tx.send(self.to_wake_signal(ev)).await.is_ok()
    }
}

#[async_trait::async_trait]
impl Trigger for ThingEventTrigger {
    fn name(&self) -> &'static str {
        "thing_event"
    }

    async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> anyhow::Result<()> {
        let mut rx = self.host.subscribe_events();
        // High-water mark over every event seen (live or replayed). Uses
        // max() because concurrent broadcasts may arrive out of rowid order.
        let mut cursor: i64 = 0;
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    cursor = cursor.max(ev.event_id);
                    if !self.passes_filters(&ev) {
                        continue;
                    }
                    if !self.gate_and_send(&ev, &tx).await {
                        return Ok(());
                    }
                }
                Err(RecvError::Lagged(dropped)) => {
                    // ⑦ O27: log the drop metric and catch up from the cursor.
                    tracing::warn!(
                        metric = "agent_wake_dropped",
                        workspace_id = %self.workspace_id,
                        dropped,
                        cursor,
                        "thing event broadcast lagged — replaying from cursor"
                    );
                    let replayed = match self.host.replay_events_since(cursor, self.min_wake_level).await {
                        Ok(events) => events,
                        Err(e) => {
                            // Keep the trigger alive; the next lag retries
                            // from the same cursor.
                            tracing::error!(
                                workspace_id = %self.workspace_id,
                                error = %e,
                                cursor,
                                "replay_events_since failed"
                            );
                            continue;
                        }
                    };
                    for ev in &replayed {
                        cursor = cursor.max(ev.event_id);
                        // Replay results are re-filtered here as a second
                        // line of defense (host replay has no LIMIT, known minor).
                        if !self.passes_filters(ev) {
                            continue;
                        }
                        if !self.gate_and_send(ev, &tx).await {
                            return Ok(());
                        }
                    }
                }
                Err(RecvError::Closed) => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::Duration;

    use tinyiothub_policy::autonomy::AutonomyMode;

    use crate::runtime::thing_agent::traits::test_stubs::{StubAutonomyPolicyReader, policy};

    const WS: &str = "ws_01";

    fn ev(event_id: i64, level: i32) -> ThingEventSignal {
        ThingEventSignal {
            workspace_id: WS.to_string(),
            thing_id: "t1".to_string(),
            event_name: "temp_high".to_string(),
            event_id,
            level,
            data: serde_json::json!({"value": 42}),
            is_unknown: false,
            actor: "device".to_string(),
        }
    }

    /// 内存桩（Task 13 起替代真实 SQLite repo 夹具）：播种指定模式。
    fn stub_repo(ws: &str, mode: AutonomyMode) -> Arc<StubAutonomyPolicyReader> {
        Arc::new(StubAutonomyPolicyReader::with_policy(ws, mode))
    }

    struct StubHost {
        tx: tokio::sync::broadcast::Sender<ThingEventSignal>,
        pre_rx: Mutex<Option<tokio::sync::broadcast::Receiver<ThingEventSignal>>>,
        replay_queue: Mutex<VecDeque<Vec<ThingEventSignal>>>,
        flood_on_replay: Mutex<Vec<ThingEventSignal>>,
        replay_calls: Mutex<Vec<(i64, i32)>>,
    }

    impl StubHost {
        fn new(capacity: usize) -> Self {
            let (tx, _) = tokio::sync::broadcast::channel(capacity);
            Self {
                tx,
                pre_rx: Mutex::new(None),
                replay_queue: Mutex::new(VecDeque::new()),
                flood_on_replay: Mutex::new(vec![]),
                replay_calls: Mutex::new(vec![]),
            }
        }

        /// Subscribe BEFORE sending `backlog` so the stored receiver starts
        /// out lagged once backlog exceeds channel capacity.
        fn with_pre_lagged_backlog(self, backlog: Vec<ThingEventSignal>) -> Self {
            let rx = self.tx.subscribe();
            for e in backlog {
                self.tx.send(e).expect("send backlog");
            }
            *self.pre_rx.lock().expect("pre_rx lock") = Some(rx);
            self
        }

        fn replay_calls(&self) -> Vec<(i64, i32)> {
            self.replay_calls.lock().expect("replay_calls lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl ThingAgentHost for StubHost {
        fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
            self.pre_rx
                .lock()
                .expect("pre_rx lock")
                .take()
                .unwrap_or_else(|| self.tx.subscribe())
        }

        async fn replay_events_since(&self, cursor: i64, min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
            self.replay_calls
                .lock()
                .expect("replay_calls lock")
                .push((cursor, min_level));
            // Simulate new live events arriving while the consumer is busy
            // replaying — with a small channel this forces a second lag.
            let flood = std::mem::take(&mut *self.flood_on_replay.lock().expect("flood lock"));
            for e in flood {
                let _ = self.tx.send(e);
            }
            Ok(self
                .replay_queue
                .lock()
                .expect("replay_queue lock")
                .pop_front()
                .unwrap_or_default())
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

    fn spawn_trigger(
        host: Arc<StubHost>,
        repo: Arc<StubAutonomyPolicyReader>,
        min_wake_level: i32,
    ) -> (mpsc::Receiver<WakeSignal>, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let trigger = ThingEventTrigger::new(host, repo, WS, min_wake_level);
        assert_eq!(trigger.name(), "thing_event");
        let (tx, rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trigger.run(tx).await });
        (rx, handle)
    }

    async fn wait_subscribed(host: &StubHost) {
        for _ in 0..1000 {
            if host.tx.receiver_count() > 0 {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("trigger did not subscribe");
    }

    async fn next_signal(rx: &mut mpsc::Receiver<WakeSignal>) -> WakeSignal {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for wake signal")
            .expect("wake channel closed")
    }

    fn source_event_id(signal: &WakeSignal) -> i64 {
        match &signal.source {
            TriggerSource::ThingEvent { event_id, .. } => *event_id,
            other => panic!("expected ThingEvent source, got {other:?}"),
        }
    }

    /// Drop the wake receiver and nudge the trigger with one more event so
    /// its send fails and the run loop exits cleanly.
    async fn shutdown(
        host: &StubHost,
        rx: mpsc::Receiver<WakeSignal>,
        handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    ) {
        drop(rx);
        host.tx.send(ev(999, 3)).expect("send shutdown nudge");
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("trigger did not stop")
            .expect("run task panicked")
            .expect("run returned error");
    }

    // ① level < min_wake_level → ignored
    #[tokio::test]
    async fn below_min_wake_level_is_ignored() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);
        wait_subscribed(&host).await;

        host.tx.send(ev(1, 2)).expect("send low-level event");
        host.tx.send(ev(2, 3)).expect("send sentinel");

        let signal = next_signal(&mut rx).await;
        assert_eq!(source_event_id(&signal), 2, "level 2 event must not wake");
        assert!(rx.try_recv().is_err());

        shutdown(&host, rx, handle).await;
    }

    // ② is_unknown → ignored
    #[tokio::test]
    async fn unknown_event_is_ignored() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);
        wait_subscribed(&host).await;

        let mut unknown = ev(1, 4);
        unknown.is_unknown = true;
        host.tx.send(unknown).expect("send unknown event");
        host.tx.send(ev(2, 3)).expect("send sentinel");

        let signal = next_signal(&mut rx).await;
        assert_eq!(source_event_id(&signal), 2, "unknown event must not wake");
        assert!(rx.try_recv().is_err());

        shutdown(&host, rx, handle).await;
    }

    // ③ actor == "agent" → ignored (resonance guard; not bypassed by critical)
    #[tokio::test]
    async fn agent_actor_is_ignored_even_at_critical_level() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);
        wait_subscribed(&host).await;

        let mut self_inflicted = ev(1, 5);
        self_inflicted.actor = "agent".to_string();
        host.tx.send(self_inflicted).expect("send agent-actor event");
        host.tx.send(ev(2, 3)).expect("send sentinel");

        let signal = next_signal(&mut rx).await;
        assert_eq!(source_event_id(&signal), 2, "agent-produced event must not wake");
        assert!(rx.try_recv().is_err());

        shutdown(&host, rx, handle).await;
    }

    // ④ mode=off → zero signals (O19), and policy is re-read per event
    #[tokio::test]
    async fn mode_off_emits_zero_signals_until_policy_changes() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Off);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo.clone(), 3);
        wait_subscribed(&host).await;

        host.tx.send(ev(1, 5)).expect("send event while off");

        // 真实库无法统计读取次数：给 trigger 一个真实时间窗口完成 event 1 的
        // Off 门控（内存 SQLite 读取为微秒级），再翻策略。
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Flip the kill switch back on; the next event must wake — proving
        // the off event above produced nothing and mode is re-read per event.
        repo.save(WS, policy(AutonomyMode::Act));
        host.tx.send(ev(2, 3)).expect("send sentinel");

        let signal = next_signal(&mut rx).await;
        assert_eq!(source_event_id(&signal), 2, "mode=off event must not wake");
        assert!(rx.try_recv().is_err());

        shutdown(&host, rx, handle).await;
    }

    // ⑤ level=5 critical → Priority::Critical pass-through with dedup key
    #[tokio::test]
    async fn critical_event_passes_through_with_critical_priority() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);
        wait_subscribed(&host).await;

        host.tx.send(ev(7, 5)).expect("send critical event");

        let signal = next_signal(&mut rx).await;
        assert_eq!(signal.priority, Priority::Critical);
        assert_eq!(signal.workspace_id, WS);
        assert_eq!(signal.dedup_key.as_deref(), Some("thing:t1:event:temp_high"));
        match &signal.source {
            TriggerSource::ThingEvent {
                thing_id,
                event_name,
                event_id,
                level,
                data,
            } => {
                assert_eq!(thing_id, "t1");
                assert_eq!(event_name, "temp_high");
                assert_eq!(*event_id, 7);
                assert_eq!(*level, 5);
                assert_eq!(*data, serde_json::json!({"value": 42}));
            }
            other => panic!("expected ThingEvent source, got {other:?}"),
        }

        shutdown(&host, rx, handle).await;
    }

    // ⑥ ordinary warning event → Priority::Normal, joins merge window
    #[tokio::test]
    async fn normal_event_emits_mergeable_signal() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);
        wait_subscribed(&host).await;

        host.tx.send(ev(3, 4)).expect("send warning event");

        let signal = next_signal(&mut rx).await;
        assert_eq!(signal.priority, Priority::Normal);
        assert_eq!(signal.dedup_key.as_deref(), Some("thing:t1:event:temp_high"));
        assert_eq!(source_event_id(&signal), 3);

        shutdown(&host, rx, handle).await;
    }

    // ⑦ broadcast lag → replay from cursor; replay results re-filtered
    #[tokio::test]
    async fn broadcast_lag_triggers_cursor_replay() {
        // Capacity 2, backlog of 5 → receiver lags by 3, retains ids 4,5.
        let host =
            Arc::new(StubHost::new(2).with_pre_lagged_backlog(vec![ev(1, 3), ev(2, 3), ev(3, 3), ev(4, 3), ev(5, 3)]));
        host.replay_queue
            .lock()
            .expect("replay_queue lock")
            .push_back(vec![ev(6, 4), ev(7, 4), ev(8, 2)]);
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);

        let mut ids = Vec::new();
        for _ in 0..4 {
            ids.push(source_event_id(&next_signal(&mut rx).await));
        }
        // Replayed 6,7 come first (id 8 is below min level — replay results
        // are re-filtered as a second line of defense), then retained live 4,5.
        assert_eq!(ids, vec![6, 7, 4, 5]);
        assert_eq!(host.replay_calls(), vec![(0, 3)]);
        assert!(rx.try_recv().is_err());

        shutdown(&host, rx, handle).await;
    }

    // ⑦b cursor must advance by max(), not "last seen" (out-of-order handoff)
    #[tokio::test]
    async fn cursor_advances_by_max_on_out_of_order_events() {
        // Capacity 2, backlog of 3 → receiver lags by 1, retains ids 2,3.
        let host = Arc::new(StubHost::new(2).with_pre_lagged_backlog(vec![ev(1, 3), ev(2, 3), ev(3, 3)]));
        {
            let mut queue = host.replay_queue.lock().expect("replay_queue lock");
            // Replay returns out-of-order ids: 10 then 5.
            queue.push_back(vec![ev(10, 4), ev(5, 4)]);
            queue.push_back(vec![]);
        }
        // While the trigger is busy in the first replay, 3 more live events
        // arrive → second lag (retains 12,13). The second replay call must
        // use cursor = max(10, 5) = 10, not 5.
        *host.flood_on_replay.lock().expect("flood lock") = vec![ev(11, 3), ev(12, 3), ev(13, 3)];
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);

        let mut ids = Vec::new();
        for _ in 0..4 {
            ids.push(source_event_id(&next_signal(&mut rx).await));
        }
        assert_eq!(ids, vec![10, 5, 12, 13]);
        assert_eq!(host.replay_calls(), vec![(0, 3), (10, 3)]);

        shutdown(&host, rx, handle).await;
    }

    // Events from other workspaces never wake this trigger.
    #[tokio::test]
    async fn other_workspace_event_is_ignored() {
        let host = Arc::new(StubHost::new(16));
        let repo = stub_repo(WS, AutonomyMode::Act);
        let (mut rx, handle) = spawn_trigger(host.clone(), repo, 3);
        wait_subscribed(&host).await;

        let mut foreign = ev(1, 5);
        foreign.workspace_id = "ws_other".to_string();
        host.tx.send(foreign).expect("send foreign event");
        host.tx.send(ev(2, 3)).expect("send sentinel");

        let signal = next_signal(&mut rx).await;
        assert_eq!(source_event_id(&signal), 2, "foreign workspace event must not wake");

        shutdown(&host, rx, handle).await;
    }
}

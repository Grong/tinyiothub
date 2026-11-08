//! TriggerRouter — 只产生 wake，不执行 Turn。
//!
//! 每 30 秒：确保所有 workspace 有 Schedule 行 → 扫描到期且启用的行 →
//! 每个 workspace 至多一个在途 wake → 交给 `WakeExecutor` 并推进 `next_wake_at`。

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use dashmap::{DashMap, DashSet};
use parking_lot::Mutex as ParkingMutex;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use tinyiothub_core::models::event::{Event, EventType};
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_storage::Db;
use tinyiothub_storage::assistant_schedule::AssistantSchedule;

use crate::domains::agent::event::types::AiEvent;
use crate::domains::agent::orchestrator::callbacks::extract_payload;
use crate::domains::agent::session::wake::{WakeExecutor, assistant_agent_id};

pub const TRIGGER_TICK_INTERVAL: Duration = Duration::from_secs(30);
const DUE_BATCH_LIMIT: i64 = 100;
/// 每 workspace + trigger 的窗口内最多派发次数（3/min 去重）。
const EVENT_RATE_LIMIT: usize = 3;
const EVENT_RATE_WINDOW: Duration = Duration::from_secs(60);

/// 告警等外部事件触发的一次 wake 请求。
#[derive(Debug, Clone)]
pub struct EventWake {
    pub workspace_id: String,
    pub trigger: String,
    pub source_id: String,
    pub summary: String,
}

/// 滑动窗口内已接受的 event 派发时间点。
struct RateWindow {
    hits: VecDeque<Instant>,
}

pub struct TriggerRouter {
    db: Arc<Db>,
    executor: Arc<dyn WakeExecutor>,
    in_flight: Arc<DashSet<String>>,
    shutdown: CancellationToken,
    event_bus: Option<Arc<EventBus>>,
    event_tx: mpsc::Sender<EventWake>,
    event_rx: ParkingMutex<Option<mpsc::Receiver<EventWake>>>,
    rate: DashMap<String, RateWindow>,
}

/// RAII guard for the per-workspace in-flight marker.
///
/// Removal happens in `Drop`, so a panicking (or cancelled) dispatch still
/// clears the marker instead of permanently disabling future wakes for the
/// workspace.
struct InFlightGuard {
    set: Arc<DashSet<String>>,
    key: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.set.remove(&self.key);
    }
}

impl TriggerRouter {
    /// 无事件总线版本（既有测试与不接报警的装配路径）。
    pub fn new(db: Arc<Db>, executor: Arc<dyn WakeExecutor>) -> Self {
        Self::new_with_event_bus(db, executor, None)
    }

    pub fn new_with_event_bus(db: Arc<Db>, executor: Arc<dyn WakeExecutor>, event_bus: Option<Arc<EventBus>>) -> Self {
        let (event_tx, event_rx) = mpsc::channel(256);
        Self {
            db,
            executor,
            in_flight: Arc::new(DashSet::new()),
            shutdown: CancellationToken::new(),
            event_bus,
            event_tx,
            event_rx: ParkingMutex::new(Some(event_rx)),
            rate: DashMap::new(),
        }
    }

    /// 提交一次事件 wake。同 workspace 在途时拒绝；同一 `{workspace}:{trigger}`
    /// 60 秒窗口内超过 3 次时拒绝。
    pub fn submit_event(&self, wake: EventWake) -> bool {
        if self.in_flight.contains(&wake.workspace_id) {
            return false;
        }
        let key = format!("{}:{}", wake.workspace_id, wake.trigger);
        let now = Instant::now();
        {
            let mut window = self
                .rate
                .entry(key)
                .or_insert_with(|| RateWindow { hits: VecDeque::new() });
            while window
                .hits
                .front()
                .is_some_and(|at| now.duration_since(*at) >= EVENT_RATE_WINDOW)
            {
                window.hits.pop_front();
            }
            if window.hits.len() >= EVENT_RATE_LIMIT {
                return false;
            }
            window.hits.push_back(now);
        }
        self.event_tx.try_send(wake).is_ok()
    }

    /// 扫描并派发到期的 wake；返回已派发任务的句柄（生产方忽略，测试可 await）。
    pub async fn tick(self: &Arc<Self>, now: DateTime<Utc>) -> Vec<tokio::task::JoinHandle<()>> {
        match self.db.find_all_workspace_ids().await {
            Ok(workspace_ids) => {
                for workspace_id in &workspace_ids {
                    let assistant_id = assistant_agent_id(workspace_id);
                    if let Err(error) = self
                        .db
                        .ensure_assistant_schedule(workspace_id, &assistant_id, now)
                        .await
                    {
                        tracing::warn!(%error, %workspace_id, "assistant schedule: ensure failed");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "assistant schedule: list workspaces failed");
                return Vec::new();
            }
        }

        let due = match self.db.list_due_assistant_schedules(now, DUE_BATCH_LIMIT).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "assistant schedule: due scan failed");
                return Vec::new();
            }
        };

        let mut handles = Vec::new();
        for schedule in due {
            if !schedule.enabled {
                continue;
            }
            if !self.in_flight.insert(schedule.workspace_id.clone()) {
                continue;
            }
            let guard = InFlightGuard {
                set: Arc::clone(&self.in_flight),
                key: schedule.workspace_id.clone(),
            };
            let router = Arc::clone(self);
            handles.push(tokio::spawn(async move {
                let _guard = guard;
                router.dispatch(schedule, now).await;
            }));
        }
        handles
    }

    async fn dispatch(self: Arc<Self>, schedule: AssistantSchedule, now: DateTime<Utc>) {
        let report = self.executor.execute(schedule.clone(), self.shutdown.clone()).await;
        let next_wake_at = now + ChronoDuration::seconds(i64::from(schedule.interval_seconds));
        if let Err(error) = self
            .db
            .mark_assistant_wake(&schedule.workspace_id, next_wake_at, now, report.outcome.as_str())
            .await
        {
            tracing::warn!(%error, workspace_id = %schedule.workspace_id, "assistant schedule: mark wake failed");
        }
    }

    async fn dispatch_event(self: Arc<Self>, wake: EventWake) {
        let cancel = self.shutdown.clone();
        let report = self.executor.execute_event(wake.clone(), cancel).await;
        tracing::info!(
            workspace_id = %wake.workspace_id,
            source_id = %wake.source_id,
            outcome = report.outcome.as_str(),
            "event wake dispatched"
        );
    }

    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<Result<(), crate::shared::error::Error>> {
        // 先同步订阅再 spawn，保证 `start` 返回时新订阅已存在——调用方随后
        // 关闭旧的报警桥，窗口内不丢告警（broadcast 会为已订阅 receiver 缓冲）。
        if let Some(bus) = self.event_bus.clone() {
            let receiver = bus.subscribe();
            let tx = self.event_tx.clone();
            tokio::spawn(subscribe_alarms(receiver, tx, self.shutdown.clone()));
        }
        let event_receiver = self.event_rx.lock().take();
        if let Some(mut receiver) = event_receiver {
            let router = Arc::clone(&self);
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = router.shutdown.cancelled() => return,
                        wake = receiver.recv() => {
                            let Some(wake) = wake else { return };
                            let workspace_id = wake.workspace_id.clone();
                            if !router.in_flight.insert(workspace_id.clone()) {
                                continue;
                            }
                            let guard = InFlightGuard {
                                set: Arc::clone(&router.in_flight),
                                key: workspace_id,
                            };
                            let router = Arc::clone(&router);
                            tokio::spawn(async move {
                                let _guard = guard;
                                router.dispatch_event(wake).await;
                            });
                        }
                    }
                }
            });
        }
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(TRIGGER_TICK_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = self.shutdown.cancelled() => return Ok::<(), crate::shared::error::Error>(()),
                    _ = ticker.tick() => {
                        let _ = self.tick(Utc::now()).await;
                    }
                }
            }
        })
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

/// 订阅 EventBus 上的 `AiEvent::AlarmCreated`，仅 Critical/Error 告警进入
/// Session Loop。订阅在调用方同步完成（本函数只消费）。
async fn subscribe_alarms(
    mut receiver: broadcast::Receiver<Event>,
    tx: mpsc::Sender<EventWake>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            event = receiver.recv() => {
                let event = match event {
                    Ok(event) => event,
                    // 慢消费者丢帧：跳过继续消费；总线关闭则退出。
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                if !matches!(event.event_type(), EventType::Ai(_)) {
                    continue;
                }
                let Some(text) = extract_payload(event.content()) else {
                    continue;
                };
                let Ok(ai) = serde_json::from_str::<AiEvent>(&text) else {
                    continue;
                };
                let AiEvent::AlarmCreated(alarm) = ai else {
                    continue;
                };
                if !matches!(alarm.severity.as_str(), "critical" | "error") {
                    continue;
                }
                let wake = EventWake {
                    workspace_id: alarm.workspace_id.clone(),
                    trigger: "alarm".to_string(),
                    source_id: alarm.id.clone(),
                    summary: format!("{}（{}，设备 {}）", alarm.message, alarm.severity, alarm.thing_id),
                };
                if tx.try_send(wake).is_err() {
                    tracing::warn!(
                        workspace_id = %alarm.workspace_id,
                        %alarm.id,
                        "alarm wake dropped: event queue full"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Mutex, Notify};

    use crate::domains::agent::session::wake::WakeReport;

    #[derive(Default)]
    struct RecordingExecutor {
        calls: AtomicUsize,
        workspaces: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for RecordingExecutor {
        async fn execute(&self, schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.workspaces.lock().await.push(schedule.workspace_id);
            WakeReport {
                session_key: "s".into(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
            }
        }
    }

    struct BlockingExecutor {
        calls: AtomicUsize,
        started: Notify,
        release: Notify,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for BlockingExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            self.release.notified().await;
            WakeReport {
                session_key: "s".into(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
            }
        }
    }

    struct SkippedExecutor;

    #[async_trait::async_trait]
    impl WakeExecutor for SkippedExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            WakeReport {
                session_key: String::new(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Skipped,
            }
        }
    }

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        Db::new(pool)
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[tokio::test]
    async fn tick_dispatches_due_and_advances() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()));

        let handles = router.tick(now).await;
        assert_eq!(handles.len(), 1);
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        let row = db.get_assistant_schedule("ws-1").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(now + chrono::Duration::seconds(300)));
        assert_eq!(row.last_outcome.as_deref(), Some("completed"));

        let handles = router.tick(now + chrono::Duration::seconds(299)).await;
        assert!(handles.is_empty(), "not due yet");
    }

    #[tokio::test]
    async fn skipped_wake_still_advances_schedule() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-skip", "__assistant__ws-skip", now)
            .await
            .unwrap();
        let router = Arc::new(TriggerRouter::new(db.clone(), Arc::new(SkippedExecutor)));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }

        let row = db.get_assistant_schedule("ws-skip").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(now + chrono::Duration::seconds(300)));
        assert_eq!(row.last_outcome.as_deref(), Some("skipped"));
    }

    #[tokio::test]
    async fn in_flight_suppresses_second_dispatch() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        let executor = Arc::new(BlockingExecutor {
            calls: AtomicUsize::new(0),
            started: Notify::new(),
            release: Notify::new(),
        });
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()));

        let mut handles = router.tick(now).await;
        assert_eq!(handles.len(), 1);
        executor.started.notified().await;

        let suppressed = router.tick(now).await;
        assert!(suppressed.is_empty(), "in-flight wake must suppress a second dispatch");

        executor.release.notify_one();
        handles.remove(0).await.unwrap();

        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        let row = db.get_assistant_schedule("ws-1").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(now + chrono::Duration::seconds(300)));
    }

    #[tokio::test]
    async fn shutdown_cancels_tick_loop_token() {
        let db = Arc::new(test_db().await);
        let router = TriggerRouter::new(db, Arc::new(RecordingExecutor::default()));
        router.shutdown();
        assert!(router.shutdown.is_cancelled());
    }

    #[tokio::test]
    async fn tick_runs_wake_end_to_end() {
        use crate::domains::agent::pool::AgentPool;
        use crate::domains::agent::session::context::TurnContextRegistry;
        use crate::domains::agent::session::events::test_support::RecordingSink;
        use crate::domains::agent::session::runner::SessionRunner;
        use crate::domains::agent::session::todos::TodoStore;
        use crate::domains::agent::session::wake::WakeRunner;
        use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
        use tinyiothub_agent::port::memory::NoopMemory;
        use tinyiothub_agent::port::observer::NoopObserver;
        use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};

        struct ScriptedProvider;

        #[async_trait::async_trait]
        impl ModelProvider for ScriptedProvider {
            async fn chat(
                &self,
                _request: ChatRequest<'_>,
                _model: &str,
                _temperature: Option<f64>,
            ) -> anyhow::Result<ChatResponse> {
                Ok(ChatResponse {
                    text: Some("ok".into()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                })
            }
        }

        impl Attributable for ScriptedProvider {
            fn role(&self) -> Role {
                Role::Provider(ProviderKind::Model(ModelProviderKind::Custom))
            }

            fn alias(&self) -> &str {
                "ScriptedProvider"
            }
        }

        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-e2e", "__assistant__ws-e2e", now)
            .await
            .unwrap();
        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                Arc::new(|| Ok(Box::new(ScriptedProvider))),
            )
            .unwrap(),
        );
        let sink = Arc::new(RecordingSink::default());
        let contexts = Arc::new(TurnContextRegistry::default());
        let runner = Arc::new(SessionRunner::new(sink.clone(), contexts));
        let wake = Arc::new(WakeRunner::from_parts(
            db.clone(),
            agent_pool,
            sink.clone(),
            Arc::new(TodoStore::default()),
            runner,
        ));
        let router = Arc::new(TriggerRouter::new(db.clone(), wake));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }

        let events = sink.events.lock().await;
        let types: Vec<&str> = events.iter().map(|event| event.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "trigger.received",
                "wake.dispatched",
                "session.started",
                "turn.started",
                "message.created",
                "turn.completed",
                "session.ended",
            ]
        );
        let session_key = events[2].payload_json["sessionId"]
            .as_str()
            .expect("session.started must carry sessionId")
            .to_string();
        assert_eq!(events[2].payload_json["kind"], "scheduled");
        assert_eq!(events.last().unwrap().payload_json["outcome"], "completed");
        drop(events);
        let session = db.get_session(&session_key).await.unwrap().unwrap();
        assert_eq!(session.kind, "scheduled");
        assert_eq!(session.status, "ended");
        let schedule = db.get_assistant_schedule("ws-e2e").await.unwrap().unwrap();
        assert_eq!(schedule.next_wake_at, Some(now + chrono::Duration::seconds(300)));
        assert_eq!(schedule.last_outcome.as_deref(), Some("completed"));
    }

    #[tokio::test]
    async fn event_wake_dedupes_while_in_flight_then_dispatches() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();

        struct BlockingExecutor {
            calls: AtomicUsize,
            started: Notify,
            release: Notify,
        }

        #[async_trait::async_trait]
        impl WakeExecutor for BlockingExecutor {
            async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
                WakeReport {
                    session_key: "s".into(),
                    outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
                }
            }
            async fn execute_event(&self, _wake: EventWake, _cancel: CancellationToken) -> WakeReport {
                self.started.notify_one();
                self.release.notified().await;
                self.calls.fetch_add(1, Ordering::SeqCst);
                WakeReport {
                    session_key: "e".into(),
                    outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
                }
            }
        }

        let executor = Arc::new(BlockingExecutor {
            calls: AtomicUsize::new(0),
            started: Notify::new(),
            release: Notify::new(),
        });
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()));
        let _handle = router.clone().start();

        assert!(router.submit_event(EventWake {
            workspace_id: "ws-1".into(),
            trigger: "alarm".into(),
            source_id: "alarm-1".into(),
            summary: "3 号楼温度过高".into(),
        }));
        executor.started.notified().await;
        assert!(!router.submit_event(EventWake {
            workspace_id: "ws-1".into(),
            trigger: "alarm".into(),
            source_id: "alarm-2".into(),
            summary: "another".into(),
        }));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
        executor.release.notify_one();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    }
}

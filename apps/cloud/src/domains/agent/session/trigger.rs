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
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use tinyiothub_core::models::event::{Event, EventType};
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_storage::Db;
use tinyiothub_storage::assistant_schedule::AssistantSchedule;
use tinyiothub_storage::suppression::{OUTCOME_AUDIT_FAILED, SuppressionMode, SyncPending};

use crate::domains::agent::event::types::AiEvent;
use crate::domains::agent::orchestrator::callbacks::extract_payload;
use crate::domains::agent::session::suppression::preflight::{PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS, PreflightGate};
use crate::domains::agent::session::wake::{
    ManualOperation, ManualPreflight, ManualStartError, ManualTurnStart, WakeExecutor, WakeOutcome, assistant_agent_id,
};

pub const TRIGGER_TICK_INTERVAL: Duration = Duration::from_secs(30);
/// Canary/enforce may delay a wake by up to this deterministic workspace jitter.
pub const WAKE_JITTER_MAX_SECONDS: i64 = 15;
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
    pub dedup_key: Option<String>,
}

impl EventWake {
    fn rate_key(&self) -> String {
        match &self.dedup_key {
            Some(key) => format!("{}:{}:{}", self.workspace_id, self.trigger, key),
            None => format!("{}:{}", self.workspace_id, self.trigger),
        }
    }
}

/// 滑动窗口内已接受的 event 派发时间点。
struct RateWindow {
    hits: VecDeque<Instant>,
}

/// Completion-time source. Production uses `Utc::now()`; tests pin it so
/// completion-based scheduling is deterministic without a fake clock server.
type RouterClock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub struct TriggerRouter {
    db: Arc<Db>,
    executor: Arc<dyn WakeExecutor>,
    in_flight: Arc<DashSet<String>>,
    shutdown: CancellationToken,
    event_bus: Option<Arc<EventBus>>,
    event_tx: mpsc::Sender<EventWake>,
    event_rx: ParkingMutex<Option<mpsc::Receiver<EventWake>>>,
    rate: DashMap<String, RateWindow>,
    clock: RouterClock,
    preflight_gate: Arc<PreflightGate>,
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
            clock: Arc::new(Utc::now),
            preflight_gate: crate::domains::agent::session::suppression::preflight::global_preflight_gate(),
        }
    }

    /// Inject the completion-time clock (tests).
    pub fn with_clock<F>(mut self, clock: F) -> Self
    where
        F: Fn() -> DateTime<Utc> + Send + Sync + 'static,
    {
        self.clock = Arc::new(clock);
        self
    }

    /// Inject the preflight admission gate (tests/composition).
    pub fn with_preflight_gate(mut self, gate: Arc<PreflightGate>) -> Self {
        self.preflight_gate = gate;
        self
    }

    /// Immediate backpressure answer for manual operations. Scheduled wakes
    /// stay coalesced by the workspace in-flight guard; this is the HTTP-facing
    /// admission check that must never wait for a slot.
    fn preflight_backpressure(&self) -> Option<u64> {
        (!self.preflight_gate.has_capacity()).then_some(PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS)
    }

    /// 提交一次事件 wake。同 workspace 在途时拒绝；同一
    /// `{workspace}:{trigger}:{dedup_key}`（无 dedup 时退化为
    /// `{workspace}:{trigger}`）60 秒窗口内超过 3 次时拒绝。
    pub fn submit_event(&self, wake: EventWake) -> bool {
        if self.in_flight.contains(&wake.workspace_id) {
            return false;
        }
        let key = wake.rate_key();
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
            // Coalescing arbiter: every due request for the same
            // `(workspaceId, plannedWakeAt)` maps to one workspace slot, so a
            // duplicate burst is dropped (including after a restart) instead
            // of queued. The planned wake's durable identity
            // (`PlannedWake::planned`) also carries the same planned time, so
            // recovery replays rather than creating a second Turn.
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
                router.dispatch(schedule).await;
            }));
        }
        handles
    }

    /// Execute one scheduled wake and advance its Schedule.
    ///
    /// `nextWakeAt = completion time + interval + jitter`. Off/shadow force
    /// jitter to zero, so their scheduling stays the previous behavior apart
    /// from moving the base from tick time to completion time (long Turns no
    /// longer race their own next slot).
    async fn dispatch(self: Arc<Self>, schedule: AssistantSchedule) {
        let report = self.executor.execute(schedule.clone(), self.shutdown.clone()).await;
        let completed_at = (self.clock)();
        let mode = match self.db.get_suppression_config(&schedule.workspace_id).await {
            Ok(Some(config)) => config.mode,
            Ok(None) => SuppressionMode::Off,
            Err(error) => {
                // Fail safe: unknown config means no jitter, preserving the
                // existing off/shadow timing rather than delaying silently.
                tracing::warn!(%error, workspace_id = %schedule.workspace_id, "assistant schedule: suppression mode read failed");
                SuppressionMode::Off
            }
        };
        let jitter_seconds = wake_jitter_seconds(&schedule.workspace_id, mode);
        let next_wake_at =
            completed_at + ChronoDuration::seconds(i64::from(schedule.interval_seconds) + jitter_seconds);
        let outcome = persisted_outcome(schedule.last_outcome.as_deref(), &report.outcome);
        match self
            .db
            .mark_assistant_wake(&schedule.workspace_id, next_wake_at, completed_at, &outcome)
            .await
        {
            Ok(()) => {
                // A committed skip leaves `SyncPending::Schedule` set until a
                // Schedule write succeeds. That write may be a later full Turn
                // (which overwrites `latest_outcome` without clearing the
                // flag), so clear the durable barrier whenever it is still set
                // and this workspace's Schedule write has succeeded. A failed
                // write keeps it for every worker to see.
                if !report.session_key.is_empty() {
                    match self
                        .db
                        .load_suppression_state(&schedule.workspace_id, &report.session_key)
                        .await
                    {
                        Ok(Some(state)) if state.schedule_sync_pending => {
                            if let Err(error) = self
                                .db
                                .clear_sync_pending(&schedule.workspace_id, &report.session_key, SyncPending::Schedule)
                                .await
                            {
                                tracing::warn!(
                                    %error,
                                    workspace_id = %schedule.workspace_id,
                                    "assistant schedule: clear schedule barrier failed"
                                );
                            }
                        }
                        Ok(_) => {}
                        Err(error) => tracing::warn!(
                            %error,
                            workspace_id = %schedule.workspace_id,
                            "assistant schedule: read schedule barrier failed"
                        ),
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, workspace_id = %schedule.workspace_id, "assistant schedule: mark wake failed");
            }
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

    /// Startup/restart recovery (ruling R16).
    ///
    /// Settle stale full/audit fences and reconcile committed skip barriers
    /// before the first schedule tick. Run independently per workspace so one
    /// corrupt store cannot block the others.
    pub async fn recover_suppression(self: &Arc<Self>) {
        let workspaces = match self.db.find_all_workspace_ids().await {
            Ok(workspaces) => workspaces,
            Err(error) => {
                tracing::warn!(%error, "suppression startup recovery: list workspaces failed");
                return;
            }
        };
        for workspace_id in workspaces {
            // Recovery settles durable fences, so it must exclude every other
            // dispatch for this workspace while it runs.
            if !self.in_flight.insert(workspace_id.clone()) {
                tracing::warn!(
                    %workspace_id,
                    "suppression startup recovery skipped: workspace wake is in flight"
                );
                continue;
            }
            let guard = InFlightGuard {
                set: Arc::clone(&self.in_flight),
                key: workspace_id.clone(),
            };
            let reports = self.executor.recover_suppression(&workspace_id).await;
            drop(guard);
            for report in reports {
                tracing::info!(
                    workspace_id = %report.workspace_id,
                    replayed_committed_skip = report.replayed_committed_skip,
                    settled_full_attempt = report.settled_full_attempt.as_deref().unwrap_or(""),
                    settled_audit_attempt = report.settled_audit_attempt.as_deref().unwrap_or(""),
                    cleared = ?report.cleared,
                    alerts = report.alerts.len(),
                    "suppression startup recovery settled a workspace"
                );
            }
        }
    }

    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<Result<(), crate::shared::error::Error>> {
        // 先同步订阅再 spawn，保证 `start` 返回时新订阅已存在——调用方随后
        // 关闭旧的报警桥，窗口内不丢告警（broadcast 会为已订阅 receiver 缓冲）。
        if let Some(bus) = self.event_bus.clone() {
            let receiver = bus.subscribe();
            tokio::spawn(subscribe_alarms(receiver, Arc::clone(&self), self.shutdown.clone()));
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
                                tracing::warn!(
                                    workspace_id = %workspace_id,
                                    source_id = %wake.source_id,
                                    "event wake dropped: workspace wake in flight"
                                );
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
            // R17: settle stale fences before the first tick can dispatch a
            // live Turn against them.
            self.recover_suppression().await;
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

    /// 共享关停令牌：随 `start` 启动的附加订阅（thing-event 桥等）与路由
    /// 生命周期一致。
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }
}

/// Read/write manual suppression operations exposed to HTTP handlers.
///
/// The TriggerRouter stays the only Schedule outcome writer. Manual operations
/// allocate their durable attempt before the 202 response and then execute
/// asynchronously through the same full-Turn/skip implementations; they never
/// advance `next_wake_at` or `last_outcome`.
#[async_trait::async_trait]
pub trait SuppressionOps: Send + Sync {
    async fn manual_preflight(&self, workspace_id: &str) -> Result<ManualPreflight, ManualStartError>;

    /// Current registry version, used to reject stale gate artifacts.
    ///
    /// Read-only; the default reuses the public preflight projection so a
    /// composition that already implements it needs no extra wiring.
    async fn registry_version(&self, workspace_id: &str) -> Result<String, ManualStartError> {
        Ok(self.manual_preflight(workspace_id).await?.registry_version)
    }

    /// Record a human-confirmed false skip through the real alert producer.
    /// Default: unwired.
    async fn record_confirmed_false_skip(
        &self,
        _workspace_id: &str,
        _turn_id: &str,
        _reviewer: &str,
        _root_cause: &str,
    ) -> Result<(), ManualStartError> {
        Err(ManualStartError::Unavailable(
            "confirmed false-skip recording is unavailable".to_string(),
        ))
    }

    /// Allocate the durable attempt synchronously, then spawn execution.
    async fn start_manual(
        &self,
        workspace_id: &str,
        operation: ManualOperation,
    ) -> Result<ManualTurnStart, ManualStartError>;
}

#[async_trait::async_trait]
impl SuppressionOps for TriggerRouter {
    async fn manual_preflight(&self, workspace_id: &str) -> Result<ManualPreflight, ManualStartError> {
        if let Some(retry_after_ms) = self.preflight_backpressure() {
            return Err(ManualStartError::Backpressure { retry_after_ms });
        }
        self.executor.manual_preflight(workspace_id).await
    }

    async fn start_manual(
        &self,
        workspace_id: &str,
        operation: ManualOperation,
    ) -> Result<ManualTurnStart, ManualStartError> {
        if let Some(retry_after_ms) = self.preflight_backpressure() {
            return Err(ManualStartError::Backpressure { retry_after_ms });
        }
        let schedule = self
            .db
            .ensure_assistant_schedule(workspace_id, &assistant_agent_id(workspace_id), Utc::now())
            .await
            .map_err(|error| ManualStartError::Unavailable(error.to_string()))?;
        // One workspace-wide in-flight slot shared with scheduled and event
        // wakes. The guard is released when the spawned execution finishes
        // (including panics or cancellation).
        if !self.in_flight.insert(workspace_id.to_string()) {
            return Err(ManualStartError::InFlight);
        }
        let guard = InFlightGuard {
            set: Arc::clone(&self.in_flight),
            key: workspace_id.to_string(),
        };
        let prepared = match self.executor.prepare_manual(&schedule, operation).await {
            Ok(prepared) => prepared,
            Err(error) => {
                drop(guard);
                return Err(error);
            }
        };
        let start = ManualTurnStart {
            session_key: prepared.session_key().to_string(),
            turn_id: prepared.turn_id().to_string(),
            attempt_id: prepared.attempt_id().to_string(),
        };
        let executor = self.executor.clone();
        let cancel = self.shutdown.clone();
        let workspace = workspace_id.to_string();
        tokio::spawn(async move {
            let _guard = guard;
            let report = executor.execute_manual(prepared, cancel).await;
            tracing::info!(
                workspace_id = %workspace,
                operation = operation.as_str(),
                session_key = %report.session_key,
                outcome = report.outcome.as_str(),
                "manual suppression operation finished"
            );
        });
        Ok(start)
    }

    async fn record_confirmed_false_skip(
        &self,
        workspace_id: &str,
        turn_id: &str,
        reviewer: &str,
        root_cause: &str,
    ) -> Result<(), ManualStartError> {
        self.executor
            .record_confirmed_false_skip(workspace_id, turn_id, reviewer, root_cause)
            .await
    }
}

/// Deterministic `0..=15s` workspace jitter for canary/enforce only.
///
/// Derived from the workspace ID (not process randomness or wall time), so
/// every worker computes the same next wake for the same workspace. Off and
/// shadow always return 0: their scheduling behavior must not change.
pub fn wake_jitter_seconds(workspace_id: &str, mode: SuppressionMode) -> i64 {
    if !matches!(mode, SuppressionMode::Canary | SuppressionMode::Enforce) {
        return 0;
    }
    let digest = Sha256::digest(workspace_id.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(bytes) % (WAKE_JITTER_MAX_SECONDS as u64 + 1)) as i64
}

/// TriggerRouter is the only Schedule outcome writer. An `audit_failed`
/// barrier must stay visible until a later full Turn completes: generic
/// skipped/failed outcomes may not overwrite it.
fn persisted_outcome(previous: Option<&str>, outcome: &WakeOutcome) -> String {
    let generic = matches!(outcome, WakeOutcome::Skipped | WakeOutcome::Failed { .. });
    if previous == Some(OUTCOME_AUDIT_FAILED) && generic {
        OUTCOME_AUDIT_FAILED.to_string()
    } else {
        outcome.as_str().to_string()
    }
}

/// 设备事件桥：把 `ThingEventBus` 上的持久化设备事件转发到 `TriggerRouter`。
///
/// 只准入与 legacy `ThingEventTrigger` 相同的判定集：未知事件、agent 自身
/// 产生的事件（防共振）与低于 warning 级（level < 3）的事件不唤醒。
pub fn spawn_thing_event_bridge(
    bus: Arc<crate::domains::event::bus::ThingEventBus>,
    router: Arc<TriggerRouter>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let mut rx = bus.subscribe();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                signal = rx.recv() => {
                    let signal = match signal {
                        Ok(signal) => signal,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    };
                    if signal.is_unknown || signal.actor == "agent" || signal.level < 3 {
                        continue;
                    }
                    let wake = thing_event_wake(
                        &signal.workspace_id,
                        &signal.thing_id,
                        &signal.event_name,
                        &signal.event_id.to_string(),
                    );
                    if !router.submit_event(wake) {
                        tracing::warn!(
                            workspace_id = %signal.workspace_id,
                            event_id = %signal.event_id,
                            "thing event wake not admitted (in flight or rate limited)"
                        );
                    }
                }
            }
        }
    });
}

/// 设备事件（如离线）→ event wake。纯映射，便于无 fixture 测试。
pub(crate) fn thing_event_wake(workspace_id: &str, thing_id: &str, event_name: &str, event_id: &str) -> EventWake {
    EventWake {
        workspace_id: workspace_id.to_string(),
        trigger: "thing_event".to_string(),
        source_id: event_id.to_string(),
        summary: format!("设备事件 {event_name}（{thing_id}）"),
        dedup_key: Some(format!("thing:{thing_id}:event:{event_name}")),
    }
}

/// 把一条 AI 事件转换为 event wake：仅 Critical/Error 告警（大小写不敏感）
/// 进入 Session Loop，其余事件忽略。
pub(crate) fn event_wake_from_ai(ai: &AiEvent) -> Option<EventWake> {
    let AiEvent::AlarmCreated(alarm) = ai else { return None };
    if !(alarm.severity.eq_ignore_ascii_case("critical") || alarm.severity.eq_ignore_ascii_case("error")) {
        return None;
    }
    Some(EventWake {
        workspace_id: alarm.workspace_id.clone(),
        trigger: "alarm".to_string(),
        source_id: alarm.id.clone(),
        summary: format!("{}（{}，设备 {}）", alarm.message, alarm.severity, alarm.thing_id),
        // 沿用旧 watchdog 分键：同一设备同类告警共享限流预算，防止刷屏。
        dedup_key: Some(format!("alarm:{}:{}", alarm.thing_id, alarm.alarm_type)),
    })
}

/// Watch 命中 → event wake。Task 所有的 watch 由 Task wake router 消费，此处跳过。
pub(crate) fn watch_wake(
    workspace_id: &str,
    rule_id: &str,
    owner_task_id: Option<&str>,
    current_value: &serde_json::Value,
    health: &str,
    zone: &str,
) -> Option<EventWake> {
    if owner_task_id.is_some() {
        return None;
    }
    Some(EventWake {
        workspace_id: workspace_id.to_string(),
        trigger: "watch".to_string(),
        source_id: rule_id.to_string(),
        summary: format!("Watch 命中 {rule_id}：value={current_value} health={health} zone={zone}"),
        dedup_key: Some(format!("watch:{rule_id}")),
    })
}

/// 订阅 EventBus 上的 `AiEvent::AlarmCreated`，仅 Critical/Error 告警进入
/// Session Loop。订阅在调用方同步完成（本函数只消费）。
async fn subscribe_alarms(
    mut receiver: broadcast::Receiver<Event>,
    router: Arc<TriggerRouter>,
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
                let Some(wake) = event_wake_from_ai(&ai) else {
                    continue;
                };
                // 统一经 `submit_event` 准入：同 workspace 在途去重 + 3/min
                // 速率窗口，与直接调用方一致。
                if !router.submit_event(wake.clone()) {
                    tracing::warn!(
                        workspace_id = %wake.workspace_id,
                        source_id = %wake.source_id,
                        "alarm wake not admitted (in flight or rate limited)"
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
        events: Mutex<Vec<EventWake>>,
        recoveries: AtomicUsize,
        recovered_workspaces: Mutex<Vec<String>>,
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

        async fn execute_event(&self, wake: EventWake, _cancel: CancellationToken) -> WakeReport {
            self.events.lock().await.push(wake);
            WakeReport {
                session_key: String::new(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Completed,
            }
        }

        async fn recover_suppression(
            &self,
            workspace_id: &str,
        ) -> Vec<crate::domains::agent::session::suppression::audit::RecoveryReport> {
            self.recoveries.fetch_add(1, Ordering::SeqCst);
            self.recovered_workspaces.lock().await.push(workspace_id.to_string());
            Vec::new()
        }
    }

    struct GuardedRecoveryExecutor {
        first: AtomicUsize,
        recovered_workspace: Mutex<Option<String>>,
        started: Notify,
        release: Notify,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for GuardedRecoveryExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            WakeReport {
                session_key: String::new(),
                outcome: crate::domains::agent::session::wake::WakeOutcome::Skipped,
            }
        }

        async fn recover_suppression(
            &self,
            workspace_id: &str,
        ) -> Vec<crate::domains::agent::session::suppression::audit::RecoveryReport> {
            if self.first.swap(1, Ordering::SeqCst) == 0 {
                *self.recovered_workspace.lock().await = Some(workspace_id.to_string());
                self.started.notify_one();
                self.release.notified().await;
            }
            Vec::new()
        }
    }

    #[tokio::test]
    async fn startup_recovery_holds_the_in_flight_guard() {
        let db = Arc::new(test_db().await);
        tinyiothub_storage::seed::seed_system(&db)
            .await
            .expect("seed system tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws-guarded', 'workspace', 'tenant-default-001', 'now', 'now')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        let executor = Arc::new(GuardedRecoveryExecutor {
            first: AtomicUsize::new(0),
            recovered_workspace: Mutex::new(None),
            started: Notify::new(),
            release: Notify::new(),
        });
        let router = Arc::new(TriggerRouter::new(db, executor.clone()));

        let recovery = tokio::spawn({
            let router = router.clone();
            async move { router.recover_suppression().await }
        });
        executor.started.notified().await;
        let workspace_id = executor
            .recovered_workspace
            .lock()
            .await
            .clone()
            .expect("recovery workspace");
        assert!(
            router.in_flight.contains(&workspace_id),
            "recovery must hold the workspace in-flight guard"
        );
        executor.release.notify_one();
        recovery.await.expect("recovery task");
        assert!(
            !router.in_flight.contains(&workspace_id),
            "the guard is released after recovery"
        );
    }

    #[tokio::test]
    async fn successful_schedule_write_clears_the_skip_schedule_barrier() {
        let root = tempfile::tempdir().expect("workspace store root");
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        let db = Arc::new(tinyiothub_storage::Db::new(pool).with_workspace_store_root(root.path().join("data/agents")));
        tinyiothub_storage::seed::seed_system(&db)
            .await
            .expect("seed system tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws-barrier', 'workspace', 'tenant-default-001', 'now', 'now')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        db.create_workspace_store("ws-barrier").await.unwrap();
        let now = ts(1_800_000_000);
        let schedule = db
            .ensure_assistant_schedule("ws-barrier", &assistant_agent_id("ws-barrier"), now)
            .await
            .unwrap();
        let pool = db.workspace_pool_for_testing("ws-barrier").await.unwrap();
        sqlx::query(
            "INSERT INTO suppression_state \
             (workspace_id, session_id, created_at, updated_at, schedule_sync_pending) \
             VALUES (?, ?, '2027-01-15T08:00:00Z', '2027-01-15T08:00:00Z', 1)",
        )
        .bind("ws-barrier")
        .bind("s")
        .execute(&pool)
        .await
        .unwrap();

        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db.clone(), executor));
        router.dispatch(schedule).await;

        let state = db
            .load_suppression_state("ws-barrier", "s")
            .await
            .unwrap()
            .expect("suppression state");
        assert!(
            !state.schedule_sync_pending,
            "a successful Schedule write must clear the skip's Schedule barrier"
        );
    }

    fn thing_signal(actor: &str, level: i32, event_id: i64) -> tinyiothub_core::models::event::ThingEventSignal {
        tinyiothub_core::models::event::ThingEventSignal {
            workspace_id: "ws-1".into(),
            thing_id: "thing-1".into(),
            event_name: "offline".into(),
            event_id,
            level,
            data: serde_json::Value::Null,
            is_unknown: false,
            actor: actor.into(),
        }
    }

    #[tokio::test]
    async fn thing_event_bridge_forwards_device_events_and_skips_agent_events() {
        let db = Arc::new(test_db().await);
        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db, executor.clone()));
        let bus = Arc::new(crate::domains::event::bus::ThingEventBus::new());
        let shutdown = CancellationToken::new();
        let _tick = router.clone().start();
        spawn_thing_event_bridge(bus.clone(), router.clone(), shutdown.clone());
        for _ in 0..10_000 {
            if bus.receiver_count() > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(bus.receiver_count() > 0, "thing-event bridge must subscribe");

        bus.publish(thing_signal("device", 4, 41));
        for _ in 0..10_000 {
            if executor.events.lock().await.len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let events = executor.events.lock().await.clone();
        assert_eq!(events.len(), 1, "one device event -> one wake");
        assert_eq!(events[0].workspace_id, "ws-1");
        assert_eq!(events[0].trigger, "thing_event");
        assert_eq!(events[0].source_id, "41");
        assert_eq!(
            events[0].dedup_key.as_deref(),
            Some("thing:thing-1:event:offline"),
            "thing dedup key must scope per device + event name"
        );

        bus.publish(thing_signal("agent", 5, 42));
        bus.publish(thing_signal("device", 2, 43));
        let mut unknown = thing_signal("device", 5, 44);
        unknown.is_unknown = true;
        bus.publish(unknown);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            executor.events.lock().await.len(),
            1,
            "unknown, agent-actor and below-warning events must not wake"
        );
        shutdown.cancel();
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
    async fn startup_recovery_runs_for_every_workspace() {
        let db = Arc::new(test_db().await);
        tinyiothub_storage::seed::seed_system(&db)
            .await
            .expect("seed system tenant before inserting the workspace");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES ('ws-recover', 'workspace', 'tenant-default-001', 'now', 'now')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        let expected = db.find_all_workspace_ids().await.expect("workspaces");
        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()));

        router.recover_suppression().await;

        assert_eq!(
            executor.recoveries.load(Ordering::SeqCst),
            expected.len(),
            "startup recovery must inspect every persisted workspace"
        );
        let recovered = executor.recovered_workspaces.lock().await;
        for workspace_id in expected {
            assert!(recovered.contains(&workspace_id), "missing {workspace_id}");
        }
        assert!(recovered.iter().any(|workspace| workspace == "ws-recover"));
    }

    fn alarm(severity: &str) -> AiEvent {
        AiEvent::AlarmCreated(tinyiothub_core::models::event::AlarmEvent {
            id: "alarm-1".into(),
            workspace_id: "ws-1".into(),
            thing_id: "thing-1".into(),
            alarm_type: "threshold".into(),
            severity: severity.into(),
            message: "温度过高".into(),
            rule_id: None,
            condition_desc: None,
            resolved: false,
            created_at: ts(1_800_000_000),
        })
    }

    #[test]
    fn event_wake_matches_severity_case_insensitively() {
        for severity in ["Critical", "ERROR", "critical", "error"] {
            let wake = event_wake_from_ai(&alarm(severity)).expect("critical/error alarm must produce a wake");
            assert_eq!(wake.workspace_id, "ws-1");
            assert_eq!(wake.trigger, "alarm");
            assert_eq!(wake.source_id, "alarm-1");
        }
        assert!(event_wake_from_ai(&alarm("warning")).is_none());
        assert!(
            event_wake_from_ai(&AiEvent::AlarmResolved {
                alarm_id: "alarm-1".into(),
                thing_id: "thing-1".into(),
                rule_id: None,
            })
            .is_none()
        );
    }

    #[tokio::test]
    async fn tick_dispatches_due_and_advances() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-1", "__assistant__ws-1", now)
            .await
            .unwrap();
        let executor = Arc::new(RecordingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()).with_clock(move || now));

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
        let router = Arc::new(TriggerRouter::new(db.clone(), Arc::new(SkippedExecutor)).with_clock(move || now));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }

        let row = db.get_assistant_schedule("ws-skip").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(now + chrono::Duration::seconds(300)));
        assert_eq!(row.last_outcome.as_deref(), Some("skipped"));
    }

    #[test]
    fn audit_failed_outcome_is_not_overwritten_by_generic_failures() {
        assert_eq!(
            persisted_outcome(Some("audit_failed"), &WakeOutcome::Skipped),
            "audit_failed"
        );
        assert_eq!(
            persisted_outcome(
                Some("audit_failed"),
                &WakeOutcome::Failed {
                    error: "provider".into()
                }
            ),
            "audit_failed"
        );
        assert_eq!(
            persisted_outcome(Some("audit_failed"), &WakeOutcome::AuditFailed),
            "audit_failed"
        );
        assert_eq!(
            persisted_outcome(Some("audit_failed"), &WakeOutcome::Completed),
            "completed",
            "a successful full Turn may clear the barrier"
        );
        assert_eq!(persisted_outcome(Some("completed"), &WakeOutcome::Skipped), "skipped");
        assert_eq!(WakeOutcome::AuditFailed.as_str(), "audit_failed");
    }

    #[tokio::test]
    async fn audit_failed_schedule_outcome_survives_a_later_skipped_wake() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule("ws-audit-failed", "__assistant__ws-audit-failed", now)
            .await
            .unwrap();
        db.mark_assistant_wake("ws-audit-failed", now, now, "audit_failed")
            .await
            .unwrap();
        let router = Arc::new(TriggerRouter::new(db.clone(), Arc::new(SkippedExecutor)).with_clock(move || now));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }

        let row = db.get_assistant_schedule("ws-audit-failed").await.unwrap().unwrap();
        assert_eq!(
            row.last_outcome.as_deref(),
            Some("audit_failed"),
            "a generic skipped outcome must not hide the audit barrier"
        );
        assert_eq!(row.next_wake_at, Some(now + chrono::Duration::seconds(300)));
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
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()).with_clock(move || now));

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
        sqlx::query("INSERT INTO subscription_plans (id, name, display_name) VALUES ('plan_free', 'free', 'Free')")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug) VALUES ('tenant-default-001', 'tenant', 'tenant-default-001')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-e2e', 'workspace', 'tenant-default-001', 'now', 'now')")
            .execute(db.pool())
            .await
            .unwrap();
        db.create_workspace_store("ws-e2e").await.unwrap();
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
        let router = Arc::new(TriggerRouter::new(db.clone(), wake).with_clock(move || now));

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
                "step.text",
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
    async fn dedup_key_scopes_the_rate_window() {
        let db = Arc::new(test_db().await);
        let router = TriggerRouter::new(db, Arc::new(RecordingExecutor::default()));
        let wake = |rule: &str| EventWake {
            workspace_id: "ws-1".into(),
            trigger: "watch".into(),
            source_id: rule.into(),
            summary: "hit".into(),
            dedup_key: Some(format!("watch:{rule}")),
        };
        for _ in 0..3 {
            assert!(router.submit_event(wake("r1")));
        }
        assert!(!router.submit_event(wake("r1")), "same rule limited");
        assert!(router.submit_event(wake("r2")), "different rule has its own budget");
    }

    #[test]
    fn watch_mapping_produces_scoped_wakes() {
        let watch = watch_wake("ws", "rule-1", None, &serde_json::json!(42), "Healthy", "A").unwrap();
        assert_eq!(watch.trigger, "watch");
        assert_eq!(watch.source_id, "rule-1");
        assert_eq!(watch.dedup_key.as_deref(), Some("watch:rule-1"));
        assert!(watch_wake("ws", "rule-1", Some("task-1"), &serde_json::json!(42), "Healthy", "A").is_none());
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
            dedup_key: None,
        }));
        executor.started.notified().await;
        assert!(!router.submit_event(EventWake {
            workspace_id: "ws-1".into(),
            trigger: "alarm".into(),
            source_id: "alarm-2".into(),
            summary: "another".into(),
            dedup_key: None,
        }));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
        executor.release.notify_one();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn committed_skip_reconciles_schedule_and_advances_from_the_tick() {
        use crate::domains::agent::pool::AgentPool;
        use crate::domains::agent::prompt::PromptMemorySource;
        use crate::domains::agent::session::context::TurnContextRegistry;
        use crate::domains::agent::session::events::test_support::RecordingSink;
        use crate::domains::agent::session::runner::SessionRunner;
        use crate::domains::agent::session::suppression::SuppressionMode;
        use crate::domains::agent::session::suppression::registry::SnapshotProviderSlot;
        use crate::domains::agent::session::todos::TodoStore;
        use crate::domains::agent::session::wake::{WakeRunner, assistant_agent_id};
        use tinyiothub_agent::port::attribution::{Attributable, ModelProviderKind, ProviderKind, Role};
        use tinyiothub_agent::port::memory::NoopMemory;
        use tinyiothub_agent::port::observer::NoopObserver;
        use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};
        use tinyiothub_core::harness::{ExecutionMode, HarnessEvent};
        use tinyiothub_core::memory::AgentMemory;

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

        struct EmptyMemory;

        #[async_trait::async_trait]
        impl PromptMemorySource for EmptyMemory {
            async fn list_active(&self, _workspace_id: &str, _agent_id: &str) -> anyhow::Result<Vec<AgentMemory>> {
                Ok(Vec::new())
            }

            async fn record_load(&self, _id: &str) -> anyhow::Result<()> {
                Ok(())
            }
        }

        const WS: &str = "ws-skip-e2e";
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(
            Db::new(tinyiothub_storage::test_helpers::test_pool().await)
                .with_workspace_store_root(root.path().join("stores")),
        );
        tinyiothub_storage::seed::seed_system(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
             VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
        )
        .bind(WS)
        .execute(db.pool())
        .await
        .unwrap();
        db.create_workspace_store(WS).await.unwrap();
        let now = ts(1_800_000_000);
        db.ensure_assistant_schedule(WS, &assistant_agent_id(WS), now)
            .await
            .unwrap();
        db.update_suppression_config(WS, Some(SuppressionMode::Enforce), None)
            .await
            .unwrap()
            .expect("schedule row");

        let agent_pool = Arc::new(
            AgentPool::new(
                &tinyiothub_core::config::AgentSettings::default(),
                Arc::new(NoopMemory),
                Arc::new(NoopObserver),
                Arc::new(|| Ok(Box::new(ScriptedProvider))),
            )
            .unwrap(),
        );
        let sessions: Arc<dyn crate::domains::agent::session::events::SessionEventSink> =
            Arc::new(RecordingSink::default());
        let runner = Arc::new(SessionRunner::new(
            sessions.clone(),
            Arc::new(TurnContextRegistry::default()),
        ));
        let wake = Arc::new(
            WakeRunner::from_parts(db.clone(), agent_pool, sessions, Arc::new(TodoStore::default()), runner)
                .with_memory_source(Arc::new(EmptyMemory))
                .with_snapshot_provider(Arc::new(SnapshotProviderSlot::with(
                    tinyiothub_runtime::watch::live_state::WorkspaceSnapshot {
                        v: 1,
                        devices: vec![],
                        total: 0,
                    },
                ))),
        );
        let clock = Arc::new(std::sync::Mutex::new(now));
        let clock_view = Arc::clone(&clock);
        let router =
            Arc::new(TriggerRouter::new(db.clone(), wake).with_clock(move || *clock_view.lock().expect("test clock")));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }
        let first = db.get_assistant_schedule(WS).await.unwrap().unwrap();
        assert_eq!(first.last_outcome.as_deref(), Some("completed"));

        // enforce mode adds the deterministic workspace jitter.
        let jitter = wake_jitter_seconds(WS, SuppressionMode::Enforce);
        let second_at = now + chrono::Duration::seconds(300 + jitter);
        *clock.lock().expect("test clock") = second_at;
        for handle in router.tick(second_at).await {
            handle.await.unwrap();
        }

        let row = db.get_assistant_schedule(WS).await.unwrap().unwrap();
        assert_eq!(row.last_outcome.as_deref(), Some("skipped"));
        assert_eq!(
            row.next_wake_at,
            Some(second_at + chrono::Duration::seconds(300 + jitter)),
            "the schedule still advances from completion with interval + enforce jitter"
        );

        let session_key = format!("agent:{WS}:{}/main", assistant_agent_id(WS));
        let state = db
            .load_suppression_state(WS, &session_key)
            .await
            .unwrap()
            .expect("suppression row");
        assert_eq!(state.latest_outcome, "skipped");
        assert!(!state.schedule_sync_pending, "TriggerRouter reconciled Schedule");
        assert!(!state.projection_sync_pending);
        assert!(!state.notification_pending);
        assert!(state.baseline_is_eligible());

        let filter = tinyiothub_storage::harness_event::EventFilter {
            session_id: Some(session_key.clone()),
            run_id: None,
            task_id: None,
            execution_mode: ExecutionMode::Production,
        };
        let events: Vec<HarnessEvent> = db.session_event_page(WS, &filter, 0, 256).await.unwrap().events;
        assert_eq!(
            events
                .iter()
                .filter(|event| { event.event_type == "turn.completed" && event.payload_json["outcome"] == "skipped" })
                .count(),
            1
        );
    }
}

/// Task 9: scheduled-wake coalescing, completion-time scheduling and the
/// manual-operation backpressure admission shared with preflight.
#[cfg(test)]
mod suppression_overload {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::TimeZone;
    use tokio::sync::Notify;

    use super::*;
    use crate::domains::agent::session::suppression::preflight::{
        PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS, PreflightGate,
    };
    use crate::domains::agent::session::wake::{ManualStartError, WakeReport};
    use tinyiothub_storage::suppression::SuppressionMode as StorageMode;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        Db::new(pool)
    }

    struct CompletedExecutor;

    #[async_trait::async_trait]
    impl WakeExecutor for CompletedExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            WakeReport {
                session_key: "s".into(),
                outcome: WakeOutcome::Completed,
            }
        }
    }

    #[derive(Default)]
    struct CountingExecutor {
        calls: AtomicUsize,
        started: Notify,
        release: Notify,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for CountingExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            self.release.notified().await;
            WakeReport {
                session_key: "s".into(),
                outcome: WakeOutcome::Completed,
            }
        }
    }

    struct FailOnceExecutor {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl WakeExecutor for FailOnceExecutor {
        async fn execute(&self, _schedule: AssistantSchedule, _cancel: CancellationToken) -> WakeReport {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                WakeReport {
                    session_key: "s".into(),
                    outcome: WakeOutcome::Failed {
                        error: "injected first-attempt failure".into(),
                    },
                }
            } else {
                WakeReport {
                    session_key: "s".into(),
                    outcome: WakeOutcome::Completed,
                }
            }
        }
    }

    async fn schedule_for(db: &Arc<Db>, workspace_id: &str, now: DateTime<Utc>) {
        db.ensure_assistant_schedule(workspace_id, &assistant_agent_id(workspace_id), now)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn suppression_overload_same_wake_coalesces_without_queueing() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        schedule_for(&db, "ws-coalesce", now).await;
        let executor = Arc::new(CountingExecutor::default());
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()).with_clock(move || now));

        let mut tick_tasks = Vec::new();
        for _ in 0..8 {
            let router = Arc::clone(&router);
            tick_tasks.push(tokio::spawn(async move { router.tick(now).await }));
        }
        executor.started.notified().await;
        let mut dispatch_handles = Vec::new();
        for task in tick_tasks {
            dispatch_handles.extend(task.await.expect("tick task"));
        }
        assert_eq!(
            executor.calls.load(Ordering::SeqCst),
            1,
            "a duplicate burst for the same planned wake must create one Turn"
        );
        assert_eq!(dispatch_handles.len(), 1, "coalesced wakes are not queued as tasks");
        assert!(router.in_flight.contains("ws-coalesce"));

        executor.release.notify_one();
        for handle in dispatch_handles {
            handle.await.unwrap();
        }
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        assert!(!router.in_flight.contains("ws-coalesce"));

        let again = router.tick(now).await;
        assert!(
            again.is_empty(),
            "the same planned wake is not due again after its schedule advanced"
        );
    }

    #[tokio::test]
    async fn suppression_overload_coalesced_wake_still_executes_after_recovery() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        schedule_for(&db, "ws-recovery", now).await;
        let executor = Arc::new(FailOnceExecutor {
            calls: AtomicUsize::new(0),
        });
        let router = Arc::new(TriggerRouter::new(db.clone(), executor.clone()).with_clock(move || now));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        let failed = db.get_assistant_schedule("ws-recovery").await.unwrap().unwrap();
        assert_eq!(failed.last_outcome.as_deref(), Some("failed"));

        router.recover_suppression().await;
        assert!(!router.in_flight.contains("ws-recovery"));

        let next_due = now + chrono::Duration::seconds(300);
        for handle in router.tick(next_due).await {
            handle.await.unwrap();
        }
        assert_eq!(
            executor.calls.load(Ordering::SeqCst),
            2,
            "a coalesced wake still executes after recovery"
        );
        let recovered = db.get_assistant_schedule("ws-recovery").await.unwrap().unwrap();
        assert_eq!(recovered.last_outcome.as_deref(), Some("completed"));
    }

    #[test]
    fn suppression_overload_jitter_is_zero_for_off_and_shadow() {
        for workspace in ["ws-off", "ws-shadow", "ws-other"] {
            assert_eq!(wake_jitter_seconds(workspace, StorageMode::Off), 0);
            assert_eq!(wake_jitter_seconds(workspace, StorageMode::Shadow), 0);
        }
    }

    #[test]
    fn suppression_overload_jitter_is_deterministic_and_bounded() {
        let mut seen = HashSet::new();
        for index in 0..64 {
            let workspace = format!("ws-jitter-{index}");
            let jitter = wake_jitter_seconds(&workspace, StorageMode::Enforce);
            assert!((0..=WAKE_JITTER_MAX_SECONDS).contains(&jitter));
            assert_eq!(jitter, wake_jitter_seconds(&workspace, StorageMode::Enforce));
            assert_eq!(
                jitter,
                wake_jitter_seconds(&workspace, StorageMode::Canary),
                "canary and enforce share the deterministic workspace jitter"
            );
            seen.insert(jitter);
        }
        assert!(
            seen.len() > 1,
            "workspace hashing must vary the jitter, not pin one value"
        );
    }

    #[tokio::test]
    async fn suppression_overload_off_and_shadow_timing_parity() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        let completed_at = now + chrono::Duration::seconds(40);
        for (workspace, mode) in [
            ("ws-off-parity", StorageMode::Off),
            ("ws-shadow-parity", StorageMode::Shadow),
        ] {
            schedule_for(&db, workspace, now).await;
            db.update_suppression_config(workspace, Some(mode), None)
                .await
                .unwrap()
                .expect("schedule row");
        }
        let router =
            Arc::new(TriggerRouter::new(db.clone(), Arc::new(CompletedExecutor)).with_clock(move || completed_at));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }

        for workspace in ["ws-off-parity", "ws-shadow-parity"] {
            let row = db.get_assistant_schedule(workspace).await.unwrap().unwrap();
            assert_eq!(
                row.next_wake_at,
                Some(completed_at + chrono::Duration::seconds(300)),
                "{workspace} keeps off/shadow jitter at zero"
            );
            assert_eq!(row.last_wake_at, Some(completed_at));
        }
    }

    #[tokio::test]
    async fn suppression_overload_enforce_uses_completion_plus_interval_and_jitter() {
        let db = Arc::new(test_db().await);
        let now = ts(1_800_000_000);
        let completed_at = now + chrono::Duration::seconds(25);
        schedule_for(&db, "ws-enforce-jitter", now).await;
        db.update_suppression_config("ws-enforce-jitter", Some(StorageMode::Enforce), None)
            .await
            .unwrap()
            .expect("schedule row");
        let router =
            Arc::new(TriggerRouter::new(db.clone(), Arc::new(CompletedExecutor)).with_clock(move || completed_at));

        for handle in router.tick(now).await {
            handle.await.unwrap();
        }

        let expected = completed_at
            + chrono::Duration::seconds(300)
            + chrono::Duration::seconds(wake_jitter_seconds("ws-enforce-jitter", StorageMode::Enforce));
        let row = db.get_assistant_schedule("ws-enforce-jitter").await.unwrap().unwrap();
        assert_eq!(row.next_wake_at, Some(expected));
        assert_eq!(row.last_wake_at, Some(completed_at));
    }

    #[tokio::test]
    async fn suppression_overload_manual_operations_share_preflight_backpressure() {
        let db = Arc::new(test_db().await);
        let gate = PreflightGate::new(1);
        let held = gate.try_acquire().expect("hold the only preflight slot");
        let router = Arc::new(TriggerRouter::new(db, Arc::new(CompletedExecutor)).with_preflight_gate(gate.clone()));

        let preflight = router.manual_preflight("ws-manual-gate").await;
        assert!(matches!(
            preflight,
            Err(ManualStartError::Backpressure { retry_after_ms })
                if retry_after_ms == PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS
        ));
        let start = router.start_manual("ws-manual-gate", ManualOperation::Wake).await;
        assert!(matches!(
            start,
            Err(ManualStartError::Backpressure { retry_after_ms })
                if retry_after_ms == PREFLIGHT_BACKPRESSURE_RETRY_AFTER_MS
        ));

        drop(held);
        assert!(gate.has_capacity());
    }
}

//! Per-workspace wake scheduler — same-key rate limit, hourly throttle,
//! directive dedup, serial run execution, drain (T8; E1 Task 7 废除合并窗).
//!
//! Pipeline:
//!
//! ```text
//! triggers → handle.enqueue() ─→ Critical 直通 / 用户指令去重 / 同键限速 / 小时预算 ─→ ready queue (cap 50) ─→ serial consumer → run()
//! ```
//!
//! Behavior rules:
//! ① same-key rate limit (E1 D3/D4, replaces the deleted 30s merge window):
//!   signals with a `dedup_key` are admitted at most N times per key per
//!   rolling minute (default [`DEFAULT_RATE_LIMIT_PER_MIN`], per-workspace
//!   override via the shared `rate_limits` table written through from
//!   apps/cloud config, hot-effective like heartbeat intervals); over-limit
//!   signals are rejected with [`EnqueueError::RateLimited`] + metric
//!   `agent_wake_rate_limited` + a `rate_limited` hub-ledger entry when a
//!   ledger is wired (spec §3：被限的 flap 必须可见，防静默)
//! ② Critical bypasses the rate limit AND the hourly throttle (O10)
//! ③ hourly wake budget 20/ws: budgeted signals over budget are dropped
//!   (metric `agent_wake_throttled`); user directives
//!   (`TriggerSource::UserDirective` with `source: None`) are never
//!   throttled — they queue, bounded only by the queue capacity 50 (O5)
//! ④ same-text user directives dedup for 60s (O5)
//! ⑤ serial execution: one run at a time per workspace
//!
//! `drain()` (O26): mode→off clears the ready queue and waits for the
//! consumer to acknowledge. An in-flight run is NOT cancelled; drain
//! completes once it finishes.
//!
//! Budget accounting: every admitted non-Critical, non-user-directive
//! signal counts ONCE against the hourly budget (合并窗已废，无聚合记账).
//! `TriggerSource::Merged` 变体保留（存储/回放兼容），scheduler 不再产生。

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

use tinyiothub_runtime::ports::HubLedger;

use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};

/// Ready-queue capacity per workspace (O5).
pub const QUEUE_CAPACITY: usize = 50;
/// Max budgeted wakes per workspace per rolling hour.
pub const HOURLY_WAKE_LIMIT: usize = 20;
/// Same-text user directive dedup window (O5).
pub const DIRECTIVE_DEDUP: Duration = Duration::from_secs(60);
/// Default same-key rate limit per workspace (E1 spec §3：每键每分钟 3 次).
pub const DEFAULT_RATE_LIMIT_PER_MIN: u32 = 3;
/// Rate-limit sliding window length (per key).
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

type BoxRunFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type RunFn = Arc<dyn Fn(WakeSignal) -> BoxRunFuture + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EnqueueError {
    /// Scheduler shut down (consumer task exited).
    #[error("scheduler shut down")]
    Closed,
    /// User directive and the queue is full — the caller must inform the
    /// user (O5). Directives are never silently dropped.
    #[error("user directive rejected: queue full")]
    Rejected,
    /// Non-directive signal and the queue is full — dropped, metric
    /// `agent_wake_dropped` logged.
    #[error("signal dropped: queue full")]
    Dropped,
    /// Hourly wake budget exhausted — dropped, metric
    /// `agent_wake_throttled` logged.
    #[error("signal throttled: hourly wake budget exhausted")]
    Throttled,
    /// Same-text user directive already enqueued within 60s (O5).
    #[error("duplicate user directive within 60s")]
    Duplicate,
    /// Same-key signal over the per-minute rate limit (E1 D3/D4) — dropped,
    /// metric `agent_wake_rate_limited` logged, `rate_limited` ledger entry.
    #[error("signal rate-limited: same-key per-minute limit exceeded")]
    RateLimited,
}

pub struct Scheduler;

impl Scheduler {
    /// Spawn the per-workspace scheduler: a serial consumer task driving
    /// `run`. The task exits once the returned handle (and its clones) are
    /// dropped. `rate_limits` is the shared per-workspace override table
    /// (key workspace_id, value N/min; 照 heartbeat intervals 写穿模式).
    pub fn spawn(
        workspace_id: String,
        run: impl Fn(WakeSignal) -> BoxRunFuture + Send + Sync + 'static,
        rate_limits: Arc<DashMap<String, u32>>,
    ) -> SchedulerHandle {
        let (ready_tx, ready_rx) = mpsc::channel(QUEUE_CAPACITY);
        let throttle = Arc::new(Mutex::new(Throttle::default()));
        let drain = Arc::new(DrainState::default());
        let run: RunFn = Arc::new(run);

        tokio::spawn(consumer_loop(ready_rx, run, drain.clone()));

        SchedulerHandle {
            workspace_id,
            ready_tx,
            throttle,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::default())),
            rate_limits,
            directive_dedup: Arc::new(Mutex::new(HashMap::new())),
            drain,
            ledger: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Clone)]
pub struct SchedulerHandle {
    workspace_id: String,
    ready_tx: mpsc::Sender<WakeSignal>,
    throttle: Arc<Mutex<Throttle>>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    rate_limits: Arc<DashMap<String, u32>>,
    directive_dedup: Arc<Mutex<HashMap<String, Instant>>>,
    drain: Arc<DrainState>,
    /// 限速事件账本（E1 spec §3）：None = 只留 metric（测试/边缘形态）。
    ledger: Arc<Mutex<Option<Arc<dyn HubLedger>>>>,
}

impl SchedulerHandle {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// 限速事件账本接线（E1 spec §3）：None（默认）只留 metric——测试/边缘形态。
    pub fn set_ledger(&self, ledger: Arc<dyn HubLedger>) {
        *self.ledger.lock() = Some(ledger);
    }

    /// Route a wake signal: Critical goes straight to the ready queue; user
    /// directives dedup then queue; keyed signals pass the same-key rate
    /// limiter, then the hourly throttle; keyless normal signals pass the
    /// hourly throttle only.
    pub fn enqueue(&self, sig: WakeSignal) -> Result<(), EnqueueError> {
        // ② Critical bypasses the rate limit and the throttle (O10).
        if sig.priority == Priority::Critical {
            return self.ready_tx.try_send(sig).map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!(
                        metric = "agent_wake_dropped",
                        workspace_id = %self.workspace_id,
                        "critical signal dropped: ready queue full"
                    );
                    EnqueueError::Dropped
                }
                mpsc::error::TrySendError::Closed(_) => EnqueueError::Closed,
            });
        }

        let directive_text = match &sig.source {
            TriggerSource::UserDirective { text, source: None, .. } => Some(text.clone()),
            _ => None,
        };
        if let Some(text) = directive_text {
            // ④ same-text user directive dedup, 60s (O5).
            let now = Instant::now();
            let mut dedup = self.directive_dedup.lock();
            dedup.retain(|_, seen| now.duration_since(*seen) < DIRECTIVE_DEDUP);
            if dedup.contains_key(&text) {
                tracing::debug!(
                    workspace_id = %self.workspace_id,
                    "duplicate user directive within 60s — ignored"
                );
                return Err(EnqueueError::Duplicate);
            }
            // ③ user directives are never throttled and never silently
            // dropped — a full queue rejects so the caller can inform the
            // user (O5). The dedup entry is recorded ONLY on a successful
            // send: a Rejected/Closed directive must stay retryable.
            return match self.ready_tx.try_send(sig) {
                Ok(()) => {
                    dedup.insert(text, now);
                    Ok(())
                }
                Err(mpsc::error::TrySendError::Full(_)) => Err(EnqueueError::Rejected),
                Err(mpsc::error::TrySendError::Closed(_)) => Err(EnqueueError::Closed),
            };
        }

        // ① same-key rate limit (E1 D3/D4): keyed signals pass the limiter
        // BEFORE the hourly budget; over-limit → reject + metric + ledger.
        if let Some(key) = &sig.dedup_key {
            let limit = self
                .rate_limits
                .get(&self.workspace_id)
                .map(|v| *v)
                .unwrap_or(DEFAULT_RATE_LIMIT_PER_MIN);
            if !self.rate_limiter.lock().allow(key, limit) {
                tracing::warn!(
                    metric = "agent_wake_rate_limited",
                    workspace_id = %self.workspace_id,
                    dedup_key = %key,
                    limit,
                    "signal rate-limited"
                );
                self.record_rate_limited(key, limit);
                return Err(EnqueueError::RateLimited);
            }
        }

        // ③ hourly budget → ready queue (budgeted, droppable).
        let mut throttle = self.throttle.lock();
        if throttle.over_budget() {
            tracing::warn!(
                metric = "agent_wake_throttled",
                workspace_id = %self.workspace_id,
                "signal throttled: hourly wake budget exhausted"
            );
            return Err(EnqueueError::Throttled);
        }
        match self.ready_tx.try_send(sig) {
            Ok(()) => {
                throttle.record();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    metric = "agent_wake_dropped",
                    workspace_id = %self.workspace_id,
                    "signal dropped: ready queue full"
                );
                Err(EnqueueError::Dropped)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(EnqueueError::Closed),
        }
    }

    /// 限速事件落账（spec §3：被限的 flap 必须可见）。fire-and-forget：
    /// 落账失败只留 warn，不阻断调度链路（账本可补，唤醒不能丢——照
    /// WatchEngine append_ledger 先例）；无 runtime/ledger 时只留 metric。
    fn record_rate_limited(&self, key: &str, limit: u32) {
        let Some(ledger) = self.ledger.lock().clone() else {
            return;
        };
        let workspace_id = self.workspace_id.clone();
        let payload = serde_json::json!({ "v": 1, "dedup_key": key, "limit": limit });
        let Ok(rt) = tokio::runtime::Handle::try_current() else {
            return;
        };
        rt.spawn(async move {
            if let Err(e) = ledger.append(&workspace_id, "rate_limited", None, payload).await {
                tracing::warn!(
                    metric = "hub_ledger_write_failed",
                    workspace_id = %workspace_id,
                    error = %e,
                    "rate_limited 落账失败（不中断调度链路）"
                );
            }
        });
    }

    /// O26: mode→off — clear the ready queue. Waits until the consumer has
    /// acknowledged, so signals enqueued after `drain()` returns are never
    /// wiped. An in-flight run is NOT cancelled; drain completes once it
    /// finishes.
    ///
    /// Concurrent drains are serialized: acks record only the LATEST
    /// generation, so two overlapping drains would starve the later one.
    pub async fn drain(&self) {
        let _guard = self.drain.lock.lock().await;
        let generation = self.drain.requested.fetch_add(1, Ordering::SeqCst) + 1;
        self.drain.consumer_notify.notify_one();
        wait_ack(&self.drain, &self.drain.consumer_acked, generation).await;
    }
}

/// Wait until `counter` reaches `generation`. The waiter is registered
/// (enabled) BEFORE the counter is re-checked, so an ack landing between
/// the check and the wait can never be lost.
async fn wait_ack(drain: &DrainState, counter: &AtomicU64, generation: u64) {
    loop {
        let notified = drain.ack_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if counter.load(Ordering::SeqCst) >= generation {
            return;
        }
        notified.await;
    }
}

#[async_trait::async_trait]
impl crate::runtime::thing_agent::traits::DirectiveSink for SchedulerHandle {
    fn enqueue(&self, sig: WakeSignal) -> Result<(), EnqueueError> {
        SchedulerHandle::enqueue(self, sig)
    }

    /// 单工作区直连句柄：忽略 workspace_id，drain 自身。
    async fn drain(&self, _workspace_id: &str) {
        SchedulerHandle::drain(self).await;
    }
}

/// Sliding-window counter of budgeted wakes admitted in the last hour.
#[derive(Default)]
struct Throttle {
    wakes: VecDeque<Instant>,
}

impl Throttle {
    fn prune(&mut self) {
        let now = Instant::now();
        while let Some(&oldest) = self.wakes.front() {
            if now.duration_since(oldest) >= Duration::from_secs(3600) {
                self.wakes.pop_front();
            } else {
                break;
            }
        }
    }

    fn over_budget(&mut self) -> bool {
        self.prune();
        self.wakes.len() >= HOURLY_WAKE_LIMIT
    }

    fn record(&mut self) {
        self.wakes.push_back(Instant::now());
    }
}

/// ① same-key rate limiter (E1 D3/D4): per-`dedup_key` sliding window
/// (60s) of admitted hits, same shape as [`Throttle`]. The handle is
/// per-workspace, so keys need no workspace dimension.
#[derive(Default)]
struct RateLimiter {
    hits: HashMap<String, VecDeque<Instant>>,
}

impl RateLimiter {
    /// Admit one hit for `key` if fewer than `limit` hits are inside the
    /// rolling window. A rejected hit does NOT consume quota.
    fn allow(&mut self, key: &str, limit: u32) -> bool {
        let now = Instant::now();
        let hits = self.hits.entry(key.to_string()).or_default();
        while hits.front().is_some_and(|&t| now.duration_since(t) >= RATE_LIMIT_WINDOW) {
            hits.pop_front();
        }
        if hits.len() >= limit as usize {
            // limit=0 → 该工作区 keyed 信号全拒；空条目即清，防键 churn 泄漏。
            if hits.is_empty() {
                self.hits.remove(key);
            }
            return false;
        }
        hits.push_back(now);
        true
    }
}

/// Drain coordination: `requested` is a monotonic generation; the consumer
/// records the latest generation it acked after purging its pending state.
#[derive(Default)]
struct DrainState {
    requested: AtomicU64,
    consumer_acked: AtomicU64,
    consumer_notify: Notify,
    ack_notify: Notify,
    /// Serializes concurrent `drain()` calls.
    lock: tokio::sync::Mutex<()>,
}

impl DrainState {
    fn ack(&self, counter: &AtomicU64, last_gen: &mut u64, generation: u64) {
        *last_gen = generation;
        counter.store(generation, Ordering::SeqCst);
        // notify_one stores a permit, so an ack is never lost even when no
        // waiter is registered yet.
        self.ack_notify.notify_one();
    }
}

/// ⑤ serial consumer: receives ready signals one at a time and awaits each
/// run before picking up the next.
async fn consumer_loop(mut ready_rx: mpsc::Receiver<WakeSignal>, run: RunFn, drain: Arc<DrainState>) {
    let mut last_gen = 0u64;
    loop {
        let generation = drain.requested.load(Ordering::SeqCst);
        if generation > last_gen {
            let mut cleared = 0usize;
            while ready_rx.try_recv().is_ok() {
                cleared += 1;
            }
            drain.ack(&drain.consumer_acked, &mut last_gen, generation);
            if cleared > 0 {
                tracing::info!(cleared, "scheduler drained ready queue");
            }
        }
        tokio::select! {
            maybe = ready_rx.recv() => match maybe {
                Some(sig) => run(sig).await,
                None => break,
            },
            // Wake-up only; the generation check above does the work. A
            // stored permit covers drains requested while a run was active.
            () = drain.consumer_notify.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Notify;
    use tokio::time::{advance, pause};

    const WS: &str = "ws_01";
    const KEY: &str = "thing:t1:event:temp_high";

    fn event_signal(event_id: i64) -> WakeSignal {
        WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::ThingEvent {
                thing_id: "t1".to_string(),
                event_name: "temp_high".to_string(),
                event_id,
                level: 3,
                data: serde_json::json!({"id": event_id}),
            },
            dedup_key: Some(KEY.to_string()),
        }
    }

    fn critical_signal(event_id: i64) -> WakeSignal {
        let mut sig = event_signal(event_id);
        sig.priority = Priority::Critical;
        if let TriggerSource::ThingEvent { level, .. } = &mut sig.source {
            *level = 5;
        }
        sig
    }

    fn directive(text: &str) -> WakeSignal {
        WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::High,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: text.to_string(),
                session_key: None,
                source: None,
                problem_key: None,
            },
            dedup_key: None,
        }
    }

    fn heartbeat(tick: u32) -> WakeSignal {
        WakeSignal {
            workspace_id: WS.to_string(),
            priority: Priority::Normal,
            source: TriggerSource::UserDirective {
                user_id: "heartbeat".to_string(),
                text: format!("heartbeat tick {tick}"),
                session_key: None,
                source: Some(format!("heartbeat:{tick}")),
                problem_key: None,
            },
            dedup_key: None,
        }
    }

    fn source_event_id(signal: &WakeSignal) -> i64 {
        match &signal.source {
            TriggerSource::ThingEvent { event_id, .. } => *event_id,
            other => panic!("expected ThingEvent source, got {other:?}"),
        }
    }

    /// Scheduler whose run callback forwards every executed signal.
    fn spawn_collector() -> (SchedulerHandle, mpsc::UnboundedReceiver<WakeSignal>) {
        spawn_collector_with(Arc::new(DashMap::new()))
    }

    fn spawn_collector_with(
        rate_limits: Arc<DashMap<String, u32>>,
    ) -> (SchedulerHandle, mpsc::UnboundedReceiver<WakeSignal>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = Scheduler::spawn(
            WS.to_string(),
            move |sig| {
                let tx = tx.clone();
                Box::pin(async move {
                    let _ = tx.send(sig);
                })
            },
            rate_limits,
        );
        (handle, rx)
    }

    /// 账本桩：记录全部 append 调用（限速落账断言口）。
    #[derive(Default)]
    struct RecordingLedger {
        entries: Mutex<Vec<(String, String, Option<String>, serde_json::Value)>>,
    }

    #[async_trait::async_trait]
    impl HubLedger for RecordingLedger {
        async fn append(
            &self,
            workspace_id: &str,
            kind: &str,
            thing_id: Option<&str>,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.entries.lock().push((
                workspace_id.to_string(),
                kind.to_string(),
                thing_id.map(str::to_string),
                payload,
            ));
            Ok(())
        }
    }

    /// Yield pump so freshly enqueued signals reach the consumer.
    async fn settle() {
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
    }

    // ① 合并窗已废（E1 D3/D4）：带键普通信号不再聚合，即来即醒、各自成 run。
    #[tokio::test]
    async fn keyed_signal_runs_immediately_without_merge_window() {
        pause();
        let (handle, mut runs) = spawn_collector();

        handle.enqueue(event_signal(7)).expect("enqueue");

        // 不 advance：若仍有合并窗，paused 时钟下这个 recv 永远等不到。
        let sig = runs.recv().await.expect("keyed signal runs immediately");
        assert_eq!(source_event_id(&sig), 7);
        assert!(matches!(sig.source, TriggerSource::ThingEvent { .. }), "不再包 Merged");
        assert!(runs.try_recv().is_err());
    }

    // ① 同键限速：默认每键每分钟 3 次，第 4 次拒 RateLimited；前三条各自成 run。
    #[tokio::test]
    async fn same_key_fourth_signal_within_a_minute_is_rate_limited() {
        pause();
        let (handle, mut runs) = spawn_collector();

        for id in 1..=3 {
            handle.enqueue(event_signal(id)).expect("within default limit 3/min");
        }
        assert_eq!(
            handle.enqueue(event_signal(4)),
            Err(EnqueueError::RateLimited),
            "第 4 次同键信号必须被拒（agent_wake_rate_limited）"
        );

        for want in 1..=3 {
            let sig = runs.recv().await.expect("admitted signal runs");
            assert_eq!(source_event_id(&sig), want);
        }
        assert!(runs.try_recv().is_err(), "被拒信号不得成 run");
    }

    // ① 限速窗 60s 滑动：窗内持续拒，窗过期后同键信号重新放行。
    #[tokio::test]
    async fn rate_limit_window_resets_after_one_minute() {
        pause();
        let (handle, _runs) = spawn_collector();

        for id in 1..=3 {
            handle.enqueue(event_signal(id)).expect("within limit");
        }
        assert_eq!(handle.enqueue(event_signal(4)), Err(EnqueueError::RateLimited));

        advance(Duration::from_secs(30)).await;
        assert_eq!(
            handle.enqueue(event_signal(5)),
            Err(EnqueueError::RateLimited),
            "30s 后仍在 60s 窗内（首次命中尚未滑出）"
        );

        advance(Duration::from_secs(31)).await;
        handle.enqueue(event_signal(6)).expect("61s 后窗已滑出，重新放行");
    }

    // ① 每工作区限速值读共享配置表（照 heartbeat intervals 写穿模式）：
    // N=1 时第 2 次同键信号即被拒。
    #[tokio::test]
    async fn per_workspace_limit_comes_from_shared_table() {
        pause();
        let rate_limits = Arc::new(DashMap::new());
        rate_limits.insert(WS.to_string(), 1u32);
        let (handle, _runs) = spawn_collector_with(rate_limits);

        handle.enqueue(event_signal(1)).expect("first within N=1");
        assert_eq!(handle.enqueue(event_signal(2)), Err(EnqueueError::RateLimited));
    }

    // ② Critical 绕过限速（O10 保持）：同键 critical 连发不受 3/min 约束，
    // 且不消耗限速配额（随后的普通同键信号仍按满额放行）。
    #[tokio::test]
    async fn critical_bypasses_rate_limit() {
        pause();
        let (handle, mut runs) = spawn_collector();

        for id in 1..=5 {
            handle.enqueue(critical_signal(id)).expect("critical bypasses rate limit");
        }
        for want in 1..=5 {
            let sig = runs.recv().await.expect("critical runs immediately");
            assert_eq!(sig.priority, Priority::Critical);
            assert_eq!(source_event_id(&sig), want);
        }

        // Critical 不碰限速器：普通信号仍享完整 3/min 配额。
        for id in 1..=3 {
            handle.enqueue(event_signal(id)).expect("limiter untouched by criticals");
        }
        assert_eq!(handle.enqueue(event_signal(4)), Err(EnqueueError::RateLimited));
    }

    // ① 无键信号不过限速器（小时预算仍是其上界）。
    #[tokio::test]
    async fn keyless_signals_are_not_rate_limited() {
        pause();
        let (handle, _runs) = spawn_collector();

        for tick in 0..5 {
            handle.enqueue(heartbeat(tick)).expect("keyless signal unaffected by rate limit");
        }
    }

    // ① 限速落账（spec §3：被限的 flap 必须可见，防静默）：RateLimited 路径
    // 向 HubLedger 落 kind=rate_limited、payload v=1 的事件。
    #[tokio::test]
    async fn rate_limited_event_is_recorded_in_ledger() {
        pause();
        let ledger = Arc::new(RecordingLedger::default());
        let (handle, _runs) = spawn_collector();
        handle.set_ledger(ledger.clone());

        for id in 1..=3 {
            handle.enqueue(event_signal(id)).expect("within limit");
        }
        assert_eq!(handle.enqueue(event_signal(4)), Err(EnqueueError::RateLimited));

        // 落账是 fire-and-forget 的 spawned 写——泵 yield 直到可见。
        for _ in 0..100 {
            if !ledger.entries.lock().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let entries = ledger.entries.lock();
        assert_eq!(entries.len(), 1, "恰一条限速事件（第 4 次被拒）");
        let (ws, kind, thing, payload) = &entries[0];
        assert_eq!(ws, WS);
        assert_eq!(kind, "rate_limited");
        assert_eq!(thing, &None);
        assert_eq!(payload["v"], 1);
        assert_eq!(payload["dedup_key"], KEY);
        assert_eq!(payload["limit"], 3);
    }

    // ① 账本写失败不中断调度链路（账本可补，唤醒不能丢）：拒绝语义不变。
    #[tokio::test]
    async fn rate_limited_ledger_failure_does_not_break_enqueue() {
        pause();
        struct FailingLedger;
        #[async_trait::async_trait]
        impl HubLedger for FailingLedger {
            async fn append(
                &self,
                _: &str,
                _: &str,
                _: Option<&str>,
                _: serde_json::Value,
            ) -> Result<(), String> {
                Err("db down".into())
            }
        }
        let (handle, _runs) = spawn_collector();
        handle.set_ledger(Arc::new(FailingLedger));

        for id in 1..=3 {
            handle.enqueue(event_signal(id)).expect("within limit");
        }
        assert_eq!(handle.enqueue(event_signal(4)), Err(EnqueueError::RateLimited));
        // 不 panic、不阻塞；spawned 写失败只留 warn。
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    // ③ 合并窗已废：每条准入的同键信号各占一次小时预算（20/h），无聚合记账。
    #[tokio::test]
    async fn each_admitted_keyed_signal_counts_against_hourly_budget() {
        pause();
        let (handle, _runs) = spawn_collector();

        for id in 1..=3 {
            handle.enqueue(event_signal(id)).expect("rate limit 3/min");
        }
        for tick in 0..17 {
            handle.enqueue(heartbeat(tick)).expect("within budget");
        }
        assert_eq!(
            handle.enqueue(heartbeat(17)),
            Err(EnqueueError::Throttled),
            "budget = 3 keyed wakes + 17 pass-through wakes = 20"
        );
    }

    // ③ normal signals over the 20/h budget are dropped; Critical and user
    // directives are exempt; the budget window slides after one hour.
    #[tokio::test]
    async fn hourly_budget_drops_normal_exempts_critical_and_directives() {
        pause();
        let (handle, _runs) = spawn_collector();

        for tick in 0..20 {
            handle.enqueue(heartbeat(tick)).expect("under budget");
        }
        assert_eq!(handle.enqueue(heartbeat(20)), Err(EnqueueError::Throttled));

        handle
            .enqueue(critical_signal(99))
            .expect("critical is exempt from the budget");
        handle
            .enqueue(directive("restart the gateway"))
            .expect("user directives are exempt");

        advance(Duration::from_secs(3601)).await;
        handle.enqueue(heartbeat(21)).expect("budget window slid after 1h");
    }

    // ④ same-text user directives dedup for 60s; different text or an
    // expired window passes (O5).
    #[tokio::test]
    async fn same_text_directive_dedup_within_60s() {
        pause();
        let (handle, _runs) = spawn_collector();

        handle
            .enqueue(directive("reboot the gateway"))
            .expect("first directive");
        assert_eq!(
            handle.enqueue(directive("reboot the gateway")),
            Err(EnqueueError::Duplicate),
            "same text within 60s"
        );
        handle
            .enqueue(directive("check status"))
            .expect("different text is not a duplicate");

        advance(Duration::from_secs(61)).await;
        handle
            .enqueue(directive("reboot the gateway"))
            .expect("dedup window expired");
    }

    // ③/O5 queue full: user directives are REJECTED (caller informed),
    // low-priority signals DROPPED with metric.
    #[tokio::test]
    async fn queue_full_rejects_directive_and_drops_low_priority() {
        pause();
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let (gate_run, started_run) = (gate.clone(), started.clone());
        let handle = Scheduler::spawn(
            WS.to_string(),
            move |_sig| {
                let gate = gate_run.clone();
                let started = started_run.clone();
                Box::pin(async move {
                    started.notify_one();
                    gate.notified().await;
                })
            },
            Arc::new(DashMap::new()),
        );

        handle.enqueue(directive("in-flight")).expect("first signal");
        started.notified().await; // consumer now holds one run in flight

        for i in 0..QUEUE_CAPACITY {
            handle.enqueue(directive(&format!("queued-{i}"))).expect("queue slot");
        }
        assert_eq!(
            handle.enqueue(directive("overflow")),
            Err(EnqueueError::Rejected),
            "full queue must reject user directives so the caller can inform the user"
        );
        assert_eq!(
            handle.enqueue(heartbeat(1)),
            Err(EnqueueError::Dropped),
            "full queue drops low-priority signals (agent_wake_dropped)"
        );
        assert_eq!(
            handle.enqueue(critical_signal(42)),
            Err(EnqueueError::Dropped),
            "Critical bypasses the throttle but is still dropped on a full queue"
        );

        gate.notify_waiters(); // let the in-flight run finish; remaining tasks abort with the runtime
    }

    // O26 drain(): 清 ready 队列（合并窗已废，ready 队列是唯一待清面）；
    // 在途 run 不取消，drain 等其完成；drain 后调度器仍存活。
    #[tokio::test]
    async fn drain_clears_ready_queue_and_stays_live() {
        pause();
        let blocking = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let (log_tx, mut log_rx) = mpsc::unbounded_channel();
        let (blocking_run, gate_run, started_run) = (blocking.clone(), gate.clone(), started.clone());
        let handle = Scheduler::spawn(
            WS.to_string(),
            move |sig| {
                let tx = log_tx.clone();
                let blocking = blocking_run.clone();
                let gate = gate_run.clone();
                let started = started_run.clone();
                Box::pin(async move {
                    let _ = tx.send(sig);
                    started.notify_one();
                    while blocking.load(Ordering::SeqCst) {
                        gate.notified().await;
                    }
                })
            },
            Arc::new(DashMap::new()),
        );

        handle.enqueue(directive("in-flight")).expect("in-flight");
        started.notified().await; // consumer blocked inside the first run

        handle.enqueue(directive("queued-1")).expect("queued directive");
        handle.enqueue(critical_signal(7)).expect("queued critical");
        handle.enqueue(event_signal(1)).expect("queued keyed signal");

        // Drain while the first run is still blocked: drain must wait for
        // the in-flight run, then purge everything pending.
        let draining = {
            let handle = handle.clone();
            tokio::spawn(async move { handle.drain().await })
        };
        settle().await;
        assert!(
            !draining.is_finished(),
            "drain must stay pending while a run is in flight (consumer cannot ack yet)"
        );
        blocking.store(false, Ordering::SeqCst);
        gate.notify_waiters();
        draining.await.expect("drain completes after consumer ack");

        handle.enqueue(directive("after-drain")).expect("scheduler stays live");

        let first = log_rx.recv().await.expect("in-flight run logged before drain");
        assert!(matches!(first.source, TriggerSource::UserDirective { .. }));
        let after = log_rx.recv().await.expect("post-drain directive runs");
        assert!(matches!(after.source, TriggerSource::UserDirective { ref text, .. } if text == "after-drain"));
        assert!(
            log_rx.try_recv().is_err(),
            "queued-1, critical and the keyed signal must have been drained"
        );
    }

    // ⑤ runs execute strictly serially — never two in flight at once.
    #[tokio::test]
    async fn runs_execute_serially() {
        pause();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());
        let (if_run, max_run, st_run, rel_run) =
            (in_flight.clone(), max_seen.clone(), started.clone(), release.clone());
        let handle = Scheduler::spawn(
            WS.to_string(),
            move |_sig| {
                let in_flight = if_run.clone();
                let max_seen = max_run.clone();
                let started = st_run.clone();
                let release = rel_run.clone();
                started.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    let n = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(n, Ordering::SeqCst);
                    release.notified().await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
            },
            Arc::new(DashMap::new()),
        );

        handle.enqueue(critical_signal(1)).expect("first");
        handle.enqueue(directive("second")).expect("second");

        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            in_flight.load(Ordering::SeqCst),
            1,
            "second run must not start while the first is active"
        );

        release.notify_one(); // finish run 1 → run 2 may start
        while started.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1, "no two runs may overlap");

        release.notify_one();
    }

    // Regression (review I1): an ack must never be lost, regardless of
    // whether a waiter is already registered — the old notify_waiters()
    // dropped the unregistered case and hung drain() forever.
    #[tokio::test]
    async fn drain_ack_is_never_lost() {
        let drain = DrainState::default();
        let mut last = 0;

        // Order A: ack lands while NO waiter is registered. The stored
        // permit must still release a waiter created afterwards.
        drain.ack(&drain.consumer_acked, &mut last, 1);
        wait_ack(&drain, &drain.consumer_acked, 1).await;

        // Order B: waiter registered (enabled) BEFORE the ack — the ack
        // must wake it (this is the check-then-register gap in wait_ack).
        let notified = drain.ack_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        drain.ack(&drain.consumer_acked, &mut last, 2);
        notified.await;
        assert_eq!(drain.consumer_acked.load(Ordering::SeqCst), 2);
    }

    // Regression (review I2): two concurrent drain() calls must BOTH
    // complete — drains are serialized so each observes acks for its own
    // generation (previously the second drain's target was unreachable).
    #[tokio::test]
    async fn concurrent_drains_both_complete() {
        pause();
        let blocking = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let (blocking_run, gate_run, started_run) = (blocking.clone(), gate.clone(), started.clone());
        let handle = Scheduler::spawn(
            WS.to_string(),
            move |_sig| {
                let blocking = blocking_run.clone();
                let gate = gate_run.clone();
                let started = started_run.clone();
                Box::pin(async move {
                    started.notify_one();
                    while blocking.load(Ordering::SeqCst) {
                        gate.notified().await;
                    }
                })
            },
            Arc::new(DashMap::new()),
        );

        handle.enqueue(directive("in-flight")).expect("enqueue");
        started.notified().await; // consumer blocked inside the run

        let d1 = tokio::spawn({
            let handle = handle.clone();
            async move { handle.drain().await }
        });
        settle().await;
        let d2 = tokio::spawn({
            let handle = handle.clone();
            async move { handle.drain().await }
        });
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(!d1.is_finished(), "drain 1 waits for the in-flight run");
        assert!(!d2.is_finished(), "drain 2 queues behind drain 1");

        blocking.store(false, Ordering::SeqCst);
        gate.notify_waiters();
        for _ in 0..100 {
            if d1.is_finished() && d2.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(d1.is_finished(), "drain 1 completes after the run");
        assert!(d2.is_finished(), "concurrent drain 2 also completes (serialized)");
        d1.await.expect("drain 1");
        d2.await.expect("drain 2");
    }

    // Regression (review M4): a directive REJECTED on a full queue must not
    // poison the 60s dedup table — the user can retry the same text.
    #[tokio::test]
    async fn rejected_directive_stays_retryable() {
        pause();
        let blocking = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let (blocking_run, gate_run, started_run) = (blocking.clone(), gate.clone(), started.clone());
        let handle = Scheduler::spawn(
            WS.to_string(),
            move |_sig| {
                let blocking = blocking_run.clone();
                let gate = gate_run.clone();
                let started = started_run.clone();
                Box::pin(async move {
                    started.notify_one();
                    while blocking.load(Ordering::SeqCst) {
                        gate.notified().await;
                    }
                })
            },
            Arc::new(DashMap::new()),
        );

        handle.enqueue(directive("in-flight")).expect("first signal");
        started.notified().await; // consumer holds one run in flight
        for i in 0..QUEUE_CAPACITY {
            handle.enqueue(directive(&format!("queued-{i}"))).expect("queue slot");
        }
        assert_eq!(
            handle.enqueue(directive("retry-me")),
            Err(EnqueueError::Rejected),
            "full queue rejects the directive"
        );

        // Release the consumer; once a queue slot frees up the SAME text
        // must be accepted (not reported as Duplicate within the 60s window).
        blocking.store(false, Ordering::SeqCst);
        gate.notify_waiters();
        let mut accepted = false;
        for _ in 0..200 {
            match handle.enqueue(directive("retry-me")) {
                Ok(()) => {
                    accepted = true;
                    break;
                }
                Err(EnqueueError::Rejected) => tokio::task::yield_now().await,
                Err(other) => panic!("expected Rejected while draining, got {other:?}"),
            }
        }
        assert!(
            accepted,
            "rejected directive must be retryable, not poisoned as Duplicate"
        );
    }
}

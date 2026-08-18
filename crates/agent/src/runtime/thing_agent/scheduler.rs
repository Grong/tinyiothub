//! Per-workspace wake scheduler — merge window, hourly throttle, directive
//! dedup, serial run execution, drain (T8).
//!
//! Pipeline:
//!
//! ```text
//! triggers → handle.enqueue() ─┬─ Critical / user directive ─→ ready queue (cap 50) ─→ serial consumer → run()
//!                              └─ mergeable (dedup_key) ─────→ merger task (30s window) ─┘
//! ```
//!
//! Behavior rules (T8 brief):
//! ① non-Critical signals with the same `dedup_key` aggregate into a 30s
//!   merge window: collected inside the window, emitted as ONE signal on the
//!   deadline; the merged signal carries every member in arrival order via
//!   [`TriggerSource::Merged`]
//! ② Critical bypasses the merge window AND the hourly throttle (O10)
//! ③ hourly wake budget 20/ws: budgeted signals over budget are dropped
//!   (metric `agent_wake_throttled`); user directives
//!   (`TriggerSource::UserDirective` with `source: None`) are never
//!   throttled — they queue, bounded only by the queue capacity 50 (O5)
//! ④ same-text user directives dedup for 60s (O5)
//! ⑤ serial execution: one run at a time per workspace
//!
//! `drain()` (O26): mode→off clears both the ready queue and pending merge
//! windows, waiting for both tasks to acknowledge.
//!
//! Budget accounting: a wake counts against the hourly budget when a
//! non-Critical, non-user-directive signal is admitted into the ready queue
//! (a 5-signal merge window counts ONCE, at flush). Critical and user
//! directives are exempt and do not consume budget.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};

/// Ready-queue capacity per workspace (O5).
pub const QUEUE_CAPACITY: usize = 50;
/// Merge window for non-Critical signals sharing a `dedup_key`.
pub const MERGE_WINDOW: Duration = Duration::from_secs(30);
/// Max budgeted wakes per workspace per rolling hour.
pub const HOURLY_WAKE_LIMIT: usize = 20;
/// Same-text user directive dedup window (O5).
pub const DIRECTIVE_DEDUP: Duration = Duration::from_secs(60);

type BoxRunFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type RunFn = Arc<dyn Fn(WakeSignal) -> BoxRunFuture + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EnqueueError {
    /// Scheduler shut down (consumer/merger tasks exited).
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
}

pub struct Scheduler;

impl Scheduler {
    /// Spawn the per-workspace scheduler: a merger task (30s windows) and a
    /// serial consumer task driving `run`. Both tasks exit once the returned
    /// handle (and its clones) are dropped.
    pub fn spawn(
        workspace_id: String,
        run: impl Fn(WakeSignal) -> BoxRunFuture + Send + Sync + 'static,
    ) -> SchedulerHandle {
        Self::spawn_with_merge_window(workspace_id, run, MERGE_WINDOW)
    }

    /// [`Scheduler::spawn`] with a custom merge window — integration tests
    /// use sub-second windows so they can run in real time (paused-clock
    /// auto-advance interacts badly with sqlx worker-thread roundtrips).
    pub fn spawn_with_merge_window(
        workspace_id: String,
        run: impl Fn(WakeSignal) -> BoxRunFuture + Send + Sync + 'static,
        merge_window: Duration,
    ) -> SchedulerHandle {
        let (ready_tx, ready_rx) = mpsc::channel(QUEUE_CAPACITY);
        let (ingress_tx, ingress_rx) = mpsc::unbounded_channel();
        let throttle = Arc::new(Mutex::new(Throttle::default()));
        let drain = Arc::new(DrainState::default());
        let run: RunFn = Arc::new(run);

        tokio::spawn(merger_loop(
            workspace_id.clone(),
            ingress_rx,
            ready_tx.clone(),
            throttle.clone(),
            drain.clone(),
            merge_window,
        ));
        tokio::spawn(consumer_loop(ready_rx, run, drain.clone()));

        SchedulerHandle {
            workspace_id,
            ready_tx,
            ingress_tx,
            throttle,
            directive_dedup: Arc::new(Mutex::new(HashMap::new())),
            drain,
        }
    }
}

#[derive(Clone)]
pub struct SchedulerHandle {
    workspace_id: String,
    ready_tx: mpsc::Sender<WakeSignal>,
    ingress_tx: mpsc::UnboundedSender<WakeSignal>,
    throttle: Arc<Mutex<Throttle>>,
    directive_dedup: Arc<Mutex<HashMap<String, Instant>>>,
    drain: Arc<DrainState>,
}

impl SchedulerHandle {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Route a wake signal: Critical and user directives go straight to the
    /// ready queue; other keyed signals join a merge window; everything else
    /// passes through the hourly throttle into the ready queue.
    pub fn enqueue(&self, sig: WakeSignal) -> Result<(), EnqueueError> {
        // ② Critical bypasses the merge window and the throttle (O10).
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

        // ① mergeable: parked in the merger task until the window deadline.
        if sig.dedup_key.is_some() {
            return self.ingress_tx.send(sig).map_err(|_| EnqueueError::Closed);
        }

        // Pass-through normal signal (e.g. heartbeat directive, O24):
        // budgeted, mergeless, droppable.
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

    /// O26: mode→off — clear the ready queue and all pending merge windows.
    /// Waits until both the consumer and the merger have acknowledged, so
    /// signals enqueued after `drain()` returns are never wiped. An
    /// in-flight run is NOT cancelled; drain completes once it finishes.
    ///
    /// Concurrent drains are serialized: acks record only the LATEST
    /// generation, so two overlapping drains would starve the later one.
    pub async fn drain(&self) {
        let _guard = self.drain.lock.lock().await;
        let generation = self.drain.requested.fetch_add(1, Ordering::SeqCst) + 1;
        // Phase 1: the merger acks first — after this it no longer flushes
        // pre-drain windows into the ready queue.
        self.drain.merger_notify.notify_one();
        wait_ack(&self.drain, &self.drain.merger_acked, generation).await;
        // Phase 2: only then the consumer purges the ready queue, so a
        // window flushed just before the drain cannot slip through.
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

/// Drain coordination: `requested` is a monotonic generation; each task
/// records the latest generation it acked after purging its pending state.
#[derive(Default)]
struct DrainState {
    requested: AtomicU64,
    consumer_acked: AtomicU64,
    merger_acked: AtomicU64,
    consumer_notify: Notify,
    merger_notify: Notify,
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

struct MergeWindow {
    signals: Vec<WakeSignal>,
    deadline: Instant,
}

/// ① merger: collects keyed non-Critical signals into per-key windows
/// and flushes one (possibly merged) signal per window into the ready queue.
async fn merger_loop(
    workspace_id: String,
    mut ingress_rx: mpsc::UnboundedReceiver<WakeSignal>,
    ready_tx: mpsc::Sender<WakeSignal>,
    throttle: Arc<Mutex<Throttle>>,
    drain: Arc<DrainState>,
    merge_window: Duration,
) {
    let mut windows: HashMap<String, MergeWindow> = HashMap::new();
    let mut last_gen = 0u64;
    loop {
        let generation = drain.requested.load(Ordering::SeqCst);
        if generation > last_gen {
            windows.clear();
            while ingress_rx.try_recv().is_ok() {}
            drain.ack(&drain.merger_acked, &mut last_gen, generation);
            tracing::info!(workspace_id = %workspace_id, "scheduler drained merge windows");
        }

        let next_deadline = windows.values().map(|w| w.deadline).min();
        // Disarmed (far-future) when no window is open; the `if` guard on
        // the select arm keeps it from firing.
        let deadline = next_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 3600));
        let sleep = tokio::time::sleep_until(deadline);
        tokio::pin!(sleep);

        tokio::select! {
            maybe = ingress_rx.recv() => match maybe {
                Some(sig) => add_to_window(&mut windows, sig, merge_window),
                None => break,
            },
            () = &mut sleep, if next_deadline.is_some() => {
                // Pull everything still buffered in ingress first so signals
                // that arrived before the deadline join this window.
                while let Ok(sig) = ingress_rx.try_recv() {
                    add_to_window(&mut windows, sig, merge_window);
                }
                flush_due_windows(&workspace_id, &mut windows, &throttle, &ready_tx);
            }
            () = drain.merger_notify.notified() => {}
        }
    }
}

fn add_to_window(windows: &mut HashMap<String, MergeWindow>, sig: WakeSignal, merge_window: Duration) {
    let Some(key) = sig.dedup_key.clone() else {
        // enqueue() only routes keyed signals here; drop defensively.
        tracing::warn!(
            metric = "agent_wake_dropped",
            "mergeable signal without dedup_key — dropped"
        );
        return;
    };
    windows
        .entry(key)
        .or_insert_with(|| MergeWindow {
            signals: Vec::new(),
            deadline: Instant::now() + merge_window,
        })
        .signals
        .push(sig);
}

fn flush_due_windows(
    workspace_id: &str,
    windows: &mut HashMap<String, MergeWindow>,
    throttle: &Mutex<Throttle>,
    ready_tx: &mpsc::Sender<WakeSignal>,
) {
    let now = Instant::now();
    let due: Vec<String> = windows
        .iter()
        .filter(|(_, w)| w.deadline <= now)
        .map(|(key, _)| key.clone())
        .collect();
    for key in due {
        let Some(window) = windows.remove(&key) else {
            continue;
        };
        let signal = build_window_signal(workspace_id, key, window.signals);
        let mut throttle = throttle.lock();
        if throttle.over_budget() {
            // ③ merged flush over budget → drop with metric.
            tracing::warn!(
                metric = "agent_wake_throttled",
                workspace_id = %workspace_id,
                "merged window throttled: hourly wake budget exhausted"
            );
            continue;
        }
        match ready_tx.try_send(signal) {
            Ok(()) => throttle.record(),
            Err(_) => {
                tracing::warn!(
                    metric = "agent_wake_dropped",
                    workspace_id = %workspace_id,
                    "merged window dropped: ready queue full"
                );
            }
        }
    }
}

/// One signal flushes unchanged; multiple collapse into a single
/// [`TriggerSource::Merged`] signal carrying all members in arrival order.
fn build_window_signal(workspace_id: &str, key: String, signals: Vec<WakeSignal>) -> WakeSignal {
    if signals.len() == 1 {
        return signals.into_iter().next().expect("window is never empty");
    }
    let priority = signals.iter().map(|s| s.priority).max().expect("window is never empty");
    // Flatten defensively: scheduler input must be raw trigger signals, but
    // a re-enqueued Merged signal must not nest.
    let mut flat = Vec::with_capacity(signals.len());
    for sig in signals {
        match sig.source {
            TriggerSource::Merged { signals: inner } => flat.extend(inner),
            _ => flat.push(sig),
        }
    }
    WakeSignal {
        workspace_id: workspace_id.to_string(),
        priority,
        source: TriggerSource::Merged { signals: flat },
        dedup_key: Some(key),
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
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = Scheduler::spawn(WS.to_string(), move |sig| {
            let tx = tx.clone();
            Box::pin(async move {
                let _ = tx.send(sig);
            })
        });
        (handle, rx)
    }

    /// Let the merger task pick up freshly enqueued signals so window
    /// deadlines anchor at the current (paused) instant.
    async fn settle() {
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
    }

    // ① five same-key signals in one 30s window collapse into a single
    // merged run carrying all five members in arrival order.
    #[tokio::test]
    async fn merge_window_aggregates_five_signals_into_one() {
        pause();
        let (handle, mut runs) = spawn_collector();

        for id in 1..=5 {
            handle.enqueue(event_signal(id)).expect("enqueue");
        }
        settle().await;

        advance(Duration::from_secs(29)).await;
        settle().await;
        assert!(
            runs.try_recv().is_err(),
            "window must not flush before its 30s deadline"
        );

        advance(Duration::from_secs(1)).await;
        let merged = runs.recv().await.expect("merged run at window deadline");
        assert_eq!(merged.workspace_id, WS);
        assert_eq!(merged.priority, Priority::Normal);
        assert_eq!(merged.dedup_key.as_deref(), Some(KEY));
        match &merged.source {
            TriggerSource::Merged { signals } => {
                let ids: Vec<i64> = signals.iter().map(source_event_id).collect();
                assert_eq!(
                    ids,
                    vec![1, 2, 3, 4, 5],
                    "merged context must carry all window events in order"
                );
            }
            other => panic!("expected Merged source, got {other:?}"),
        }
        assert!(runs.try_recv().is_err(), "5 signals must collapse into exactly 1 run");
    }

    // ① merged signal priority is the max of its members.
    #[tokio::test]
    async fn merged_priority_is_max_of_members() {
        pause();
        let (handle, mut runs) = spawn_collector();

        handle.enqueue(event_signal(1)).expect("enqueue");
        let mut high = event_signal(2);
        high.priority = Priority::High;
        handle.enqueue(high).expect("enqueue");
        settle().await;

        advance(Duration::from_secs(30)).await;
        let merged = runs.recv().await.expect("merged run");
        assert_eq!(merged.priority, Priority::High);
    }

    // ① a window with a single signal flushes that signal unchanged
    // (not wrapped in Merged).
    #[tokio::test]
    async fn single_signal_window_flushes_unmerged() {
        pause();
        let (handle, mut runs) = spawn_collector();

        handle.enqueue(event_signal(7)).expect("enqueue");
        settle().await;
        advance(Duration::from_secs(30)).await;

        let sig = runs.recv().await.expect("run at deadline");
        assert_eq!(source_event_id(&sig), 7);
        assert!(runs.try_recv().is_err());
    }

    // ② Critical bypasses the merge window: it runs without any time
    // advance and is never wrapped in Merged (O10).
    #[tokio::test]
    async fn critical_bypasses_merge_window() {
        pause();
        let (handle, mut runs) = spawn_collector();

        handle.enqueue(critical_signal(1)).expect("enqueue");

        // No advance(): if the signal had joined a merge window this recv
        // could never complete under paused time.
        let sig = runs.recv().await.expect("critical runs immediately");
        assert_eq!(sig.priority, Priority::Critical);
        assert_eq!(source_event_id(&sig), 1);

        advance(Duration::from_secs(60)).await;
        settle().await;
        assert!(runs.try_recv().is_err(), "no window artifacts after a critical");
    }

    // ③ a merged flush counts as ONE wake against the 20/h budget.
    #[tokio::test]
    async fn merged_flush_counts_as_one_wake_against_hourly_budget() {
        pause();
        let (handle, mut runs) = spawn_collector();

        for id in 1..=5 {
            handle.enqueue(event_signal(id)).expect("enqueue");
        }
        settle().await;
        advance(Duration::from_secs(30)).await;
        let _merged = runs.recv().await.expect("merged run");

        for tick in 0..19 {
            handle.enqueue(heartbeat(tick)).expect("within budget");
        }
        assert_eq!(
            handle.enqueue(heartbeat(19)),
            Err(EnqueueError::Throttled),
            "budget = 1 merged wake + 19 pass-through wakes = 20"
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
        let handle = Scheduler::spawn(WS.to_string(), move |_sig| {
            let gate = gate_run.clone();
            let started = started_run.clone();
            Box::pin(async move {
                started.notify_one();
                gate.notified().await;
            })
        });

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

    // O26 drain(): clears the ready queue AND pending merge windows while a
    // run is in flight; the scheduler stays live afterwards.
    #[tokio::test]
    async fn drain_clears_ready_queue_and_merge_windows() {
        pause();
        let blocking = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(Notify::new());
        let started = Arc::new(Notify::new());
        let (log_tx, mut log_rx) = mpsc::unbounded_channel();
        let (blocking_run, gate_run, started_run) = (blocking.clone(), gate.clone(), started.clone());
        let handle = Scheduler::spawn(WS.to_string(), move |sig| {
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
        });

        handle.enqueue(directive("in-flight")).expect("in-flight");
        started.notified().await; // consumer blocked inside the first run

        handle.enqueue(directive("queued-1")).expect("queued directive");
        handle.enqueue(critical_signal(7)).expect("queued critical");
        handle.enqueue(event_signal(1)).expect("signal parked in merge window");

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
        draining.await.expect("drain completes after both tasks ack");

        // A merge window that survived would flush at t+30s.
        advance(Duration::from_secs(60)).await;
        settle().await;

        handle.enqueue(directive("after-drain")).expect("scheduler stays live");

        let first = log_rx.recv().await.expect("in-flight run logged before drain");
        assert!(matches!(first.source, TriggerSource::UserDirective { .. }));
        let after = log_rx.recv().await.expect("post-drain directive runs");
        assert!(matches!(after.source, TriggerSource::UserDirective { ref text, .. } if text == "after-drain"));
        assert!(
            log_rx.try_recv().is_err(),
            "queued-1, critical and the windowed signal must have been drained"
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
        let handle = Scheduler::spawn(WS.to_string(), move |_sig| {
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
        });

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
        drain.ack(&drain.merger_acked, &mut last, 1);
        wait_ack(&drain, &drain.merger_acked, 1).await;

        // Order B: waiter registered (enabled) BEFORE the ack — the ack
        // must wake it (this is the check-then-register gap in wait_ack).
        let notified = drain.ack_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        drain.ack(&drain.consumer_acked, &mut last, 1);
        notified.await;
        assert_eq!(drain.consumer_acked.load(Ordering::SeqCst), 1);
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
        let handle = Scheduler::spawn(WS.to_string(), move |_sig| {
            let blocking = blocking_run.clone();
            let gate = gate_run.clone();
            let started = started_run.clone();
            Box::pin(async move {
                started.notify_one();
                while blocking.load(Ordering::SeqCst) {
                    gate.notified().await;
                }
            })
        });

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
        let handle = Scheduler::spawn(WS.to_string(), move |_sig| {
            let blocking = blocking_run.clone();
            let gate = gate_run.clone();
            let started = started_run.clone();
            Box::pin(async move {
                started.notify_one();
                while blocking.load(Ordering::SeqCst) {
                    gate.notified().await;
                }
            })
        });

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

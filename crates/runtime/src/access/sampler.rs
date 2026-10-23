//! 中央采样器（E4b spec §2.2/§3.1）。
//!
//! 每 50ms 一次 tick：列一遍 [`SamplingCatalog`]，对每个 `(workspace, thing,
//! source)` 判断是否 due、是否被需求门控，然后把同一 thing 的 due source
//! 合并成**一次** `submit_read`。
//!
//! 规则（spec §0 裁定）：
//! - 默认全采：`demand_only == false` 的 source 按 `interval_ms` 采
//!   （下限 `min_interval_ms`）。
//! - `health == true`：慢通道常开，忽略需求门控。
//! - `demand_only == true`：只有当前有需求才采；需求消失后保留
//!   [`DEMAND_GRACE`]（30s）宽限，宽限内继续按 interval 采。
//! - Push/stream worker 永不 `submit_read`；改为按活跃 source 集
//!   `set_demand`，集合变化时提交，且至少每 [`DEMAND_RESEND_INTERVAL`]
//!   重发一次（worker 被重建时其 demand watch 是空的）。
//!
//! 需求 = [`DemandTracker`] 的并集：端口快照属性（每 1s 刷新，失败保留上一
//! 份）、显式订阅（source 级）、近期读取（property 级）。property → source
//! 映射走 [`source_for_property`]；`""` 表示 thing 级需求。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use tinyiothub_core::driver::manifest::SourceDecl;

use crate::access::catalog::{SampledThing, SamplingCatalog, source_for_property};
use crate::access::demand::DemandTracker;
use crate::ports::DemandSource;

/// 需求消失后的宽限窗口（spec §0）。
pub const DEMAND_GRACE: Duration = Duration::from_secs(30);
/// 采样 tick 间隔（spec §2.2）。
pub const TICK_INTERVAL: Duration = Duration::from_millis(50);
/// `DemandSource` 快照刷新间隔（spec §2.1）。
pub const SNAPSHOT_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
/// Push worker 活跃集的重发周期：worker 被替换/重建后其 `demand_tx` watch
/// 是空的，集合不变也必须定期重发。
pub const DEMAND_RESEND_INTERVAL: Duration = Duration::from_secs(30);

/// 只有 `poll` / `position` source 进入 worker 的 Read 路径。
/// `position` 是驱动按 schedule 读取的数据源（位置保留字段由 DataServer
/// 消费），轮询语义与 `poll` 相同。
const POLL_KIND: &str = "poll";
const POSITION_KIND: &str = "position";

fn is_sampled_kind(kind: &str) -> bool {
    kind == POLL_KIND || kind == POSITION_KIND
}

/// Sampler 提交读/需求的 worker 面（生产实现包住 DataServer 的 worker 表，
/// 测试用 fake）。
pub trait SamplingWorkers: Send + Sync {
    /// 提交一次读；同一 thing 的多个 due source 必须合并成一次调用。
    fn submit_read(&self, thing_id: &str, sources: Vec<String>);
    /// 更新 push worker 的活跃 source 集。
    fn set_demand(&self, thing_id: &str, sources: Vec<String>);
    /// 该 thing 的 worker 是否为 Push 驱动（无 worker 视为非 push）。
    fn is_push(&self, thing_id: &str) -> bool;
}

type SourceKey = (String, String, String);
type ThingKey = (String, String);

/// 采样器计数器快照（轻量 atomics，不引入指标后端）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SamplerMetrics {
    /// 受需求门控的 source 本进程累计“活跃”的 tick 次数（`demand_only` 且非
    /// health）。按 source·tick 计，不是去重后的 source 数。
    pub demand_active_sources: u64,
    /// 受需求门控的 source 因无需求（且宽限已过）被跳过的 tick 次数。
    pub demand_skipped_sources: u64,
    /// 累计提交给 worker 的 source 读次数（同一 thing 的 due source 合并为
    /// 一次 `submit_read`，这里按合并前的 source 数计）。
    pub reads_submitted: u64,
    /// 调度滞后累计毫秒数：每次到期 source 实际采样时刻晚于 `due_at` 的
    /// 时长之和（首次采样与准点 tick 记 0）。
    pub sampling_lag_ms_total: u64,
    /// 观测到的单次调度滞后最大值（毫秒）。
    pub sampling_lag_ms_max: u64,
}

/// 易失的调度状态（重启后由 catalog/manifest 重建，spec §3.1）。
#[derive(Default)]
struct SamplerState {
    /// `(ws, thing, source) -> 上次提交读的 tick`。
    last_sample: HashMap<SourceKey, Instant>,
    /// `(ws, thing, source) -> 最近一次观察到需求的 tick`（宽限起点）。
    last_demand: HashMap<SourceKey, Instant>,
    /// 门控状态（仅 state 变化时记 `sampling_demand_start/stop`）。
    demand_gate: HashMap<SourceKey, bool>,
    /// push thing 上次下发的活跃 source 集（排序后）。
    last_active: HashMap<ThingKey, Vec<String>>,
    /// push thing 上次 `set_demand` 的 tick——集合不变也按周期重发。
    last_demand_sent: HashMap<ThingKey, Instant>,
    /// 上次快照刷新 tick。
    last_snapshot_at: Option<Instant>,
}

pub struct Sampler {
    catalog: Arc<dyn SamplingCatalog>,
    demand: Arc<DemandTracker>,
    workers: Arc<dyn SamplingWorkers>,
    /// Task 7 之前为 `None`（只用本地订阅/近期读取）。
    demand_source: Option<Arc<dyn DemandSource>>,
    state: Mutex<SamplerState>,
    demand_active_sources: AtomicU64,
    demand_skipped_sources: AtomicU64,
    reads_submitted: AtomicU64,
    sampling_lag_ms_total: AtomicU64,
    sampling_lag_ms_max: AtomicU64,
}

impl Sampler {
    pub fn new(
        catalog: Arc<dyn SamplingCatalog>,
        demand: Arc<DemandTracker>,
        workers: Arc<dyn SamplingWorkers>,
        demand_source: Option<Arc<dyn DemandSource>>,
    ) -> Self {
        Self {
            catalog,
            demand,
            workers,
            demand_source,
            state: Mutex::new(SamplerState::default()),
            demand_active_sources: AtomicU64::new(0),
            demand_skipped_sources: AtomicU64::new(0),
            reads_submitted: AtomicU64::new(0),
            sampling_lag_ms_total: AtomicU64::new(0),
            sampling_lag_ms_max: AtomicU64::new(0),
        }
    }

    /// 当前计数器快照（只读，供诊断/测试）。
    pub fn metrics(&self) -> SamplerMetrics {
        SamplerMetrics {
            demand_active_sources: self.demand_active_sources.load(Ordering::Relaxed),
            demand_skipped_sources: self.demand_skipped_sources.load(Ordering::Relaxed),
            reads_submitted: self.reads_submitted.load(Ordering::Relaxed),
            sampling_lag_ms_total: self.sampling_lag_ms_total.load(Ordering::Relaxed),
            sampling_lag_ms_max: self.sampling_lag_ms_max.load(Ordering::Relaxed),
        }
    }

    /// 50ms 主循环；`demand_source` 存在时按 1s 节奏刷新快照。
    /// shutdown 广播到达即退出。
    pub async fn run(&self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
        let mut ticker = tokio::time::interval(TICK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    tracing::info!("Sampler received shutdown signal");
                    break;
                }
                _ = ticker.tick() => {
                    let now = Instant::now();
                    self.maybe_refresh_snapshot(now).await;
                    self.tick_once(now);
                }
            }
        }
    }

    /// 一次确定性 tick（测试入口）。
    ///
    /// catalog 每 tick 只列一次；同一 thing 的 due source 合并为一次
    /// `submit_read`；push worker 只收 `set_demand`。
    pub fn tick_once(&self, now: Instant) {
        // 订阅/近期读取的过期清理随 tick 执行（spec §2.2）。
        self.demand.gc_at(now);

        let mut state = self.state.lock();
        for thing in self.catalog.list() {
            let thing_key = (thing.workspace_id.clone(), thing.thing_id.clone());
            let push = self.workers.is_push(&thing.thing_id);
            // 每 thing 每 tick 只取一次需求属性集，per-source 门控复用它。
            let demanded = self
                .demand
                .demanded_properties_at(&thing.workspace_id, &thing.thing_id, now);
            let mut active = Vec::new();
            let mut due = Vec::new();

            for source in &thing.sources {
                let (is_active, just_activated) =
                    Self::source_active(&mut state, &self.demand, &thing, source, &demanded, now);
                // 指标只统计受需求门控的 source：health/默认全采恒活跃，
                // 计入会让 active/skipped 失去意义。
                if source.demand_only && !source.health {
                    let counter = if is_active {
                        &self.demand_active_sources
                    } else {
                        &self.demand_skipped_sources
                    };
                    counter.fetch_add(1, Ordering::Relaxed);
                }
                if !is_active {
                    continue;
                }
                active.push(source.id.clone());
                if push || !is_sampled_kind(&source.kind) {
                    continue;
                }
                let key = (thing.workspace_id.clone(), thing.thing_id.clone(), source.id.clone());
                let interval = Duration::from_millis(source.interval_ms.max(source.min_interval_ms));
                if just_activated {
                    // 门控从关到开：把基准拉回“一个 interval 之前”（不早于 now），
                    // 首个 due tick 的滞后不把门控空闲期算进去（M-2）。
                    state
                        .last_sample
                        .insert(key.clone(), now.checked_sub(interval).unwrap_or(now));
                }
                // 首次采样没有 due_at，滞后记 0；此后 due_at = 上次采样 + interval。
                let due_at = state.last_sample.get(&key).map(|last| *last + interval);
                let is_due = due_at.is_none_or(|due_at| now >= due_at);
                if is_due {
                    if let Some(due_at) = due_at {
                        let lag_ms =
                            u64::try_from(now.saturating_duration_since(due_at).as_millis()).unwrap_or(u64::MAX);
                        self.sampling_lag_ms_total.fetch_add(lag_ms, Ordering::Relaxed);
                        self.sampling_lag_ms_max.fetch_max(lag_ms, Ordering::Relaxed);
                    }
                    state.last_sample.insert(key, now);
                    due.push(source.id.clone());
                }
            }

            if push {
                active.sort();
                let changed = state.last_active.get(&thing_key) != Some(&active);
                // 集合不变也周期重发：worker 可能已被替换/重建，其 demand
                // watch 为空，只有重发才能恢复推送（I-2）。
                let stale = state
                    .last_demand_sent
                    .get(&thing_key)
                    .is_none_or(|sent| now.saturating_duration_since(*sent) >= DEMAND_RESEND_INTERVAL);
                if changed || stale {
                    self.workers.set_demand(&thing.thing_id, active.clone());
                    state.last_active.insert(thing_key.clone(), active);
                    state.last_demand_sent.insert(thing_key, now);
                }
            } else if !due.is_empty() {
                // 同一 thing 的所有 due source 一次提交（spec §3.1）。
                self.reads_submitted.fetch_add(due.len() as u64, Ordering::Relaxed);
                self.workers.submit_read(&thing.thing_id, due);
            }
        }
    }

    /// 该 source 本 tick 是否“活跃”（应采/应订阅），以及是否刚从门控关闭
    /// 转为活跃（用于重置滞后基准）。
    ///
    /// 非 `demand_only` 与 `health` 恒活跃；`demand_only` 由需求 + 30s 宽限
    /// 决定，并在门控状态变化时记一次账本事件。
    fn source_active(
        state: &mut SamplerState,
        demand: &DemandTracker,
        thing: &SampledThing,
        source: &SourceDecl,
        demanded_properties: &BTreeSet<String>,
        now: Instant,
    ) -> (bool, bool) {
        if source.health || !source.demand_only {
            return (true, false);
        }

        let key = (thing.workspace_id.clone(), thing.thing_id.clone(), source.id.clone());
        let demanded = has_demand_now(demand, thing, source, demanded_properties, now);
        if demanded {
            state.last_demand.insert(key.clone(), now);
        }
        let in_grace = state
            .last_demand
            .get(&key)
            .is_some_and(|at| now.saturating_duration_since(*at) < DEMAND_GRACE);
        let active = demanded || in_grace;

        let previous = state.demand_gate.insert(key, active);
        if let Some(previous) = previous
            && previous != active
        {
            if active {
                tracing::info!(
                    workspace_id = %thing.workspace_id,
                    thing_id = %thing.thing_id,
                    source = %source.id,
                    "sampling_demand_start"
                );
            } else {
                tracing::info!(
                    workspace_id = %thing.workspace_id,
                    thing_id = %thing.thing_id,
                    source = %source.id,
                    "sampling_demand_stop"
                );
            }
        }
        (active, active && previous == Some(false))
    }

    /// 按 1s 节奏刷新端口快照；失败保留上一份并记 `demand_snapshot_error`。
    async fn maybe_refresh_snapshot(&self, now: Instant) {
        let Some(source) = self.demand_source.clone() else {
            return;
        };
        {
            let state = self.state.lock();
            if state
                .last_snapshot_at
                .is_some_and(|last| now.saturating_duration_since(last) < SNAPSHOT_REFRESH_INTERVAL)
            {
                return;
            }
        }

        // 不在持锁状态下 await。
        match source.snapshot().await {
            Ok(refs) => self.demand.set_snapshot(refs),
            Err(error) => tracing::warn!(
                error = %error,
                "demand_snapshot_error: 快照刷新失败，保留上一份快照"
            ),
        }
        self.state.lock().last_snapshot_at = Some(now);
    }
}

/// 当前是否对该 source 有需求（精确到 source）。
///
/// **不能**使用 `DemandTracker::has_demand_at`：快照属性与近期读取按
/// property 记录、不携带 source，那个方法会把任意 property 需求算到
/// thing 的每个 source 上，导致 `demand_only` 门控失效。这里按 spec 的
/// 三条件判定：
/// 1. 显式订阅命中该 source（source 级）；
/// 2. 需求属性含 `""`（thing 级 health/availability watch）；
/// 3. 需求属性经 [`source_for_property`] 精确映射到该 source。
fn has_demand_now(
    demand: &DemandTracker,
    thing: &SampledThing,
    source: &SourceDecl,
    demanded_properties: &BTreeSet<String>,
    now: Instant,
) -> bool {
    if demand.has_subscription_demand_at(&thing.workspace_id, &thing.thing_id, &source.id, now) {
        return true;
    }
    demanded_properties.iter().any(|property| {
        property.is_empty() || source_for_property(&thing.sources, property).as_deref() == Some(source.id.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use parking_lot::Mutex as PlMutex;

    use crate::ports::DemandPropertyRef;

    struct FakeCatalog {
        things: Vec<SampledThing>,
    }

    impl SamplingCatalog for FakeCatalog {
        fn list(&self) -> Vec<SampledThing> {
            self.things.clone()
        }
    }

    #[derive(Default)]
    struct FakeWorkers {
        reads: PlMutex<Vec<(String, Vec<String>)>>,
        demands: PlMutex<Vec<(String, Vec<String>)>>,
        push: Vec<String>,
    }

    impl FakeWorkers {
        fn push_for(thing_id: &str) -> Self {
            Self {
                push: vec![thing_id.to_string()],
                ..Self::default()
            }
        }

        fn read_count(&self) -> usize {
            self.reads.lock().len()
        }

        fn last_read(&self) -> (String, Vec<String>) {
            self.reads.lock().last().cloned().expect("a read was submitted")
        }

        fn demand_count(&self) -> usize {
            self.demands.lock().len()
        }

        fn last_demand(&self) -> (String, Vec<String>) {
            self.demands.lock().last().cloned().expect("a demand was set")
        }
    }

    impl SamplingWorkers for FakeWorkers {
        fn submit_read(&self, thing_id: &str, sources: Vec<String>) {
            self.reads.lock().push((thing_id.to_string(), sources));
        }

        fn set_demand(&self, thing_id: &str, sources: Vec<String>) {
            self.demands.lock().push((thing_id.to_string(), sources));
        }

        fn is_push(&self, thing_id: &str) -> bool {
            self.push.iter().any(|id| id == thing_id)
        }
    }

    fn src(id: &str, interval_ms: u64, demand_only: bool, health: bool) -> SourceDecl {
        SourceDecl {
            id: id.into(),
            kind: "poll".into(),
            fields: vec!["level".into()],
            interval_ms,
            min_interval_ms: 200,
            demand_only,
            health,
        }
    }

    fn thing(workspace_id: &str, thing_id: &str, sources: Vec<SourceDecl>) -> SampledThing {
        SampledThing {
            workspace_id: workspace_id.into(),
            thing_id: thing_id.into(),
            driver_name: "simulator".into(),
            sources,
        }
    }

    fn sampler(catalog: Vec<SampledThing>, workers: Arc<FakeWorkers>) -> Sampler {
        Sampler::new(
            Arc::new(FakeCatalog { things: catalog }),
            Arc::new(DemandTracker::new()),
            workers,
            None,
        )
    }

    fn sampler_with_demand(catalog: Vec<SampledThing>, workers: Arc<FakeWorkers>) -> (Sampler, Arc<DemandTracker>) {
        let demand = Arc::new(DemandTracker::new());
        let sampler = Sampler::new(Arc::new(FakeCatalog { things: catalog }), demand.clone(), workers, None);
        (sampler, demand)
    }

    fn subscribe(
        demand: &DemandTracker,
        workspace: &str,
        thing: &str,
        source: &str,
        at: Instant,
    ) -> crate::access::demand::SubscriptionId {
        demand.subscribe_at(workspace, &[thing.to_string()], &[source.to_string()], DEMAND_GRACE, at)
    }

    #[test]
    fn always_source_samples_every_interval() {
        let workers = Arc::new(FakeWorkers::default());
        let sampler = sampler(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, false, false)])],
            workers.clone(),
        );
        let base = Instant::now();

        sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["telemetry".into()]));

        // 未到 interval：不采。
        sampler.tick_once(base + Duration::from_millis(500));
        assert_eq!(workers.read_count(), 1);

        sampler.tick_once(base + Duration::from_millis(1000));
        assert_eq!(workers.read_count(), 2);
    }

    /// E4d：`kind=position` 是驱动按 schedule 读取的数据源，采样器必须像
    /// `poll` 一样轮询（否则 `placement.fromSource` 指向 position 的驱动
    /// 永远拿不到读数，位置摄取形同虚设）。
    #[test]
    fn position_source_is_sampled_like_poll() {
        let workers = Arc::new(FakeWorkers::default());
        let mut position = src("gps", 1000, false, false);
        position.kind = "position".into();
        let position_sampler = sampler(vec![thing("ws", "t1", vec![position])], workers.clone());
        let base = Instant::now();

        position_sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["gps".into()]));
        position_sampler.tick_once(base + Duration::from_millis(500));
        assert_eq!(workers.read_count(), 1, "未到 interval 不重复采");
        position_sampler.tick_once(base + Duration::from_millis(1000));
        assert_eq!(workers.read_count(), 2);

        // push/stream 仍只走 set_demand，绝不 submit_read。
        let workers = Arc::new(FakeWorkers::default());
        let mut push = src("events", 1000, false, false);
        push.kind = "push".into();
        let push_sampler = sampler(vec![thing("ws", "t1", vec![push])], workers.clone());
        push_sampler.tick_once(base);
        assert_eq!(workers.read_count(), 0);
    }

    #[test]
    fn interval_is_clamped_to_min_interval() {
        let workers = Arc::new(FakeWorkers::default());
        let mut source = src("telemetry", 100, false, false);
        source.min_interval_ms = 500;
        let sampler = sampler(vec![thing("ws", "t1", vec![source])], workers.clone());
        let base = Instant::now();

        sampler.tick_once(base);
        sampler.tick_once(base + Duration::from_millis(300));
        assert_eq!(workers.read_count(), 1, "interval 100ms 必须被 clamp 到 500ms");
        sampler.tick_once(base + Duration::from_millis(500));
        assert_eq!(workers.read_count(), 2);
    }

    #[test]
    fn demand_only_source_skips_without_demand() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, _demand) = sampler_with_demand(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, true, false)])],
            workers.clone(),
        );
        let base = Instant::now();

        sampler.tick_once(base);
        assert_eq!(workers.read_count(), 0, "无需求时 demand_only 不采");
        sampler.tick_once(base + Duration::from_secs(10));
        assert_eq!(workers.read_count(), 0);
        assert_eq!(sampler.metrics().demand_skipped_sources, 2);
        assert_eq!(sampler.metrics().demand_active_sources, 0);
        assert_eq!(sampler.metrics().reads_submitted, 0);
    }

    #[test]
    fn demand_only_source_samples_with_source_subscription() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, true, false)])],
            workers.clone(),
        );
        let base = Instant::now();
        subscribe(&demand, "ws", "t1", "telemetry", base);

        sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["telemetry".into()]));

        // 其他 source 的订阅不激活本 source。
        let other = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing(
                "ws",
                "t1",
                vec![src("telemetry", 1000, true, false), src("energy", 1000, true, false)],
            )],
            other.clone(),
        );
        subscribe(&demand, "ws", "t1", "energy", base);
        sampler.tick_once(base);
        assert_eq!(other.last_read(), ("t1".into(), vec!["energy".into()]));
    }

    /// 指标跟随门控转换：skipped → active；reads_submitted 按 source 计。
    #[test]
    fn metrics_track_gate_transitions_and_reads() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing(
                "ws",
                "t1",
                vec![src("slow", 1000, true, false), src("health", 1000, false, true)],
            )],
            workers.clone(),
        );
        let base = Instant::now();

        // 无需求：slow 跳过，health 照采。
        sampler.tick_once(base);
        assert_eq!(
            sampler.metrics(),
            SamplerMetrics {
                demand_active_sources: 0,
                demand_skipped_sources: 1,
                reads_submitted: 1,
                sampling_lag_ms_total: 0,
                sampling_lag_ms_max: 0,
            }
        );
        assert_eq!(workers.last_read(), ("t1".into(), vec!["health".into()]));

        // 订阅 slow：下一次 due tick 同时采 slow + health。
        subscribe(&demand, "ws", "t1", "slow", base);
        sampler.tick_once(base + Duration::from_secs(1));
        assert_eq!(
            sampler.metrics(),
            SamplerMetrics {
                demand_active_sources: 1,
                demand_skipped_sources: 1,
                reads_submitted: 3,
                sampling_lag_ms_total: 0,
                sampling_lag_ms_max: 0,
            }
        );
        assert_eq!(workers.last_read(), ("t1".into(), vec!["slow".into(), "health".into()]));
    }

    /// `sampling_lag_ms`：准点 tick 记 0，迟到 tick 记 `now - due_at`。
    #[test]
    fn sampling_lag_is_zero_on_time_and_positive_when_late() {
        let workers = Arc::new(FakeWorkers::default());
        let sampler = sampler(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, false, false)])],
            workers.clone(),
        );
        let base = Instant::now();

        // 首次采样没有 due_at → 0。
        sampler.tick_once(base);
        assert_eq!(sampler.metrics().sampling_lag_ms_total, 0);
        assert_eq!(sampler.metrics().sampling_lag_ms_max, 0);

        // due_at = base + 1000，晚 250ms。
        sampler.tick_once(base + Duration::from_millis(1250));
        assert_eq!(sampler.metrics().sampling_lag_ms_total, 250);
        assert_eq!(sampler.metrics().sampling_lag_ms_max, 250);

        // due_at = 1250 + 1000 = 2250，准点 → 不增加。
        sampler.tick_once(base + Duration::from_millis(2250));
        assert_eq!(sampler.metrics().sampling_lag_ms_total, 250);
        assert_eq!(sampler.metrics().sampling_lag_ms_max, 250);

        // due_at = 3250，晚 350ms → total 600、max 350。
        sampler.tick_once(base + Duration::from_millis(3600));
        assert_eq!(sampler.metrics().sampling_lag_ms_total, 600);
        assert_eq!(sampler.metrics().sampling_lag_ms_max, 350);
        assert_eq!(workers.read_count(), 4);
    }

    /// M-2：`demand_only` source 重新激活时，首个 due tick 的滞后不得把
    /// 门控空闲期算进去（基准重置为 `now - interval`）。
    #[test]
    fn sampling_lag_ignores_reactivation_idle() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, true, false)])],
            workers.clone(),
        );
        let base = Instant::now();

        // 无需求：跳过，不产生滞后。
        sampler.tick_once(base);
        assert_eq!(workers.read_count(), 0);

        // 需求出现（首次激活）：立即采，滞后 0。
        subscribe(&demand, "ws", "t1", "telemetry", base);
        sampler.tick_once(base + Duration::from_secs(10));
        assert_eq!(workers.read_count(), 1);
        assert_eq!(sampler.metrics().sampling_lag_ms_total, 0);

        // 需求过期（宽限 30s 后）→ 门控关闭，期间继续推进时钟。
        sampler.tick_once(base + Duration::from_secs(41));
        assert_eq!(workers.read_count(), 1, "宽限结束后停采");

        // 需求再次出现：若沿用旧 last_sample，滞后会是 ~30s。
        subscribe(&demand, "ws", "t1", "telemetry", base + Duration::from_secs(41));
        sampler.tick_once(base + Duration::from_secs(41));
        assert_eq!(workers.read_count(), 2, "重新激活后立即采");
        assert_eq!(
            sampler.metrics().sampling_lag_ms_total,
            0,
            "重新激活的空闲期不得计入 sampling_lag_ms"
        );
        assert_eq!(sampler.metrics().sampling_lag_ms_max, 0);
    }

    #[test]
    fn demand_only_source_samples_with_matching_property_demand() {
        let workers = Arc::new(FakeWorkers::default());
        let mut source = src("telemetry", 1000, true, false);
        source.fields = vec!["level".into()];
        let (sampler, demand) = sampler_with_demand(vec![thing("ws", "t1", vec![source])], workers.clone());
        let base = Instant::now();

        // 快照引用映射到只有一个 poll source 的 thing。
        demand.set_snapshot(vec![DemandPropertyRef {
            workspace_id: "ws".into(),
            thing_id: "t1".into(),
            property: "level".into(),
        }]);
        sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["telemetry".into()]));

        // thing 级（""）需求也激活 source。
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, true, false)])],
            workers.clone(),
        );
        demand.set_snapshot(vec![DemandPropertyRef {
            workspace_id: "ws".into(),
            thing_id: "t1".into(),
            property: String::new(),
        }]);
        sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["telemetry".into()]));
    }

    /// 回归：property 需求只能激活其映射到的 source，不得因
    /// `has_demand_at` 的 thing 级合并而激活所有 demand_only source。
    #[test]
    fn per_source_demand_does_not_activate_other_sources() {
        let base = Instant::now();
        let ab_sources = || {
            let mut a = src("source_a", 1000, true, false);
            a.fields = vec!["a".into()];
            let mut b = src("source_b", 1000, true, false);
            b.fields = vec!["b".into()];
            vec![a, b]
        };
        let snapshot = |property: &str| {
            vec![DemandPropertyRef {
                workspace_id: "ws".into(),
                thing_id: "t1".into(),
                property: property.into(),
            }]
        };

        // 快照属性 "a" → 只采 A。
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(vec![thing("ws", "t1", ab_sources())], workers.clone());
        demand.set_snapshot(snapshot("a"));
        sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["source_a".into()]));

        // 订阅 source B → 只采 B。
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(vec![thing("ws", "t1", ab_sources())], workers.clone());
        subscribe(&demand, "ws", "t1", "source_b", base);
        sampler.tick_once(base);
        assert_eq!(workers.last_read(), ("t1".into(), vec!["source_b".into()]));

        // thing 级 `""` 需求 → 两个 source 都采。
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(vec![thing("ws", "t1", ab_sources())], workers.clone());
        demand.set_snapshot(snapshot(""));
        sampler.tick_once(base);
        let (thing_id, mut sources) = workers.last_read();
        sources.sort();
        assert_eq!(thing_id, "t1");
        assert_eq!(sources, vec!["source_a".to_string(), "source_b".to_string()]);
    }

    #[test]
    fn health_source_samples_regardless_of_demand() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, _demand) = sampler_with_demand(
            vec![thing("ws", "t1", vec![src("health", 1000, true, true)])],
            workers.clone(),
        );

        sampler.tick_once(Instant::now());
        assert_eq!(workers.last_read(), ("t1".into(), vec!["health".into()]));
    }

    #[test]
    fn demand_grace_keeps_sampling_then_stops() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing("ws", "t1", vec![src("telemetry", 1000, true, false)])],
            workers.clone(),
        );
        let base = Instant::now();
        let subscription = subscribe(&demand, "ws", "t1", "telemetry", base);

        sampler.tick_once(base);
        assert_eq!(workers.read_count(), 1);

        // 20s 时订阅仍有效：正常采，宽限起点刷新到 20s。
        sampler.tick_once(base + Duration::from_secs(20));
        assert_eq!(workers.read_count(), 2);
        demand.unsubscribe(&subscription);

        // 49s = 需求消失后 29s：宽限内仍采。
        sampler.tick_once(base + Duration::from_secs(49));
        assert_eq!(workers.read_count(), 3, "30s 宽限内必须继续采");

        // 51s = 需求消失后 31s：宽限结束，停采。
        sampler.tick_once(base + Duration::from_secs(51));
        assert_eq!(workers.read_count(), 3, "宽限结束后 demand_only 必须停采");
    }

    #[test]
    fn due_sources_are_grouped_into_one_read_per_thing() {
        let workers = Arc::new(FakeWorkers::default());
        let sampler = sampler(
            vec![thing(
                "ws",
                "t1",
                vec![src("telemetry", 1000, false, false), src("energy", 1000, false, false)],
            )],
            workers.clone(),
        );
        let base = Instant::now();

        sampler.tick_once(base);
        assert_eq!(workers.read_count(), 1, "两个 due source 只能一次 submit_read");
        let (thing_id, mut sources) = workers.last_read();
        sources.sort();
        assert_eq!(thing_id, "t1");
        assert_eq!(sources, vec!["energy".to_string(), "telemetry".to_string()]);
    }

    #[test]
    fn only_due_sources_are_submitted() {
        let workers = Arc::new(FakeWorkers::default());
        let sampler = sampler(
            vec![thing(
                "ws",
                "t1",
                vec![src("fast", 1000, false, false), src("slow", 2000, false, false)],
            )],
            workers.clone(),
        );
        let base = Instant::now();

        sampler.tick_once(base);
        assert_eq!(workers.last_read().1.len(), 2);

        sampler.tick_once(base + Duration::from_millis(1000));
        assert_eq!(workers.last_read(), ("t1".into(), vec!["fast".into()]));
    }

    #[test]
    fn push_worker_gets_set_demand_only_on_changes_and_never_reads() {
        let workers = Arc::new(FakeWorkers::push_for("t1"));
        let (sampler, demand) = sampler_with_demand(
            vec![thing(
                "ws",
                "t1",
                vec![src("always", 1000, false, false), src("gated", 1000, true, false)],
            )],
            workers.clone(),
        );
        let base = Instant::now();

        sampler.tick_once(base);
        assert_eq!(workers.demand_count(), 1);
        assert_eq!(workers.last_demand(), ("t1".into(), vec!["always".into()]));

        // 活跃集不变：不重复下发。
        sampler.tick_once(base + Duration::from_millis(50));
        assert_eq!(workers.demand_count(), 1);

        // 需求出现 → 活跃集变化 → 再下发一次。
        subscribe(&demand, "ws", "t1", "gated", base);
        sampler.tick_once(base + Duration::from_secs(1));
        assert_eq!(workers.demand_count(), 2);
        assert_eq!(
            workers.last_demand(),
            ("t1".into(), vec!["always".into(), "gated".into()])
        );

        // 需求持续：集合不变，保持不下发。
        sampler.tick_once(base + Duration::from_secs(2));
        assert_eq!(workers.demand_count(), 2);

        assert_eq!(workers.read_count(), 0, "push worker 绝不 submit_read");
    }

    /// I-2：活跃集不变时也必须每 30s 重发，覆盖 worker 被重建（新 worker
    /// 的 demand watch 为空）的场景。
    #[test]
    fn push_demand_is_re_sent_periodically_even_when_unchanged() {
        let workers = Arc::new(FakeWorkers::push_for("t1"));
        let sampler = sampler(
            vec![thing("ws", "t1", vec![src("always", 1000, false, false)])],
            workers.clone(),
        );
        let base = Instant::now();

        sampler.tick_once(base);
        assert_eq!(workers.demand_count(), 1);

        // 周期未到：集合不变 → 不重发。
        sampler.tick_once(base + Duration::from_secs(29));
        assert_eq!(workers.demand_count(), 1);

        // 满 30s：即使集合不变也重发一次（新 worker 借此拿到活跃集）。
        sampler.tick_once(base + DEMAND_RESEND_INTERVAL);
        assert_eq!(workers.demand_count(), 2);
        assert_eq!(workers.last_demand(), ("t1".into(), vec!["always".into()]));

        sampler.tick_once(base + Duration::from_secs(59));
        assert_eq!(workers.demand_count(), 2);
        sampler.tick_once(base + Duration::from_secs(60));
        assert_eq!(workers.demand_count(), 3);
    }

    #[test]
    fn push_health_source_is_always_active() {
        let workers = Arc::new(FakeWorkers::push_for("t1"));
        let sampler = sampler(
            vec![thing("ws", "t1", vec![src("health", 1000, true, true)])],
            workers.clone(),
        );

        sampler.tick_once(Instant::now());
        assert_eq!(workers.last_demand(), ("t1".into(), vec!["health".into()]));
    }

    /// 计数/可切换失败的 DemandSource。
    struct FakeDemandSource {
        calls: AtomicUsize,
        fail: bool,
        refs: Vec<DemandPropertyRef>,
    }

    #[async_trait::async_trait]
    impl DemandSource for FakeDemandSource {
        async fn snapshot(&self) -> Result<Vec<DemandPropertyRef>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err("db down".into())
            } else {
                Ok(self.refs.clone())
            }
        }
    }

    fn refs(workspace: &str, thing: &str, property: &str) -> Vec<DemandPropertyRef> {
        vec![DemandPropertyRef {
            workspace_id: workspace.into(),
            thing_id: thing.into(),
            property: property.into(),
        }]
    }

    #[tokio::test]
    async fn snapshot_failure_keeps_last_snapshot_and_cadence_is_one_second() {
        let demand = Arc::new(DemandTracker::new());
        demand.set_snapshot(refs("ws", "t1", "level"));

        let source = Arc::new(FakeDemandSource {
            calls: AtomicUsize::new(0),
            fail: true,
            refs: Vec::new(),
        });
        let sampler = Sampler::new(
            Arc::new(FakeCatalog { things: Vec::new() }),
            demand.clone(),
            Arc::new(FakeWorkers::default()),
            Some(source.clone()),
        );
        let base = Instant::now();

        sampler.maybe_refresh_snapshot(base).await;
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);
        assert!(
            demand.has_demand_for_property_at("ws", "t1", "level", base),
            "失败必须保留上一份快照"
        );

        // 1s 内不重复请求。
        sampler.maybe_refresh_snapshot(base + Duration::from_millis(500)).await;
        assert_eq!(source.calls.load(Ordering::SeqCst), 1);

        // 成功快照整体替换。
        let sampler = Sampler::new(
            Arc::new(FakeCatalog { things: Vec::new() }),
            demand.clone(),
            Arc::new(FakeWorkers::default()),
            Some(Arc::new(FakeDemandSource {
                calls: AtomicUsize::new(0),
                fail: false,
                refs: Vec::new(),
            })),
        );
        sampler.maybe_refresh_snapshot(base + Duration::from_secs(2)).await;
        assert!(!demand.has_demand_for_property_at("ws", "t1", "level", base + Duration::from_secs(2)));
    }

    #[test]
    fn workspace_scoping_is_respected() {
        let workers = Arc::new(FakeWorkers::default());
        let (sampler, demand) = sampler_with_demand(
            vec![thing("ws-a", "t1", vec![src("telemetry", 1000, true, false)])],
            workers.clone(),
        );
        let base = Instant::now();
        subscribe(&demand, "ws-b", "t1", "telemetry", base);

        sampler.tick_once(base);
        assert_eq!(workers.read_count(), 0, "跨工作区订阅不得满足需求");
    }
}

//! E4b 退出标准的确定性 E2E（无 sleep）：
//!
//! Hub 订阅（`DemandTracker` 显式订阅）→ sampler tick → `demandOnly` source
//! 按 interval 采样；断开后 30s 宽限结束停采，而 `health` source 继续；
//! push worker 的 `set_demand` = health ∪ 需求 source，且只在集合变化时下发。
//!
//! 全部用注入的 `Instant` 驱动，不睡真实时钟。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tinyiothub_core::driver::manifest::SourceDecl;
use tinyiothub_runtime::access::catalog::{SampledThing, SamplingCatalog};
use tinyiothub_runtime::access::demand::{DemandTracker, SUBSCRIPTION_TTL};
use tinyiothub_runtime::access::sampler::{Sampler, SamplingWorkers};
use tinyiothub_runtime::hub_stream::subscribe_with_guard;

/// 记录 sampler 下发的读/需求（fake worker 面）。
#[derive(Default)]
struct RecordingWorkers {
    reads: Mutex<Vec<(String, Vec<String>)>>,
    demands: Mutex<Vec<(String, Vec<String>)>>,
    push: bool,
}

impl RecordingWorkers {
    fn new(push: bool) -> Self {
        Self {
            push,
            ..Self::default()
        }
    }

    fn read_count(&self) -> usize {
        self.reads.lock().unwrap().len()
    }

    fn last_read(&self) -> Vec<String> {
        self.reads
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a read was submitted")
            .1
    }

    fn demand_count(&self) -> usize {
        self.demands.lock().unwrap().len()
    }

    fn last_demand(&self) -> Vec<String> {
        self.demands
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a demand set was submitted")
            .1
    }
}

impl SamplingWorkers for RecordingWorkers {
    fn submit_read(&self, thing_id: &str, sources: Vec<String>) {
        self.reads.lock().unwrap().push((thing_id.to_string(), sources));
    }

    fn set_demand(&self, thing_id: &str, sources: Vec<String>) {
        self.demands.lock().unwrap().push((thing_id.to_string(), sources));
    }

    fn is_push(&self, thing_id: &str) -> bool {
        self.push && thing_id == "t1"
    }
}

struct FakeCatalog {
    things: Vec<SampledThing>,
}

impl SamplingCatalog for FakeCatalog {
    fn list(&self) -> Vec<SampledThing> {
        self.things.clone()
    }
}

fn source(id: &str, interval_ms: u64, demand_only: bool, health: bool) -> SourceDecl {
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

/// 一个物：`slow` 是 demandOnly poll source，`health` 常开。
fn thing() -> SampledThing {
    SampledThing {
        workspace_id: "ws".into(),
        thing_id: "t1".into(),
        driver_name: "demo".into(),
        sources: vec![source("slow", 1000, true, false), source("health", 1000, false, true)],
    }
}

fn sampler(workers: Arc<RecordingWorkers>) -> (Sampler, Arc<DemandTracker>) {
    let demand = Arc::new(DemandTracker::new());
    let sampler = Sampler::new(
        Arc::new(FakeCatalog { things: vec![thing()] }),
        demand.clone(),
        workers,
        None,
    );
    (sampler, demand)
}

fn subscribe_slow(demand: &DemandTracker, at: Instant) -> u64 {
    demand.subscribe_at("ws", &["t1".to_string()], &["slow".to_string()], SUBSCRIPTION_TTL, at)
}

fn sorted(mut sources: Vec<String>) -> Vec<String> {
    sources.sort();
    sources
}

#[test]
fn hub_subscription_drives_demand_only_source_and_health_continues() {
    let workers = Arc::new(RecordingWorkers::new(false));
    let (sampler, demand) = sampler(workers.clone());
    let base = Instant::now();

    // 无 Hub 连接：slow 被跳过，health 照常采样。
    sampler.tick_once(base);
    assert_eq!(workers.last_read(), vec!["health"]);
    assert_eq!(sampler.metrics().demand_skipped_sources, 1);
    assert_eq!(sampler.metrics().demand_active_sources, 0);

    // Hub 连接订阅 slow：下一次 due tick 同时采 slow + health。
    let subscription = subscribe_slow(&demand, base + Duration::from_secs(1));
    sampler.tick_once(base + Duration::from_secs(1));
    assert_eq!(
        sorted(workers.last_read()),
        vec!["health".to_string(), "slow".to_string()]
    );
    assert_eq!(sampler.metrics().demand_active_sources, 1);
    assert_eq!(workers.read_count(), 2);

    // 断开连接：30s 宽限内 slow 仍按 interval 采（health 不受影响）。
    demand.unsubscribe(&subscription);
    sampler.tick_once(base + Duration::from_secs(20));
    assert!(
        workers.last_read().contains(&"slow".to_string()),
        "宽限内 demandOnly source 必须继续采样"
    );
    assert_eq!(sampler.metrics().demand_active_sources, 2);
    assert_eq!(workers.read_count(), 3);

    // 宽限结束（>30s）：slow 停采，health 继续。
    sampler.tick_once(base + Duration::from_secs(32));
    assert_eq!(workers.last_read(), vec!["health"]);
    assert_eq!(sampler.metrics().demand_skipped_sources, 2);
    assert_eq!(workers.read_count(), 4);

    // 计数器：4 个 tick 共提交 1 + 2 + 2 + 1 个 source 读。
    assert_eq!(sampler.metrics().reads_submitted, 6);
}

/// 跨层：真实 Hub stream `DemandGuard`（连接建立/断开）驱动采样，而不是
/// 直接操作 `DemandTracker`。
#[test]
fn hub_stream_guard_lifecycle_drives_demand_only_sampling() {
    let workers = Arc::new(RecordingWorkers::new(false));
    let (sampler, demand) = sampler(workers.clone());
    let base = Instant::now();

    // 无 Hub 连接：slow 跳过，health 照采。
    sampler.tick_once(base);
    assert_eq!(workers.last_read(), vec!["health"]);

    // Hub stream 连接建立（handler 用的同一入口）。
    let guard = subscribe_with_guard(demand.clone(), "ws", &["t1".to_string()], &["slow".to_string()]);
    sampler.tick_once(base + Duration::from_secs(1));
    assert_eq!(
        sorted(workers.last_read()),
        vec!["health".to_string(), "slow".to_string()]
    );

    // 客户端断开 → guard Drop 立即 unsubscribe；宽限内仍采。
    drop(guard);
    assert!(
        !demand.has_subscription_demand_at("ws", "t1", "slow", base + Duration::from_secs(2)),
        "guard Drop 必须立即移除订阅需求"
    );
    sampler.tick_once(base + Duration::from_secs(20));
    assert!(workers.last_read().contains(&"slow".to_string()));

    // 宽限结束：slow 停采，health 继续。
    sampler.tick_once(base + Duration::from_secs(32));
    assert_eq!(workers.last_read(), vec!["health"]);
    assert_eq!(sampler.metrics().demand_skipped_sources, 2);
}

#[test]
fn push_worker_set_demand_is_health_union_demanded_and_only_on_transitions() {
    let workers = Arc::new(RecordingWorkers::new(true));
    let (sampler, demand) = sampler(workers.clone());
    let base = Instant::now();

    // 首次 tick：只有常开的 health。
    sampler.tick_once(base);
    assert_eq!(workers.demand_count(), 1);
    assert_eq!(workers.last_demand(), vec!["health"]);

    // 慢通道 tick：活跃集不变 → 不重复下发。
    sampler.tick_once(base + Duration::from_millis(50));
    assert_eq!(workers.demand_count(), 1);

    // 需求出现 → health ∪ slow。
    let subscription = subscribe_slow(&demand, base + Duration::from_secs(1));
    sampler.tick_once(base + Duration::from_secs(1));
    assert_eq!(workers.demand_count(), 2);
    assert_eq!(workers.last_demand(), vec!["health", "slow"]);

    // 需求持续：集合不变 → 不下发。
    sampler.tick_once(base + Duration::from_secs(2));
    assert_eq!(workers.demand_count(), 2);

    // 断开后宽限内集合仍是 health ∪ slow（不抖动下发）。
    demand.unsubscribe(&subscription);
    sampler.tick_once(base + Duration::from_secs(3));
    assert_eq!(workers.demand_count(), 2);
    assert_eq!(workers.last_demand(), vec!["health", "slow"]);

    // 宽限结束 → 回到 health。
    sampler.tick_once(base + Duration::from_secs(34));
    assert_eq!(workers.demand_count(), 3);
    assert_eq!(workers.last_demand(), vec!["health"]);

    // push worker 永不 submit_read。
    assert_eq!(workers.read_count(), 0);
}

/// 需求 source 的 tick 级计数与 push 集合一致：无需求只跳过、有需求才活跃。
#[test]
fn demand_metrics_match_the_push_active_set() {
    let workers = Arc::new(RecordingWorkers::new(true));
    let (sampler, demand) = sampler(workers.clone());
    let base = Instant::now();

    sampler.tick_once(base);
    assert_eq!(sampler.metrics().demand_skipped_sources, 1);
    assert_eq!(sampler.metrics().demand_active_sources, 0);

    subscribe_slow(&demand, base);
    sampler.tick_once(base + Duration::from_secs(1));
    assert_eq!(sampler.metrics().demand_skipped_sources, 1);
    assert_eq!(sampler.metrics().demand_active_sources, 1);
    assert_eq!(sampler.metrics().reads_submitted, 0, "push 路径不提交读");
}

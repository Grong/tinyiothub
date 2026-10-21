//! 派生引擎事件驱动重算全链路（spec §三；X4 单引擎化）。
//! 链：模拟源物属性变更事件 → EventBus → Manager 单订阅引擎命中反向索引
//! （def 级 property 过滤）→ 缓存取引用值 → 求值 → sink →（测试内直接消费
//! rx 走共享管线 `DataServer::consume_driver_reading`）→ 园区物缓存更新。
//!
//! E5 时序：Manager.start()/notify_config_changed 返回即订阅已建立（引擎
//! subscribe 同步完成），断言 subscriber_count 后再发布；等待一律用确定性
//! 轮询（wait_until），禁裸 sleep。

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tinyiothub_core::driver::{CancelToken, DriverMode, DriverReading, PropertyChangeSource, ResultValue, ThingDriver};
use tinyiothub_core::models::event::{Event, EventLevel, EventSource, EventType, RichContent, ThingEventType};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_runtime::data_server::DataServer;
use tinyiothub_runtime::driver::DriverWrapper;
use tinyiothub_runtime::driver::derived::DERIVED_EVENTBUS_LAGGED;
use tinyiothub_runtime::driver::derived_manager::DerivedDriverManager;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::{DerivedPropertyQueries, ThingCacheSource};

// ── 测试替身 ──────────────────────────────────────────────────────────

struct FakeCache {
    devices: Mutex<Vec<Thing>>,
}

impl FakeCache {
    fn new(devices: Vec<Thing>) -> Self {
        Self {
            devices: Mutex::new(devices),
        }
    }
}

impl ThingCacheSource for FakeCache {
    fn all(&self) -> Vec<Thing> {
        self.devices.lock().unwrap().clone()
    }
    fn get(&self, id: &str) -> Option<Thing> {
        self.devices.lock().unwrap().iter().find(|d| d.id == id).cloned()
    }
    fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.devices.lock().unwrap().iter().find(|d| d.name == name).cloned()
    }
    fn insert(&self, device: Thing) {
        self.devices.lock().unwrap().push(device);
    }
    fn update(&self, device: Thing) {
        let mut devices = self.devices.lock().unwrap();
        if let Some(existing) = devices.iter_mut().find(|d| d.id == device.id) {
            *existing = device;
        }
    }
    fn remove(&self, id: &str) {
        self.devices.lock().unwrap().retain(|d| d.id != id);
    }
}

/// port 测试替身：派生定义表（可运行中追加——单订阅用例加热更新 owner）+
/// descendant_count 计数（可调）。
struct StubQueries {
    props: Mutex<Vec<ThingProperty>>,
    descendants: Arc<AtomicI64>,
}

impl StubQueries {
    fn new(props: Vec<ThingProperty>, descendants: Arc<AtomicI64>) -> Self {
        Self {
            props: Mutex::new(props),
            descendants,
        }
    }
    fn add_prop(&self, p: ThingProperty) {
        self.props.lock().unwrap().push(p);
    }
    fn remove_props_of(&self, thing_id: &str) {
        self.props.lock().unwrap().retain(|p| p.thing_id != thing_id);
    }
}

#[async_trait::async_trait]
impl DerivedPropertyQueries for StubQueries {
    async fn list_derived(&self) -> Result<Vec<ThingProperty>, String> {
        Ok(self.props.lock().unwrap().clone())
    }
    async fn find_thing_with_properties(&self, _thing_id: &str) -> Result<Option<Thing>, String> {
        Ok(None) // 既有用例缓存预置齐全，不命中补载路径
    }
    async fn count_descendants(&self, _root: &str, _thing_type: Option<&str>) -> Result<i64, String> {
        Ok(self.descendants.load(Ordering::SeqCst))
    }
}

/// 容量 1 的 broadcast 事件源（lag 用例专用——EventBus::new 固定 1000）。
#[derive(Clone)]
struct SmallBus {
    tx: tokio::sync::broadcast::Sender<Event>,
}

impl SmallBus {
    fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(1);
        Self { tx }
    }
    fn send(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}

impl PropertyChangeSource for SmallBus {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

// ── 装配辅助 ──────────────────────────────────────────────────────────

fn meter(id: &str, name: &str, value: Option<&str>) -> Thing {
    let mut prop = ThingProperty {
        id: format!("prop-{id}"),
        thing_id: id.to_string(),
        name: "用电数".to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    };
    if let Some(v) = value {
        prop.set_current_value(v.to_string());
    }
    let mut t = Thing {
        id: id.to_string(),
        name: name.to_string(),
        ..Thing::default()
    };
    t.properties = Some(vec![prop]);
    t
}

fn campus() -> Thing {
    let mut t = Thing {
        id: "campus-1".to_string(),
        name: "园区".to_string(),
        ..Thing::default()
    };
    t.properties = Some(vec![
        ThingProperty {
            id: "prop-total".to_string(),
            thing_id: "campus-1".to_string(),
            name: "total_power".to_string(),
            data_type: Some("float".to_string()),
            ..ThingProperty::default()
        },
        ThingProperty {
            id: "prop-count".to_string(),
            thing_id: "campus-1".to_string(),
            name: "device_count".to_string(),
            data_type: Some("float".to_string()),
            ..ThingProperty::default()
        },
    ]);
    t
}

fn total_power_prop() -> ThingProperty {
    ThingProperty {
        id: "prop-total".to_string(),
        thing_id: "campus-1".to_string(),
        name: "total_power".to_string(),
        source: Some("derived".to_string()),
        expression: Some("{电表1.用电数}+{电表2.用电数}".to_string()),
        refs: Some(
            r#"[{"thing_id":"meter-1","property_name":"用电数"},{"thing_id":"meter-2","property_name":"用电数"}]"#
                .to_string(),
        ),
        ..ThingProperty::default()
    }
}

fn device_count_prop() -> ThingProperty {
    ThingProperty {
        id: "prop-count".to_string(),
        thing_id: "campus-1".to_string(),
        name: "device_count".to_string(),
        source: Some("derived".to_string()),
        expression: Some("descendant_count()".to_string()),
        refs: Some("[]".to_string()),
        ..ThingProperty::default()
    }
}

/// 第二 owner（单订阅用例）：园区2 + 其派生 total_power（引用电表1）。
fn campus2() -> Thing {
    let mut t = Thing {
        id: "campus-2".to_string(),
        name: "园区2".to_string(),
        ..Thing::default()
    };
    t.properties = Some(vec![ThingProperty {
        id: "prop-total-2".to_string(),
        thing_id: "campus-2".to_string(),
        name: "total_power".to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    }]);
    t
}

fn campus2_total_prop() -> ThingProperty {
    ThingProperty {
        id: "prop-total-2".to_string(),
        thing_id: "campus-2".to_string(),
        name: "total_power".to_string(),
        source: Some("derived".to_string()),
        expression: Some("{电表1.用电数} * 2".to_string()),
        refs: Some(r#"[{"thing_id":"meter-1","property_name":"用电数"}]"#.to_string()),
        ..ThingProperty::default()
    }
}

fn prop_change_event(thing_id: &str, prop_id: &str) -> Event {
    Event::new_device_event(
        ThingEventType::PropertyChange,
        EventLevel::Info,
        EventSource::device_property(thing_id.to_string(), prop_id.to_string(), "test".to_string()),
        RichContent::new_text("t".to_string(), "c".to_string()),
    )
    .expect("构造测试事件")
}

struct Rig {
    manager: Arc<DerivedDriverManager>,
    device_cache: Arc<FakeCache>,
    queries: Arc<StubQueries>,
    descendants: Arc<AtomicI64>,
    _consumer: tokio::task::JoinHandle<()>,
}

/// 标准装配：电表1(100)/电表2(200)/园区物；Manager 单引擎（ensure_owner 建
/// defs 句柄 + 首值补偿），sink 读数由消费任务走共享管线回流（与
/// DataServer::run 内同一函数）。
fn assemble(
    meters: Vec<Thing>,
    derived_props: Vec<ThingProperty>,
    events: Arc<dyn PropertyChangeSource>,
    publish_bus: Arc<EventBus>,
) -> Rig {
    let mut things = meters;
    things.push(campus());
    let device_cache = Arc::new(FakeCache::new(things));
    let cache_dyn: Arc<dyn ThingCacheSource> = device_cache.clone();
    let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<DriverReading>(64);
    let descendants = Arc::new(AtomicI64::new(5));
    let queries = Arc::new(StubQueries::new(derived_props, descendants.clone()));

    let manager = Arc::new(DerivedDriverManager::new(
        queries.clone(),
        cache_dyn.clone(),
        sink_tx,
        events,
    ));

    let consumer_cache = cache_dyn.clone();
    let consumer = tokio::spawn(async move {
        while let Some(reading) = sink_rx.recv().await {
            DataServer::consume_driver_reading(reading, &consumer_cache, publish_bus.clone()).await;
        }
    });

    Rig {
        manager,
        device_cache,
        queries,
        descendants,
        _consumer: consumer,
    }
}

async fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if f() {
            return;
        }
        assert!(Instant::now() < deadline, "超时等待: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn cached_prop(cache: &FakeCache, thing_id: &str, prop_name: &str) -> Option<String> {
    cache
        .get(thing_id)
        .and_then(|t| t.properties)
        .and_then(|props| props.into_iter().find(|p| p.name == prop_name))
        .and_then(|p| p.current_value)
}

// ── 用例 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn recompute_on_referenced_property_change() {
    let event_bus = Arc::new(EventBus::new());
    let rig = assemble(
        vec![
            meter("meter-1", "电表1", Some("100")),
            meter("meter-2", "电表2", Some("200")),
        ],
        vec![total_power_prop()],
        event_bus.clone(),
        event_bus.clone(),
    );
    rig.manager.start().await.expect("manager start");

    // E5：start 返回即订阅已建立（bus 自保持 receiver + 引擎单订阅 = 2）。
    assert!(event_bus.subscriber_count() >= 2, "派生引擎订阅必须已建立");

    // 首值补偿先就位（100+200）
    wait_until("首值 total_power=300", || {
        cached_prop(&rig.device_cache, "campus-1", "total_power").as_deref() == Some("300")
    })
    .await;

    // 先订阅再发布（broadcast 只投递订阅之后的事件——rx 只见 meter-1 与
    // 园区 350 两条，首值 300 的园区事件不在缓冲内）。
    let mut event_rx = event_bus.subscribe();

    // 模拟源物管线：缓存先于事件（spec §一 cache_and_publish 顺序）。
    let mut m1 = rig.device_cache.get("meter-1").expect("meter-1");
    m1.properties.as_mut().unwrap()[0].set_current_value("150".to_string());
    rig.device_cache.update(m1);
    event_bus
        .publish(prop_change_event("meter-1", "prop-meter-1"))
        .await
        .unwrap();

    // 重算：150+200=350，且园区物自己也产生 PropertyChange 事件。
    wait_until("重算 total_power=350", || {
        cached_prop(&rig.device_cache, "campus-1", "total_power").as_deref() == Some("350")
    })
    .await;

    let mut campus_event_seen = false;
    for _ in 0..4 {
        let Ok(Ok(event)) = tokio::time::timeout(Duration::from_secs(1), event_rx.recv()).await else {
            break;
        };
        if event.source().thing_id() == Some("campus-1")
            && matches!(event.event_type(), EventType::Device(ThingEventType::PropertyChange))
        {
            assert_eq!(
                event.content().metadata().get("value"),
                Some(&serde_json::Value::String("350".to_string())),
                "园区事件必须携带重算后的新值"
            );
            campus_event_seen = true;
            break;
        }
    }
    assert!(campus_event_seen, "园区物 total_power 变更必须发布 PropertyChange 事件");
}

#[tokio::test]
async fn missing_ref_value_marks_stale_no_write() {
    let event_bus = Arc::new(EventBus::new());
    // 电表2 无值 → total_power 不写入 + stale("等待数据")
    let rig = assemble(
        vec![meter("meter-1", "电表1", Some("100")), meter("meter-2", "电表2", None)],
        vec![total_power_prop()],
        event_bus.clone(),
        event_bus,
    );
    rig.manager.start().await.expect("manager start");

    wait_until("prop-total 标 stale", || {
        rig.manager.derived_status("prop-total").map(|s| s.stale) == Some(true)
    })
    .await;
    let status = rig.manager.derived_status("prop-total").expect("status");
    let reason = status.stale_reason.unwrap_or_default();
    assert!(
        reason.contains("等待数据"),
        "stale_reason 须含\"等待数据\", got: {reason}"
    );
    assert_eq!(
        cached_prop(&rig.device_cache, "campus-1", "total_power"),
        None,
        "缺引用值不写入"
    );
}

#[tokio::test]
async fn startup_full_recompute_produces_first_value() {
    let event_bus = Arc::new(EventBus::new());
    let rig = assemble(
        vec![
            meter("meter-1", "电表1", Some("100")),
            meter("meter-2", "电表2", Some("200")),
        ],
        vec![total_power_prop()],
        event_bus.clone(),
        event_bus,
    );
    rig.manager.start().await.expect("manager start");

    // 不经任何事件，total_power 已有值（首值补偿，spec §三）
    wait_until("首值补偿 total_power=300", || {
        cached_prop(&rig.device_cache, "campus-1", "total_power").as_deref() == Some("300")
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn lagged_triggers_full_recompute_and_metric() {
    // current_thread：start() 返回后派生的订阅循环任务尚未被调度——
    // 同步连发 3 条进容量 1 的通道，接收方首 recv 必得 Lagged(2)（确定性）。
    let small_bus = SmallBus::new();
    let dummy_bus = Arc::new(EventBus::new());
    let rig = assemble(
        vec![
            meter("meter-1", "电表1", Some("100")),
            meter("meter-2", "电表2", Some("200")),
        ],
        vec![total_power_prop()],
        Arc::new(small_bus.clone()),
        dummy_bus,
    );
    rig.manager.start().await.expect("manager start");

    // 缓存先更新到最新值（lag 自愈的输入永远是最新缓存）。
    let mut m1 = rig.device_cache.get("meter-1").expect("meter-1");
    m1.properties.as_mut().unwrap()[0].set_current_value("150".to_string());
    rig.device_cache.update(m1);

    let lagged_before = DERIVED_EVENTBUS_LAGGED.load(Ordering::SeqCst);
    for _ in 0..3 {
        small_bus.send(prop_change_event("meter-1", "prop-meter-1"));
    }

    // 自愈：全量重算使值正确（150+200=350）
    wait_until("lag 自愈 total_power=350", || {
        cached_prop(&rig.device_cache, "campus-1", "total_power").as_deref() == Some("350")
    })
    .await;
    let lagged_after = DERIVED_EVENTBUS_LAGGED.load(Ordering::SeqCst);
    assert!(
        lagged_after >= lagged_before + 2,
        "DERIVED_EVENTBUS_LAGGED 必须 +2, before={lagged_before} after={lagged_after}"
    );
}

#[tokio::test]
async fn thing_deleted_marks_stale_and_hot_updates_index() {
    let event_bus = Arc::new(EventBus::new());
    let rig = assemble(
        vec![
            meter("meter-1", "电表1", Some("100")),
            meter("meter-2", "电表2", Some("200")),
        ],
        vec![total_power_prop()],
        event_bus.clone(),
        event_bus.clone(),
    );
    rig.manager.start().await.expect("manager start");
    wait_until("首值 total_power=300", || {
        cached_prop(&rig.device_cache, "campus-1", "total_power").as_deref() == Some("300")
    })
    .await;

    // 物删除：缓存摘除（DataServer DeviceDeleted 职责）+ 管理面钩子。
    rig.device_cache.remove("meter-1");
    rig.manager.notify_thing_deleted("meter-1").await;

    let status = rig.manager.derived_status("prop-total").expect("status");
    assert!(status.stale, "删除引用物后必须 stale");
    let reason = status.stale_reason.clone().unwrap_or_default();
    assert!(
        reason.contains("引用缺失"),
        "stale_reason 须含\"引用缺失\", got: {reason}"
    );

    // 索引/refs 热更新：被删物的迟到事件不再触发重算（否则 reason 会被
    // 重算覆写为"等待数据"——下面的断言即热更新证明，负向等待有界）。
    event_bus
        .publish(prop_change_event("meter-1", "prop-meter-1"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    let reason_after = rig
        .manager
        .derived_status("prop-total")
        .and_then(|s| s.stale_reason)
        .unwrap_or_default();
    assert!(
        reason_after.contains("引用缺失"),
        "删除后 refs 必须已从驱动摘除（不触发重算）, got: {reason_after}"
    );

    // 正向对照：存活引用物的事件仍触发重算（驱动存活），此时 meter-1 无值
    // → "等待数据"。
    event_bus
        .publish(prop_change_event("meter-2", "prop-meter-2"))
        .await
        .unwrap();
    wait_until("meter-2 事件触发重算", || {
        rig.manager
            .derived_status("prop-total")
            .and_then(|s| s.stale_reason)
            .is_some_and(|r| r.contains("等待数据"))
    })
    .await;
}

#[tokio::test]
async fn tree_change_recomputes_descendant_count() {
    let event_bus = Arc::new(EventBus::new());
    let rig = assemble(
        vec![meter("meter-1", "电表1", Some("100"))],
        vec![device_count_prop()],
        event_bus.clone(),
        event_bus,
    );
    rig.manager.start().await.expect("manager start");
    wait_until("首值 device_count=5", || {
        cached_prop(&rig.device_cache, "campus-1", "device_count").as_deref() == Some("5")
    })
    .await;

    rig.descendants.store(7, Ordering::SeqCst);
    rig.manager.notify_tree_changed("campus-1").await;
    wait_until("子树变更 device_count=7", || {
        cached_prop(&rig.device_cache, "campus-1", "device_count").as_deref() == Some("7")
    })
    .await;
}

/// T1 监督注入测试：一次重算 panic（注入到缓存读取）只杀死该 spawn 任务
/// （首值补偿任务），引擎订阅循环是独立任务仍然存活——下一次触发即自愈。
/// 佐证 T1 降级结论：派生路径无 panic 面，残余风险（宿主 bug）不扩散。
#[tokio::test]
async fn panic_in_one_recompute_does_not_kill_subscriber_loop() {
    struct PanicOnceCache {
        inner: FakeCache,
        armed: Arc<AtomicBool>,
    }
    impl ThingCacheSource for PanicOnceCache {
        fn all(&self) -> Vec<Thing> {
            self.inner.all()
        }
        fn get(&self, id: &str) -> Option<Thing> {
            // I-2 后求值按 thing_id 走 get（owner 自身 get("campus-1") 发生在
            // ensure_owner 捕获路径，不注入——只杀重算任务的引用解析）
            if id != "campus-1" && self.armed.swap(false, Ordering::SeqCst) {
                panic!("T1 注入：重算路径 panic");
            }
            self.inner.get(id)
        }
        fn get_by_name(&self, name: &str) -> Option<Thing> {
            self.inner.get_by_name(name)
        }
        fn insert(&self, device: Thing) {
            self.inner.insert(device);
        }
        fn update(&self, device: Thing) {
            self.inner.update(device);
        }
        fn remove(&self, id: &str) {
            self.inner.remove(id);
        }
    }

    let event_bus = Arc::new(EventBus::new());
    let inner = FakeCache::new(vec![
        meter("meter-1", "电表1", Some("100")),
        meter("meter-2", "电表2", Some("200")),
        campus(),
    ]);
    let armed = Arc::new(AtomicBool::new(true));
    let panic_cache = Arc::new(PanicOnceCache { inner, armed });
    let cache_dyn: Arc<dyn ThingCacheSource> = panic_cache.clone();
    let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<DriverReading>(64);
    let queries = Arc::new(StubQueries::new(vec![total_power_prop()], Arc::new(AtomicI64::new(0))));
    let manager = Arc::new(DerivedDriverManager::new(
        queries,
        cache_dyn.clone(),
        sink_tx,
        event_bus.clone(),
    ));
    let consumer_cache = cache_dyn.clone();
    let bus_for_consume = event_bus.clone();
    let consumer = tokio::spawn(async move {
        while let Some(reading) = sink_rx.recv().await {
            DataServer::consume_driver_reading(reading, &consumer_cache, bus_for_consume.clone()).await;
        }
    });

    manager.start().await.expect("manager start");
    // 首值补偿任务在第一次引用解析（I-2 后按 id get）时 panic——该任务死亡，
    // sink 无读数。给补偿任务一个执行的窗口（正向探测：armed 已被消耗）。
    wait_until("panic 已注入", || !panic_cache.armed.load(Ordering::SeqCst)).await;
    // 注意：swap 先于 panic 执行，armed=false 不代表任务已死；但后续断言
    // 不依赖死亡时刻，只依赖订阅循环仍工作。
    assert_eq!(
        cached_prop(&panic_cache.inner, "campus-1", "total_power"),
        None,
        "panic 任务不产生读数"
    );

    // 订阅循环（独立任务）仍存活：发布事件 → 重算成功 → 值出现（自愈）。
    event_bus
        .publish(prop_change_event("meter-1", "prop-meter-1"))
        .await
        .unwrap();
    wait_until("订阅循环自愈 total_power=300", || {
        cached_prop(&panic_cache.inner, "campus-1", "total_power").as_deref() == Some("300")
    })
    .await;

    drop(consumer);
    let _ = &manager;
}

// ── 终审 I-1 等价改写（X4 单引擎化 + E4b worker）：Manager 不触碰 worker ──
// 原语义：ensure_driver 不得顶掉同 key 物理 Poll wrapper（顶掉 = 轮询/命令
// 静默死亡）。worker 模型下 DataServer 独占 worker 表，Manager 无任何写入
// 入口——本测试钉死等价断言：Manager 启动后同 key 物理 Poll worker 仍在、
// 派生 owner 物不会被注册成 worker（无 worker 即不可能对其 read_data）。
// X4 收尾（Task 3）后 API 侧亦放行物理驱动物配派生（I-1 限制取消，混合
// 模式共存语义见 park_derived_e2e_test）。

struct PollStubDriver {
    thing: Thing,
}

impl ThingDriver for PollStubDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }
    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }
    fn mode(&self) -> DriverMode {
        DriverMode::Poll
    }
    fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, tinyiothub_core::error::Error> {
        if cancel.is_cancelled() {
            return Err(tinyiothub_core::error::Error::Cancelled);
        }
        Ok(vec![])
    }
    fn execute_command(
        &mut self,
        _cmd: &ThingCommand,
        cancel: &CancelToken,
    ) -> Result<bool, tinyiothub_core::error::Error> {
        if cancel.is_cancelled() {
            return Err(tinyiothub_core::error::Error::Cancelled);
        }
        Ok(true)
    }
}

#[tokio::test]
async fn poll_worker_untouched_and_no_worker_for_derived_owner() {
    let device_cache = Arc::new(FakeCache::new(vec![meter("meter-1", "电表1", Some("100")), campus()]));
    let cache_dyn: Arc<dyn ThingCacheSource> = device_cache.clone();
    let event_bus = Arc::new(EventBus::new());
    let data_server = DataServer::new(cache_dyn.clone(), event_bus.clone());
    // 预登记物理 Poll worker 于 owner 同 key——Manager 启动后必须原样保留。
    data_server.register_worker(
        "campus-1",
        DriverWrapper::new(Box::new(PollStubDriver { thing: campus() })),
    );
    let (sink_tx, _sink_rx) = tokio::sync::mpsc::channel::<DriverReading>(64);
    let manager = DerivedDriverManager::new(
        Arc::new(StubQueries::new(vec![total_power_prop()], Arc::new(AtomicI64::new(0)))),
        cache_dyn,
        sink_tx,
        event_bus,
    );
    manager.start().await.expect("manager start");

    assert!(
        data_server.get_driver_overview("campus-1").is_some(),
        "物理 Poll worker 不得被派生引擎影响"
    );
    assert_eq!(
        data_server.worker_count(),
        1,
        "Manager 启动不得新增 worker（无 worker 即轮询循环不会对 owner 物 read_data）"
    );
}

/// X4 单引擎断言：全 workspace 一个 EventBus 订阅——加第二个 owner 不增，
/// owner 删光回落（引擎停）。
#[tokio::test]
async fn engine_uses_single_eventbus_subscription() {
    let event_bus = Arc::new(EventBus::new());
    let baseline = event_bus.subscriber_count(); // bus 自保持 receiver
    let rig = assemble(
        vec![
            meter("meter-1", "电表1", Some("100")),
            meter("meter-2", "电表2", Some("200")),
        ],
        vec![total_power_prop()],
        event_bus.clone(),
        event_bus.clone(),
    );
    rig.manager.start().await.expect("manager start");
    // E5：start 返回即订阅已建立。
    assert_eq!(
        event_bus.subscriber_count(),
        baseline + 1,
        "start 后全 workspace 应只有一个派生订阅"
    );

    // 第二个 owner 物的派生属性（热更新活路径）：索引/defs 句柄登记，
    // 不新增订阅（不是每物一个订阅）。
    rig.device_cache.insert(campus2());
    rig.queries.add_prop(campus2_total_prop());
    rig.manager.notify_config_changed("campus-2").await;
    assert_eq!(event_bus.subscriber_count(), baseline + 1, "第二个 owner 不得新增订阅");

    // 删光全部 owner → 引擎停，订阅回落到基线（abort 异步生效——确定性轮询）。
    rig.manager.notify_thing_deleted("campus-1").await;
    rig.manager.notify_thing_deleted("campus-2").await;
    wait_until("owner 删光后订阅回落基线", || {
        event_bus.subscriber_count() == baseline
    })
    .await;
}

/// 终审 M-a：配置面删掉最后一条派生 → notify_config_changed 回收 owner 登记
/// → 引擎订阅停止（不空转到删物）。
#[tokio::test]
async fn removing_last_derived_stops_engine_subscription() {
    let event_bus = Arc::new(EventBus::new());
    let baseline = event_bus.subscriber_count();
    let rig = assemble(
        vec![
            meter("meter-1", "电表1", Some("100")),
            meter("meter-2", "电表2", Some("200")),
        ],
        vec![total_power_prop()],
        event_bus.clone(),
        event_bus.clone(),
    );
    rig.manager.start().await.expect("manager start");
    assert_eq!(event_bus.subscriber_count(), baseline + 1, "start 后应有一个派生订阅");

    // 管理面删除该 owner 最后一条派生定义，再走配置变更钩子。
    rig.queries.remove_props_of("campus-1");
    rig.manager.notify_config_changed("campus-1").await;

    // defs 空 → owner 登记回收 → 引擎停（abort 异步生效——确定性轮询）。
    wait_until("末条派生删除后订阅回落基线", || {
        event_bus.subscriber_count() == baseline
    })
    .await;
}

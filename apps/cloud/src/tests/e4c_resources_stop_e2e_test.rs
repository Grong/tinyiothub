//! E4c 资源仲裁 + stop 在途取消 E2E（spec `2026-10-22-e4c-resources-stop-design.md`
//! §3 退出标准）。
//!
//! 覆盖三条主线：
//! 1. **并发 busy**：真实 `ArbitrationRegistry` + 真实工具（`create_thing_tools`
//!    产出的 `call_device` / `set_device`）+ 真实 `AccessService`（声明 →
//!    `ResourceClaim`）——同设备同 exclusive 资源第二个调用拿到带持有者与资源
//!    id 的 busy；不同资源互不冲突；shared capacity 允许到容量为止；TTL 到期
//!    后自动可再 claim。
//! 2. **stop 管线**：真实 `StopTool`（组合根接线）+ 真实 `DataServer` + 假驱动。
//!    - 协作驱动：stop 立即返回 `cancellation_requested`（任务仍在途），随后
//!      `CommandCancelled outcome=cancelled` + 恰好一条 `call_cancelled` 账本行；
//!      闸门期间下一次调用被 `stopped:` 拒绝（claim 兜底文案为 `busy: stop:`）。
//!    - 不合作驱动（阻塞 ~1.5s 无视 token）：stop 仍立即返回，迟到返回留痕
//!      `cancelled_unknown` + `call_cancelled_unknown` 账本行。
//!
//! 驱动均为 `DriverMode::Push`（execute-only worker）：临时轮询串不会提交
//! Read，取消终态只可能来自被测命令本身（spec §2.3 "采样读也可能被取消"）。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use tinyiothub_agent::port::tool::Tool;
use tinyiothub_agent::runtime::thing_agent::arbitration::DEVICE_RESOURCE;
use tinyiothub_agent::runtime::thing_agent::seams::composite::CompositeDef;
use tinyiothub_agent::runtime::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL, ResourceMode, ResourceSpec};
use tinyiothub_core::driver::{CancelToken, DriverMode, ResultValue, RetryConfig, ThingDriver};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::{Event, EventType, ThingEventType};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_runtime::access::demand::DemandTracker;
use tinyiothub_runtime::access::registry::ManifestRegistry;
use tinyiothub_runtime::access::resources::ResourceResolver;
use tinyiothub_runtime::access::service::AccessService;
use tinyiothub_runtime::access::validator::AccessCallValidator;
use tinyiothub_runtime::data_server::DataServer;
use tinyiothub_runtime::driver::DriverWrapper;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::HubLedger;
use tinyiothub_skills::trust::ToolSafety;
use tinyiothub_storage::cache::ThingCache;

use crate::domains::agent::host::composite::CompositeExecutor;
use crate::domains::agent::host::tools::thing::PendingActionStore;
use crate::domains::agent::host::tools::{ThingToolContext, create_thing_tools};
use crate::shared::access_ports::{DbAcceptanceStore, DbDeviceStore, DbDriverStore, DbProjectionStore, DbThingSource};
use crate::shared::runtime_ports::ThingCacheAdapter;
use crate::test_utils::seed_test_workspace;

const WS: &str = "ws-e4c";
const TENANT: &str = "tenant-1";

// ── 夹具：真实 DB + 真实 AccessService 声明解析 ───────────────────────

/// 包级资源：actuator/camera exclusive，streams shared capacity=2。
const RESOURCE_MANIFEST: &str = r#"{
  "manifestVersion":"1.0",
  "identity":{"driver":"resource-demo","version":"1.0.0","protocol":"demo",
              "abiVersion":2,"dynamicCapabilities":true},
  "config":{"type":"object"},"tools":[],
  "stateFields":[{"name":"level","type":"integer"}],
  "sources":[],
  "resources":[
    {"id":"actuator","mode":"exclusive","capacity":1},
    {"id":"camera","mode":"exclusive","capacity":1},
    {"id":"streams","mode":"shared","capacity":2}
  ],
  "placement":{"mode":"runtime"},
  "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
}"#;

async fn test_pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("migrations");
    seed_test_workspace(&pool, TENANT, WS).await;
    pool
}

/// 一台 device（可选属性/动作行，供 set/调用前置校验）。
async fn seed_device(pool: &sqlx::SqlitePool, thing_id: &str, properties: &[&str], actions: &[&str]) {
    // 关闭确认门：本文件测资源仲裁/stop，不测 E2 Approval（直达 claim 路径）。
    sqlx::query("UPDATE workspaces SET require_action_confirm = 0 WHERE id = ?")
        .bind(WS)
        .execute(pool)
        .await
        .expect("disable confirm gate");
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, ?, 'device')")
        .bind(thing_id)
        .bind(format!("Thing {thing_id}"))
        .bind(WS)
        .execute(pool)
        .await
        .expect("insert device");
    for name in properties {
        sqlx::query("INSERT INTO thing_properties (id, thing_id, name, data_type) VALUES (?, ?, ?, 'integer')")
            .bind(format!("prop-{name}"))
            .bind(thing_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("insert property");
    }
    for name in actions {
        sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES (?, ?, ?)")
            .bind(format!("act-{name}-{thing_id}"))
            .bind(thing_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("register action");
    }
}

/// 真 AccessService：声明写入 store，包级 resources 来自内置 manifest。
async fn resource_access(pool: &sqlx::SqlitePool, thing_id: &str, tools: Value) -> Arc<AccessService> {
    let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
    let manifests = Arc::new(ManifestRegistry::new());
    manifests.seed_builtin(RESOURCE_MANIFEST).expect("resource manifest");
    let access = Arc::new(AccessService::new(
        std::path::PathBuf::from("."),
        manifests,
        Arc::new(DbDriverStore(db.clone())),
        Arc::new(DbDeviceStore(db.clone())),
        Arc::new(DbProjectionStore(db.clone())),
        Arc::new(DbThingSource(db.clone())),
        Arc::new(DbAcceptanceStore(db.clone())),
        Arc::new(DemandTracker::new()),
    ));
    let decl: tinyiothub_core::driver::declaration::DeviceDeclaration = serde_json::from_value(json!({
        "manifestVersion": "1.0",
        "driver": "resource-demo",
        "driverVersion": "1.0.0",
        "stateFields": [{"name":"level","type":"integer"}],
        "tools": tools,
    }))
    .expect("declaration json");
    db.upsert_device_declaration(&tinyiothub_core::driver::declaration::DeviceDeclarationRecord {
        workspace_id: WS.into(),
        thing_id: thing_id.into(),
        driver_name: decl.driver.clone(),
        driver_version: decl.driver_version.clone(),
        declaration_json: serde_json::to_string(&decl).unwrap(),
        declaration_hash: "e4c-e2e".into(),
        status: "active".into(),
        updated_at: "2026-10-22T00:00:00Z".into(),
    })
    .await
    .expect("declaration row");
    access
}

/// 生产同款工具装配（11 个设备/非设备工具 + hidden 别名）。
fn thing_tools(
    pool: &sqlx::SqlitePool,
    registry: Arc<ArbitrationRegistry>,
    access: Option<Arc<AccessService>>,
    data_server: Option<Arc<DataServer>>,
    event_bus: Option<Arc<EventBus>>,
) -> Vec<(Box<dyn Tool>, ToolSafety)> {
    create_thing_tools(
        pool.clone(),
        WS,
        &ThingToolContext {
            device_cache: None,
            data_server,
            pending_actions: Some(Arc::new(PendingActionStore::new())),
            arbitration: Some(registry),
            live: None,
            event_bus,
            manager: None,
            access: access.as_ref().map(|a| a.clone() as Arc<dyn AccessCallValidator>),
            demand: None,
            resources: access.map(|a| a as Arc<dyn ResourceResolver>),
        },
    )
}

fn take_tool(tools: &mut Vec<(Box<dyn Tool>, ToolSafety)>, name: &str) -> Box<dyn Tool> {
    let idx = tools
        .iter()
        .position(|(tool, _)| tool.name() == name)
        .unwrap_or_else(|| panic!("tool '{name}' 未注册"));
    tools.remove(idx).0
}

fn exclusive(id: &str) -> ResourceSpec {
    ResourceSpec {
        id: id.into(),
        mode: ResourceMode::Exclusive,
        capacity: 1,
    }
}

fn shared(id: &str, capacity: u32) -> ResourceSpec {
    ResourceSpec {
        id: id.into(),
        mode: ResourceMode::Shared,
        capacity,
    }
}

async fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if f() {
            return;
        }
        assert!(Instant::now() < deadline, "超时等待: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ── 1. 并发 busy / 不同资源 / shared capacity / TTL ───────────────────

/// 同设备同 exclusive 资源：run-A 持 claim 期间 run-B 的 call 得到
/// `busy: <holder> holds <resource>`；run-A 结束（Lease drop）后 run-B 完成。
#[tokio::test]
async fn concurrent_call_same_exclusive_resource_second_busy_then_first_completes() {
    let pool = test_pool().await;
    seed_device(&pool, "dev-1", &[], &["reboot"]).await;
    let access = resource_access(
        &pool,
        "dev-1",
        json!([{"name":"reboot","kind":"action","parameters":{"type":"object"},"resources":["actuator"]}]),
    )
    .await;
    let registry = Arc::new(ArbitrationRegistry::default());
    let mut tools = thing_tools(&pool, registry.clone(), Some(access), None, None);
    let call = take_tool(&mut tools, "call_device");

    // run-A 持有一条外部 lease（真实工具调用在驱动执行期间持有 claim 的
    // 覆盖见 `concurrent_real_tool_calls_second_busy_while_first_executes`）。
    let lease_a = registry
        .try_acquire_resources(
            "dev-1",
            "watch-run-A".into(),
            &[exclusive("actuator")],
            DEFAULT_LEASE_TTL,
        )
        .expect("run-A 占 actuator");

    // run-B 并发尝试同一资源 → 失败 ToolResult，文案带持有者 + 资源 id。
    let busy = call
        .execute(json!({"thingId": "dev-1", "action": "reboot"}))
        .await
        .expect("call_device");
    assert!(!busy.success, "同资源并发必须 busy: {busy:?}");
    assert_eq!(
        busy.error.as_deref(),
        Some("busy: watch-run-A holds actuator"),
        "busy 文案必须人读得懂（spec §2.4）"
    );

    // run-A 完成释放 → run-B 放行（无 DataServer → simulated）。
    drop(lease_a);
    let ok = call
        .execute(json!({"thingId": "dev-1", "action": "reboot"}))
        .await
        .expect("call_device");
    assert!(ok.success, "持有者结束后必须放行: {:?}", ok.error);
    assert!(ok.output.contains("simulated"), "无 DataServer 走模拟: {ok:?}");
}

/// Issue 4：claim 必须覆盖真实执行窗口，而不是入队瞬间。
///
/// 慢驱动进入 execute 后阻塞；**两条真实 `call_device` 工具调用**并发——
/// 第一条工具已返回 `executed`（命令已入队/在驱动内执行），第二条必须拿到
/// `busy: <holder> holds <resource>`。旧实现（入队即释放租约）这里会双双
/// 放行。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_real_tool_calls_second_busy_while_first_executes() {
    struct GatedExecuteDriver {
        thing: Thing,
        started: Arc<tokio::sync::Notify>,
        release: Arc<AtomicBool>,
    }

    impl ThingDriver for GatedExecuteDriver {
        fn thing(&self) -> &Thing {
            &self.thing
        }
        fn thing_mut(&mut self) -> &mut Thing {
            &mut self.thing
        }
        fn mode(&self) -> DriverMode {
            DriverMode::Push
        }
        fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
            Ok(Vec::new())
        }
        fn execute_command(&mut self, _cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
            self.started.notify_one();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !self.release.load(Ordering::SeqCst) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(true)
        }
    }

    let pool = test_pool().await;
    let thing_id = "dev-slow-call";
    seed_device(&pool, thing_id, &[], &["reboot"]).await;
    let access = resource_access(
        &pool,
        thing_id,
        json!([{"name":"reboot","kind":"action","parameters":{"type":"object"},"resources":["actuator"]}]),
    )
    .await;

    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(AtomicBool::new(false));
    let rig = stop_rig(
        thing_id,
        Box::new(GatedExecuteDriver {
            thing: stop_thing(thing_id),
            started: started.clone(),
            release: release.clone(),
        }),
    )
    .await;
    let mut tools = thing_tools(
        &pool,
        rig.registry.clone(),
        Some(access),
        Some(rig.server.clone()),
        None,
    );
    let call = take_tool(&mut tools, "call_device");

    // 第一条真实工具调用：入队后工具立即返回 executed。
    let first = call
        .execute(json!({"thingId": thing_id, "action": "reboot"}))
        .await
        .expect("call_device");
    assert!(first.success, "第一条必须下发: {:?}", first.error);
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("driver must enter execute");
    assert!(rig.worker.in_flight(), "第一条命令必须在驱动内执行（不是仅入队）");

    // 第二条真实工具调用：第一条仍在执行 → busy 带持有者 + 资源 id。
    let t0 = Instant::now();
    let second = call
        .execute(json!({"thingId": thing_id, "action": "reboot"}))
        .await
        .expect("call_device");
    assert!(!second.success, "并发第二条必须 busy: {second:?}");
    assert_eq!(
        second.error.as_deref(),
        Some("busy: chat:ws-e4c holds actuator"),
        "busy 必须点名真实执行中的持有者与声明资源"
    );
    assert!(t0.elapsed() < Duration::from_secs(1), "busy 必须立即拒绝（不排队）");

    // 放行第一条 → 执行结束释放租约 → 第三条可行。
    release.store(true, Ordering::SeqCst);
    wait_until("第一条命令执行结束", || !rig.worker.in_flight()).await;
    let third = call
        .execute(json!({"thingId": thing_id, "action": "reboot"}))
        .await
        .expect("call_device");
    assert!(third.success, "执行结束后必须放行: {:?}", third.error);

    let _ = rig.shutdown_tx.send(());
}

/// 不同声明资源互不冲突；TTL 过期的 claim 不再占位。
#[tokio::test]
async fn different_resources_do_not_conflict_and_expired_lease_releases() {
    let pool = test_pool().await;
    seed_device(&pool, "dev-1", &[], &["reboot", "calibrate"]).await;
    let access = resource_access(
        &pool,
        "dev-1",
        json!([
            {"name":"reboot","kind":"action","parameters":{"type":"object"},"resources":["actuator"]},
            {"name":"calibrate","kind":"action","parameters":{"type":"object"},"resources":["camera"]}
        ]),
    )
    .await;
    let registry = Arc::new(ArbitrationRegistry::default());
    let mut tools = thing_tools(&pool, registry.clone(), Some(access), None, None);
    let call = take_tool(&mut tools, "call_device");

    let _actuator = registry
        .try_acquire_resources(
            "dev-1",
            "watch-run-A".into(),
            &[exclusive("actuator")],
            DEFAULT_LEASE_TTL,
        )
        .expect("run-A 占 actuator");

    // camera 上的调用与 actuator 无冲突。
    let ok = call
        .execute(json!({"thingId": "dev-1", "action": "calibrate"}))
        .await
        .expect("call_device");
    assert!(ok.success, "不同资源必须可并行: {:?}", ok.error);

    // camera 被占后同一调用 busy，且点名 camera。
    let _camera = registry
        .try_acquire_resources("dev-1", "watch-run-C".into(), &[exclusive("camera")], DEFAULT_LEASE_TTL)
        .expect("run-C 占 camera");
    let busy = call
        .execute(json!({"thingId": "dev-1", "action": "calibrate"}))
        .await
        .expect("call_device");
    assert!(!busy.success);
    assert_eq!(busy.error.as_deref(), Some("busy: watch-run-C holds camera"));
    drop(_camera);

    // TTL 到期：短 TTL 的 claim 过期后不再占位（持有者崩溃恢复）。
    let short = registry
        .try_acquire_resources(
            "dev-1",
            "crashed-run".into(),
            &[exclusive("camera")],
            Duration::from_millis(20),
        )
        .expect("短 TTL claim");
    std::mem::forget(short); // 模拟持有者崩溃（永不 Drop）：条目只能由 TTL 判过期
    tokio::time::sleep(Duration::from_millis(40)).await;
    let ok = call
        .execute(json!({"thingId": "dev-1", "action": "calibrate"}))
        .await
        .expect("call_device");
    assert!(ok.success, "TTL 过期后必须可再 claim: {:?}", ok.error);
}

/// shared capacity：1/2 占用时 call 与 set 均可进入；2/2 时 busy；
/// 释放一个后又可进入。
#[tokio::test]
async fn shared_capacity_allows_up_to_capacity_for_call_and_set() {
    let pool = test_pool().await;
    seed_device(&pool, "dev-1", &["level"], &["stream_start"]).await;
    let access = resource_access(
        &pool,
        "dev-1",
        json!([
            {"name":"stream_start","kind":"action","parameters":{"type":"object"},"resources":["streams"]},
            {"name":"set_level","kind":"set","targetField":"level",
             "parameters":{"type":"object","properties":{"level":{"type":"integer"}}},
             "resources":["streams"]}
        ]),
    )
    .await;
    let registry = Arc::new(ArbitrationRegistry::default());
    let mut tools = thing_tools(
        &pool,
        registry.clone(),
        Some(access),
        None,
        Some(Arc::new(EventBus::new())),
    );
    let call = take_tool(&mut tools, "call_device");
    let set = take_tool(&mut tools, "set_device");

    let holder_a = registry
        .try_acquire_resources(
            "dev-1",
            "watch-run-A".into(),
            &[shared("streams", 2)],
            DEFAULT_LEASE_TTL,
        )
        .expect("run-A 占 1/2");

    // 1/2 占用：call 与 set 各可用第二个槽位。
    let call_ok = call
        .execute(json!({"thingId": "dev-1", "action": "stream_start"}))
        .await
        .expect("call_device");
    assert!(call_ok.success, "shared 1/2 应允许 call: {:?}", call_ok.error);
    let set_ok = set
        .execute(json!({"thingId": "dev-1", "properties": {"level": 1}}))
        .await
        .expect("set_device");
    assert!(set_ok.success, "shared 1/2 应允许 set: {:?}", set_ok.error);

    // 2/2 占用：两者都 busy，文案点名首个持有者与资源 id。
    let holder_c = registry
        .try_acquire_resources(
            "dev-1",
            "watch-run-C".into(),
            &[shared("streams", 2)],
            DEFAULT_LEASE_TTL,
        )
        .expect("run-C 占 2/2");
    let call_busy = call
        .execute(json!({"thingId": "dev-1", "action": "stream_start"}))
        .await
        .expect("call_device");
    assert!(!call_busy.success, "shared 2/2 必须 busy");
    assert_eq!(call_busy.error.as_deref(), Some("busy: watch-run-A holds streams"));
    let set_busy = set
        .execute(json!({"thingId": "dev-1", "properties": {"level": 2}}))
        .await
        .expect("set_device");
    assert!(!set_busy.success, "shared 2/2 必须 busy");
    assert!(set_busy.error.unwrap_or_default().contains("watch-run-A"));
    drop(holder_c);

    // 释放一个槽位 → 又可用。
    let again = set
        .execute(json!({"thingId": "dev-1", "properties": {"level": 3}}))
        .await
        .expect("set_device");
    assert!(again.success, "释放槽位后必须放行: {:?}", again.error);
    drop(holder_a);
}

// ── 2. stop 管线：真实 StopTool + 真实 DataServer ─────────────────────

/// 协作驱动：执行中响应 CancelToken，但被测试放行后才返回（保证 stop
/// 返回时任务仍在途；放行延迟远小于 1s 宽限）。
struct CooperativeExecuteDriver {
    thing: Thing,
    started: Arc<tokio::sync::Notify>,
    cancelled_seen: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
}

impl ThingDriver for CooperativeExecuteDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }
    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }
    /// execute-only：Poll 轮询串不提交 Read。
    fn mode(&self) -> DriverMode {
        DriverMode::Push
    }
    fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
        Ok(Vec::new())
    }
    fn execute_command(&mut self, _cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error> {
        self.started.notify_one();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cancel.is_cancelled() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        if cancel.is_cancelled() {
            self.cancelled_seen.store(true, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(1);
            while !self.release.load(Ordering::SeqCst) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            return Err(Error::Cancelled);
        }
        Ok(true)
    }
}

/// 不合作驱动：阻塞 ~1.5s，无视 CancelToken；迟到返回触发 unknown。
struct NonCooperativeExecuteDriver {
    thing: Thing,
    started: Arc<tokio::sync::Notify>,
}

impl ThingDriver for NonCooperativeExecuteDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }
    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }
    fn mode(&self) -> DriverMode {
        DriverMode::Push
    }
    fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
        Ok(Vec::new())
    }
    fn execute_command(&mut self, _cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
        self.started.notify_one();
        std::thread::sleep(Duration::from_millis(1500));
        Ok(true)
    }
}

/// 账本替身：记录 (workspace, kind, thing_id, payload)。
#[derive(Default)]
struct RecordingLedger(std::sync::Mutex<Vec<(String, String, Option<String>, Value)>>);

impl RecordingLedger {
    fn rows(&self) -> Vec<(String, String, Option<String>, Value)> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl HubLedger for RecordingLedger {
    async fn append(
        &self,
        workspace_id: &str,
        kind: &str,
        thing_id: Option<&str>,
        payload: Value,
    ) -> Result<(), String> {
        self.0.lock().unwrap().push((
            workspace_id.to_string(),
            kind.to_string(),
            thing_id.map(str::to_string),
            payload,
        ));
        Ok(())
    }
}

struct StopRig {
    server: Arc<DataServer>,
    registry: Arc<ArbitrationRegistry>,
    ledger: Arc<RecordingLedger>,
    events: tokio::sync::broadcast::Receiver<Event>,
    worker: Arc<tinyiothub_runtime::driver::worker::WorkerHandle>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

fn stop_thing(thing_id: &str) -> Thing {
    Thing {
        id: thing_id.to_string(),
        name: thing_id.to_string(),
        workspace_id: Some(WS.to_string()),
        ..Thing::default()
    }
}

async fn stop_rig(thing_id: &str, driver: Box<dyn ThingDriver>) -> StopRig {
    let cache = Arc::new(ThingCache::new());
    cache.insert(stop_thing(thing_id));
    let bus = Arc::new(EventBus::new());
    let events = bus.subscribe();
    let server = Arc::new(DataServer::new(Arc::new(ThingCacheAdapter(cache.clone())), bus.clone()));
    let ledger = Arc::new(RecordingLedger::default());
    server.set_hub_ledger(ledger.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    server.run(shutdown_rx).await.expect("data_server run");
    server.register_worker(thing_id, DriverWrapper::new(driver));
    let worker = server.workers().get(thing_id).expect("registered worker").clone();
    StopRig {
        server,
        registry: Arc::new(ArbitrationRegistry::default()),
        ledger,
        events,
        worker,
        shutdown_tx,
    }
}

fn reboot_command(thing_id: &str) -> ThingCommand {
    ThingCommand {
        id: uuid::Uuid::new_v4().to_string(),
        thing_id: thing_id.to_string(),
        name: "reboot".to_string(),
        display_name: None,
        description: None,
        parameters: None,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

async fn next_cancelled_event(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Event {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("应在 5s 内收到取消终态事件")
            .expect("事件通道应保持打开");
        if event.event_type() == &EventType::Device(ThingEventType::CommandCancelled) {
            return event;
        }
    }
}

/// 协作驱动：stop 立即返回 `cancellation_requested`（任务仍在途），随后
/// 异步 `cancelled` 事件 + 恰好一条 `call_cancelled` 账本；闸门拒绝下一次调用。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_cooperative_in_flight_cancels_within_grace_and_gates_next_call() {
    let started = Arc::new(tokio::sync::Notify::new());
    let cancelled_seen = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let thing_id = "dev-stop";
    let mut rig = stop_rig(
        thing_id,
        Box::new(CooperativeExecuteDriver {
            thing: stop_thing(thing_id),
            started: started.clone(),
            cancelled_seen: cancelled_seen.clone(),
            release: release.clone(),
        }),
    )
    .await;

    let pool = test_pool().await;
    // StopTool 现按 workspace 解析物（Issue 2）——DB 里必须存在同一台设备。
    seed_device(&pool, thing_id, &[], &["reboot"]).await;
    let mut tools = thing_tools(&pool, rig.registry.clone(), None, Some(rig.server.clone()), None);
    let stop = take_tool(&mut tools, "stop");
    let call = take_tool(&mut tools, "call_device");

    // 长任务在途（execute-only worker）。
    rig.worker
        .submit_execute(reboot_command(thing_id))
        .expect("submit execute");
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("execute must start");
    assert!(rig.worker.in_flight(), "提交后应处于在途");

    // stop：置闸门 + cancel_current，立即返回（不等待驱动退出）。
    let t0 = Instant::now();
    let out = stop
        .execute(json!({"thingId": thing_id, "reason": "ops emergency"}))
        .await
        .expect("stop");
    let stop_latency = t0.elapsed();
    assert!(out.success, "{:?}", out.error);
    assert!(
        stop_latency < Duration::from_millis(100),
        "stop 必须立即返回（实测 {stop_latency:?}）"
    );
    let payload: Value = serde_json::from_str(&out.output).expect("stop json");
    assert_eq!(payload["status"], "cancellation_requested");
    assert_eq!(payload["reason"], "ops emergency");
    assert_eq!(payload["inFlight"], 1, "应命中 1 个在途调用");
    assert!(
        rig.worker.in_flight(),
        "stop 返回时驱动任务仍应运行（release 尚未放行）"
    );

    // 闸门期间下一次 call 被 `stopped:` 拒绝（先于声明/claim）。
    let blocked = call
        .execute(json!({"thingId": thing_id, "action": "reboot"}))
        .await
        .expect("call_device");
    assert!(!blocked.success, "stop 闸门期间调用必须拒绝");
    assert_eq!(blocked.error.as_deref(), Some("stopped: ops emergency"));
    // claim 兜底文案同样带 stop 原因（spec §2.1）。
    let busy = rig
        .registry
        .try_acquire_default(thing_id, "late-run".into())
        .expect_err("闸门期间 claim 必须失败");
    assert_eq!(busy.to_string(), "busy: stop:ops emergency holds __device__");
    assert!(busy.resource == DEVICE_RESOURCE);

    // 放行驱动 → 宽限内退出 → cancelled 事件 + 恰好一条 call_cancelled 账本。
    release.store(true, Ordering::SeqCst);
    wait_until("驱动观察到取消", || cancelled_seen.load(Ordering::SeqCst)).await;
    wait_until("call_cancelled 账本行", || rig.ledger.rows().len() == 1).await;

    let rows = rig.ledger.rows();
    assert_eq!(rows.len(), 1, "一次调用恰好一条账本: {rows:?}");
    assert_eq!(rows[0].0, WS);
    assert_eq!(rows[0].1, "call_cancelled");
    assert_eq!(rows[0].2.as_deref(), Some(thing_id));
    assert_eq!(rows[0].3["outcome"], "cancelled");
    assert!(
        rows[0].3["elapsedMs"].as_u64().unwrap_or(u64::MAX) < 1000,
        "宽限内退出: {}",
        rows[0].3
    );
    assert_eq!(rig.server.cancelled_counts(), (1, 0));

    let event = next_cancelled_event(&mut rig.events).await;
    assert_eq!(event.content().metadata().get("outcome"), Some(&json!("cancelled")));
    assert_eq!(event.content().metadata().get("thingId"), Some(&json!(thing_id)));
    assert!(!rig.worker.in_flight(), "终态后 worker 不再在途");

    let _ = rig.shutdown_tx.send(());
}

/// 不合作驱动：stop 立即返回；>1s 后迟到返回留痕 `cancelled_unknown` +
/// `call_cancelled_unknown` 账本（不发 cancelled）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_non_cooperative_driver_records_cancelled_unknown_after_grace() {
    let started = Arc::new(tokio::sync::Notify::new());
    let thing_id = "dev-slow";
    let mut rig = stop_rig(
        thing_id,
        Box::new(NonCooperativeExecuteDriver {
            thing: stop_thing(thing_id),
            started: started.clone(),
        }),
    )
    .await;

    let pool = test_pool().await;
    // StopTool 现按 workspace 解析物（Issue 2）——DB 里必须存在同一台设备。
    seed_device(&pool, thing_id, &[], &["reboot"]).await;
    let mut tools = thing_tools(&pool, rig.registry.clone(), None, Some(rig.server.clone()), None);
    let stop = take_tool(&mut tools, "stop");

    rig.worker
        .submit_execute(reboot_command(thing_id))
        .expect("submit execute");
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("execute must start");
    assert!(rig.worker.in_flight());

    let t0 = Instant::now();
    let out = stop
        .execute(json!({"thingId": thing_id, "reason": "stuck driver"}))
        .await
        .expect("stop");
    assert!(
        t0.elapsed() < Duration::from_millis(100),
        "不合作驱动也必须立即返回（实测 {:?}）",
        t0.elapsed()
    );
    assert!(out.success);
    let payload: Value = serde_json::from_str(&out.output).unwrap();
    assert_eq!(payload["status"], "cancellation_requested");
    assert_eq!(payload["inFlight"], 1);

    // 阻塞 ~1.5s 的驱动迟到返回：>1s 宽限 → cancelled_unknown。
    wait_until("call_cancelled_unknown 账本行", || !rig.ledger.rows().is_empty()).await;
    let rows = rig.ledger.rows();
    assert_eq!(rows.len(), 1, "不得同时留下 cancelled 行: {rows:?}");
    assert_eq!(rows[0].0, WS);
    assert_eq!(rows[0].1, "call_cancelled_unknown");
    assert_eq!(rows[0].2.as_deref(), Some(thing_id));
    assert_eq!(rows[0].3["outcome"], "cancelled_unknown");
    assert!(
        rows[0].3["elapsedMs"].as_u64().unwrap_or(0) >= 1000,
        ">1s 才退出必须判定 unknown: {}",
        rows[0].3
    );
    assert_eq!(rig.server.cancelled_counts(), (0, 1));

    let event = next_cancelled_event(&mut rig.events).await;
    assert_eq!(
        event.content().metadata().get("outcome"),
        Some(&json!("cancelled_unknown"))
    );

    let _ = rig.shutdown_tx.send(());
}

// ── 3. composite run-wide lease 覆盖 worker 执行窗口 ──────────────────

/// 可阻塞/可失败的假驱动：`block` 动作等到测试放行；`failures` 中的动作
/// 返回 Err；其余动作成功并计数。
struct CompositeGateDriver {
    thing: Thing,
    started: Arc<tokio::sync::Notify>,
    release: Arc<AtomicBool>,
    failures: Arc<Mutex<Vec<String>>>,
    executions: Arc<AtomicUsize>,
}

impl ThingDriver for CompositeGateDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }
    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }
    /// execute-only：轮询串不提交 Read。
    fn mode(&self) -> DriverMode {
        DriverMode::Push
    }
    /// 单次终态：失败用例不必等默认重试退避；短 timeout 让重试层在终态
    /// 失败后重置状态，下一步（如回滚）能真正进入驱动而不是返回缓存错误。
    fn retry_config(&self) -> RetryConfig {
        RetryConfig {
            max_attempts: 1,
            base_interval: Duration::from_millis(1),
            max_interval: Duration::from_millis(1),
            timeout: Duration::from_millis(1),
            ..RetryConfig::default()
        }
    }
    fn read_data(&mut self, _cancel: &CancelToken) -> Result<Vec<ResultValue>, Error> {
        Ok(Vec::new())
    }
    fn execute_command(&mut self, cmd: &ThingCommand, _cancel: &CancelToken) -> Result<bool, Error> {
        self.started.notify_one();
        if cmd.name == "block" {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !self.release.load(Ordering::SeqCst) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if self.failures.lock().unwrap().iter().any(|action| action == &cmd.name) {
            return Err(Error::IOError("driver exploded".to_string()));
        }
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }
}

fn composite_def(id: &str, name: &str, steps: Value) -> CompositeDef {
    serde_json::from_value(json!({
        "id": id,
        "workspace_id": WS,
        "name": name,
        "description": "",
        "v": 1,
        "steps": steps,
    }))
    .expect("valid composite definition")
}

/// 核心回归：composite 的第一个 Call 步骤在 worker 内阻塞时，`execute`
/// 不得提前返回；第二个 composite（以及直接 call）在同一资源上必须 busy；
/// 放行后第一条完成并释放 run-wide 租约。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composite_run_lease_covers_worker_execution_and_blocks_second_run() {
    let pool = test_pool().await;
    let thing_id = "dev-composite-block";
    seed_device(&pool, thing_id, &[], &[]).await;

    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(AtomicBool::new(false));
    let executions = Arc::new(AtomicUsize::new(0));
    let rig = stop_rig(
        thing_id,
        Box::new(CompositeGateDriver {
            thing: stop_thing(thing_id),
            started: started.clone(),
            release: release.clone(),
            failures: Arc::new(Mutex::new(Vec::new())),
            executions: executions.clone(),
        }),
    )
    .await;

    let executor = Arc::new(CompositeExecutor {
        pool: pool.clone(),
        workspace_id: WS.into(),
        data_server: Some(rig.server.clone()),
        event_bus: None,
        arbitration: Some(rig.registry.clone()),
        holder: "test".into(),
        resources: None,
    });
    let def = Arc::new(composite_def(
        "c-block",
        "blocker",
        json!([{"op": "call", "thing_id": thing_id, "action": "block", "params": {}}]),
    ));

    let first_executor = executor.clone();
    let first_def = def.clone();
    let first = tokio::spawn(async move { first_executor.execute(&first_def).await });

    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("first composite command must reach the driver");
    assert!(
        !first.is_finished(),
        "composite 必须在 worker 执行完成前保持未返回（旧实现入队即返回）"
    );

    // 第二个 composite：run-wide 租约仍被第一条链持有 → 立即 busy，且不执行步骤。
    let second = executor.execute(&def).await;
    assert_eq!(second["status"], "failed", "{second}");
    assert_eq!(second["steps"].as_array().unwrap().len(), 1, "{second}");
    let error = second["steps"][0]["error"].as_str().unwrap_or_default();
    assert!(error.contains("composite:blocker:test"), "busy 必须点名持有者: {error}");
    assert!(error.contains("占用"), "busy 文案必须人读得懂: {error}");

    // 直接 call 同一资源同样 busy（claim 覆盖 worker 执行窗口）。
    let busy = rig
        .registry
        .try_acquire_default(thing_id, "late-run".into())
        .expect_err("composite 执行期间资源必须仍被持有");
    assert_eq!(busy.holder, "composite:blocker:test");

    // 放行第一条：worker 完成 → composite 返回 → run-wide 租约释放。
    release.store(true, Ordering::SeqCst);
    let first_out = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .expect("first composite must complete after release")
        .expect("first composite task");
    assert_eq!(first_out["status"], "completed", "{first_out}");
    assert_eq!(executions.load(Ordering::SeqCst), 1);

    rig.registry
        .try_acquire_default(thing_id, "after-run".into())
        .expect("composite must release the run-wide lease after completion");
    let _ = rig.shutdown_tx.send(());
}

/// 顺序同资源步骤在真实 worker 下仍逐步通过（每步等待完成，run-wide
/// 租约只 claim 一次，不 self-conflict）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composite_sequential_same_resource_steps_still_complete() {
    let pool = test_pool().await;
    let thing_id = "dev-composite-seq";
    seed_device(&pool, thing_id, &[], &[]).await;

    let executions = Arc::new(AtomicUsize::new(0));
    let rig = stop_rig(
        thing_id,
        Box::new(CompositeGateDriver {
            thing: stop_thing(thing_id),
            started: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(AtomicBool::new(true)),
            failures: Arc::new(Mutex::new(Vec::new())),
            executions: executions.clone(),
        }),
    )
    .await;

    let executor = CompositeExecutor {
        pool: pool.clone(),
        workspace_id: WS.into(),
        data_server: Some(rig.server.clone()),
        event_bus: None,
        arbitration: Some(rig.registry.clone()),
        holder: "test".into(),
        resources: None,
    };
    let def = composite_def(
        "c-seq",
        "sequential",
        json!([
            {"op": "call", "thing_id": thing_id, "action": "first", "params": {}},
            {"op": "call", "thing_id": thing_id, "action": "second", "params": {}}
        ]),
    );

    let out = executor.execute(&def).await;
    assert_eq!(out["status"], "completed", "{out}");
    assert_eq!(out["steps"].as_array().unwrap().len(), 2, "{out}");
    assert_eq!(executions.load(Ordering::SeqCst), 2, "两步都必须真正执行");

    rig.registry
        .try_acquire_default(thing_id, "after-run".into())
        .expect("composite must release the run-wide lease after completion");
    let _ = rig.shutdown_tx.send(());
}

/// worker 回报失败终态：该步标记 failed（错误文案来自 driver），返回后
/// run-wide 租约必须释放。driver 失败后不再下发任何命令，避免重试层的
/// 终态错误缓存干扰本用例。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composite_worker_failure_marks_step_failed_and_releases_run_lease() {
    let pool = test_pool().await;
    let thing_id = "dev-composite-driver-fail";
    seed_device(&pool, thing_id, &[], &[]).await;

    let executions = Arc::new(AtomicUsize::new(0));
    let rig = stop_rig(
        thing_id,
        Box::new(CompositeGateDriver {
            thing: stop_thing(thing_id),
            started: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(AtomicBool::new(true)),
            failures: Arc::new(Mutex::new(vec!["boom".to_string()])),
            executions: executions.clone(),
        }),
    )
    .await;

    let executor = CompositeExecutor {
        pool: pool.clone(),
        workspace_id: WS.into(),
        data_server: Some(rig.server.clone()),
        event_bus: None,
        arbitration: Some(rig.registry.clone()),
        holder: "test".into(),
        resources: None,
    };
    let def = composite_def(
        "c-driver-fail",
        "driver_failure",
        json!([{"op": "call", "thing_id": thing_id, "action": "boom", "params": {}}]),
    );

    let out = executor.execute(&def).await;
    assert_eq!(out["status"], "failed", "{out}");
    let error = out["steps"][0]["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("driver exploded"),
        "等待到 worker 失败终态并标记该步 failed: {error}"
    );
    assert_eq!(executions.load(Ordering::SeqCst), 0, "失败的驱动调用不计成功次数");

    rig.registry
        .try_acquire_default(thing_id, "after-run".into())
        .expect("failed composite must still release the run-wide lease");
    let _ = rig.shutdown_tx.send(());
}

/// 步骤失败（等待路径返回 Err）：标记该步 failed → 回滚已完成步骤（回滚
/// Call 在真实 worker 上真正执行），返回后 run-wide 租约必须释放。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composite_step_failure_rolls_back_and_releases_run_lease() {
    let pool = test_pool().await;
    let thing_id = "dev-composite-fail";
    let missing = "dev-composite-missing";
    seed_device(&pool, thing_id, &[], &[]).await;

    let executions = Arc::new(AtomicUsize::new(0));
    let rig = stop_rig(
        thing_id,
        Box::new(CompositeGateDriver {
            thing: stop_thing(thing_id),
            started: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(AtomicBool::new(true)),
            failures: Arc::new(Mutex::new(Vec::new())),
            executions: executions.clone(),
        }),
    )
    .await;

    let executor = CompositeExecutor {
        pool: pool.clone(),
        workspace_id: WS.into(),
        data_server: Some(rig.server.clone()),
        event_bus: None,
        arbitration: Some(rig.registry.clone()),
        holder: "test".into(),
        resources: None,
    };
    // 第 2 步引用不在 DataServer 缓存中的物 → 等待路径返回 Err（不是入队成功）。
    let def = composite_def(
        "c-fail",
        "failure",
        json!([
            {"op": "call", "thing_id": thing_id, "action": "good", "params": {},
             "rollback": {"op": "call", "thing_id": thing_id, "action": "undo", "params": {}}},
            {"op": "call", "thing_id": missing, "action": "anything", "params": {}}
        ]),
    );

    let out = executor.execute(&def).await;
    assert_eq!(out["status"], "rolled_back", "{out}");
    let failed = out["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["index"] == 1)
        .expect("failed step present");
    let error = failed["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("not found"),
        "等待路径的错误必须标记该步 failed: {error}"
    );
    assert!(
        out["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["status"] == "rolled_back"),
        "已完成步骤必须回滚: {out}"
    );
    // good + undo 在 worker 上真正执行；失败步骤未进入驱动。
    assert_eq!(executions.load(Ordering::SeqCst), 2);

    rig.registry
        .try_acquire_default(thing_id, "after-run".into())
        .expect("failure/rollback must still release the run-wide lease");
    let _ = rig.shutdown_tx.send(());
}

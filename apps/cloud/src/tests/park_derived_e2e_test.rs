//! 园区派生全链路 E2E（spec §七/§八；R2 防线：真实 DB + 真实 EventBus +
//! 真实 sink 管线，禁 mock 管线）。
//!
//! 装配与 derived_properties_api_test 同构（真实 pool + 真实路由 + 真实
//! Manager），差异：本测试跑真实 `DataServer::run` sink 消费任务（生产同款
//! 循环），派生读数经 value_sink → 共享管线回流缓存并发布事件。
//!
//! 取舍记录（均为"数据源入口"层，管线每一级都是生产代码）：
//! - 表计读数入口：经 `DataServer::consume_driver_reading`（run() 消费任务
//!   调用的同一 pub 函数）直接注入——简报明示"模拟器行为已在 Task 3 证明，
//!   本测试经 ingest 注入即可，不装模拟驱动"。driver_cache 里为表计登记的
//!   ManualPushDriver（mode=Push、永不产生读数）只是 consume 按 thing_id
//!   查 wrapper 的载体；下游 apply_property_values → cache_and_publish →
//!   EventBus → 派生重算 → sink → run() 真实消费 → 缓存 + 事件全真。
//! - 事件落库：生产 PersistenceEventHandler 按策略不落 Info 级 PropertyChange
//!   （防噪，见 persistence_handler.rs should_persist），故本测试在真实
//!   EventBus 上注册 RecordingEvents——实现生产 EventHandler trait、走相同
//!   的 bus 分发 + 真实 `Db::insert_event`——证明"事件真实发布且真实落库"。
//! - 园区物缓存种子：派生引擎从缓存捕获 owner 物
//!   （derived_manager.rs ensure_owner），园区物必须带派生属性进缓存。
//!   生产的缓存写入保证此前只有 bootstrap load_device_cache（重启路径）——
//!   REST 建物/场景实例化不写缓存（DeviceCreated/Updated 事件仅 legacy
//!   driver service 发布），评审 Important-1 已为 ensure_owner 补库载自愈；
//!   本文件 test 1-3 仍直接播种（真实 id）覆盖缓存命中路径，test 4 不播种
//!   园区物、不重启，覆盖 cache-miss 自愈活路径（notify_config_changed 活
//!   路径同验）；notify_tree_changed 活路径由 test 2 活验。

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use tinyiothub_core::driver::{DriverMode, DriverReading, ResultValue, ThingDriver};
use tinyiothub_core::models::event::Event;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_runtime::data_server::DataServer;
use tinyiothub_runtime::driver::DriverWrapper;
use tinyiothub_runtime::driver::derived_manager::DerivedDriverManager;
use tinyiothub_runtime::ports::ThingCacheSource;

use crate::shared::runtime_ports::{DbDerivedPropertyQueries, ThingCacheAdapter};
use crate::state::AppState;
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_json, response_parts, seed_test_workspace,
    setup_test_app_with_pool,
};

const WS: &str = "ws-default-001";

// ── 测试数据源（非管线替身，见文件头）─────────────────────────────────

/// mode=Push 的空读驱动：DataServer 轮询循环按 mode 跳过，自身永不产生
/// 读数；存在仅为让 consume_driver_reading 按 thing_id 找到 wrapper。
struct ManualPushDriver {
    thing: Thing,
}

impl ThingDriver for ManualPushDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }
    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }
    fn mode(&self) -> DriverMode {
        DriverMode::Push
    }
    fn read_data(&mut self) -> Result<Vec<ResultValue>, tinyiothub_core::error::Error> {
        Ok(vec![]) // Push 驱动不经轮询拉取（防御性空读，正常永不调用）
    }
    fn execute_command(&mut self, _cmd: &ThingCommand) -> Result<bool, tinyiothub_core::error::Error> {
        Err(tinyiothub_core::error::Error::DriverError(
            "手动注入驱动不支持动作".into(),
        ))
    }
}

/// 全量事件记录器：真实 bus 分发 → 真实 `Db::insert_event`（生产
/// PersistenceEventHandler 按策略不落 Info 级属性变更，见文件头）。
struct RecordingEvents {
    db: Arc<tinyiothub_storage::Db>,
}

#[async_trait::async_trait]
impl tinyiothub_core::event::EventHandler for RecordingEvents {
    async fn handle(&self, event: &Event) -> tinyiothub_core::error::Result<()> {
        self.db
            .insert_event(event)
            .await
            .map_err(|e| tinyiothub_core::error::Error::Internal(e.to_string()))
    }
    fn name(&self) -> &str {
        "RecordingEvents"
    }
    fn should_handle(&self, _event: &Event) -> bool {
        true
    }
}

// ── 装配 ─────────────────────────────────────────────────────────────

struct Rig {
    pool: sqlx::SqlitePool,
    app_state: AppState,
    data_server: DataServer,
    cache_src: Arc<dyn ThingCacheSource>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

/// 真实 DB + 真实 EventBus + 真实 DataServer（run() 已启动 sink 消费与
/// 轮询循环；ManualPushDriver mode=Push，轮询空转——派生引擎不注册任何
/// wrapper，不占轮询迭代）。
async fn setup_rig() -> Rig {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", WS).await;
    let cache_src: Arc<dyn ThingCacheSource> = Arc::new(ThingCacheAdapter(app_state.device_cache.clone()));
    let data_server = DataServer::new(cache_src.clone(), app_state.event_bus.clone());
    app_state.event_bus.register_handler(Arc::new(RecordingEvents {
        db: app_state.db.clone(),
    }));
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    data_server.run(shutdown_rx).await.expect("data_server run");
    Rig {
        pool,
        app_state,
        data_server,
        cache_src,
        shutdown_tx,
    }
}

fn build_router(app_state: &AppState) -> Router {
    let api_router = crate::api::create_router(app_state);
    Router::new().nest("/api", api_router).with_state(app_state.clone())
}

/// 冷启动装配 Manager：扫库重建索引 + 建 owner defs 句柄 + 首值补偿 +
/// 单订阅引擎（start 返回即事件订阅已建立，E5）。
async fn install_manager(rig: &mut Rig) -> Arc<DerivedDriverManager> {
    let manager = Arc::new(DerivedDriverManager::new(
        Arc::new(DbDerivedPropertyQueries((*rig.app_state.db).clone())),
        rig.cache_src.clone(),
        rig.data_server.value_sink(),
        rig.app_state.event_bus.clone(),
    ));
    manager.start().await.expect("derived manager start");
    rig.app_state.derived_manager = Some(manager.clone());
    manager
}

/// 物进缓存 + 登记 ingest 载体 wrapper（真实属性 id）。
fn register_manual_thing(rig: &Rig, thing: Thing) {
    rig.app_state.device_cache.insert(thing.clone());
    rig.data_server.driver_cache().insert(
        thing.id.clone(),
        Arc::new(parking_lot::RwLock::new(DriverWrapper::new(Box::new(
            ManualPushDriver { thing },
        )))),
    );
}

/// 表计上报入口：与 DataServer::run() 消费任务同一函数（真实共享管线；
/// consume 返回即缓存已更新、事件已发布进 bus）。
async fn ingest(rig: &Rig, thing_id: &str, prop_name: &str, value: &str) {
    DataServer::consume_driver_reading(
        DriverReading {
            thing_id: thing_id.to_string(),
            values: vec![ResultValue::new(
                prop_name.to_string(),
                "float".to_string(),
                Some(value.to_string()),
            )],
        },
        &rig.data_server.driver_cache(),
        &rig.cache_src,
        rig.app_state.event_bus.clone(),
    )
    .await;
}

// ── HTTP 辅助（同 derived_properties_api_test 先例）───────────────────

fn token() -> String {
    create_test_token_with_workspace("user-1", "tenant-1", WS)
}

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    let body_str = match body {
        Some(v) => v.to_string(),
        None => String::new(),
    };
    builder.body(Body::from(body_str)).unwrap()
}

async fn create_thing(app: &Router, token: &str, name: &str, parent_id: Option<&str>) -> String {
    let body = match parent_id {
        Some(p) => json!({ "name": name, "parentId": p }),
        None => json!({ "name": name }),
    };
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "创建物 {name} 失败: {json}");
    json["result"]["id"].as_str().unwrap().to_string()
}

async fn post_property(app: &Router, token: &str, thing_id: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/things/admin/{thing_id}/properties"),
            token,
            Some(body),
        ))
        .await
        .unwrap();
    response_parts(response).await
}

async fn create_property(app: &Router, token: &str, thing_id: &str, body: Value) -> String {
    let (status, json) = post_property(app, token, thing_id, body).await;
    assert_eq!(status, StatusCode::OK, "创建属性失败: {json}");
    json["result"]["id"].as_str().unwrap().to_string()
}

async fn get_properties(app: &Router, token: &str, thing_id: &str) -> Value {
    let response = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/admin/{thing_id}/properties"),
            token,
            None,
        ))
        .await
        .unwrap();
    let json = response_json(response).await;
    assert_eq!(json["code"], 0, "GET properties 失败: {json}");
    json["result"].clone()
}

// ── 夹具与确定性等待 ─────────────────────────────────────────────────

fn mk_prop(id: &str, thing_id: &str, name: &str) -> ThingProperty {
    ThingProperty {
        id: id.to_string(),
        thing_id: thing_id.to_string(),
        name: name.to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    }
}

fn mk_thing(id: &str, name: &str, props: Vec<ThingProperty>) -> Thing {
    let mut t = Thing {
        id: id.to_string(),
        name: name.to_string(),
        ..Thing::default()
    };
    t.properties = Some(props);
    t
}

fn cached_prop(rig: &Rig, thing_id: &str, prop_name: &str) -> Option<String> {
    rig.app_state
        .device_cache
        .get(thing_id)
        .and_then(|t| t.properties)
        .and_then(|props| props.into_iter().find(|p| p.name == prop_name))
        .and_then(|p| p.current_value)
}

/// 确定性轮询（禁裸 sleep 碰运气；轮询间隔 25ms， deadline 5s 为 CI 余量，
/// 正常路径 1s 内收敛）。
async fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if f() {
            return;
        }
        assert!(Instant::now() < deadline, "超时等待: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// 确定性等待园区物 PropertyChange 落库（publish_event_safe 为 spawn 分发，
/// 轮询 events 表真实行：event_subtype=property_change 且 metadata 携带期望新值）。
async fn wait_park_property_change(pool: &sqlx::SqlitePool, park_id: &str, want_value: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT event_subtype, content FROM events WHERE thing_id = ? AND title LIKE 'Property Changed%'",
        )
        .bind(park_id)
        .fetch_all(pool)
        .await
        .expect("查询 events 表");
        if rows.iter().any(|(subtype, content)| {
            subtype.contains("PropertyChange") && content.contains(&format!("\"value\":\"{want_value}\""))
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "超时等待园区物 PropertyChange(value={want_value}) 落库"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

// ── 用例 ─────────────────────────────────────────────────────────────

/// spec §八 step3/4：表计上报 → 园区 total_power 重算 → 缓存更新 + 事件
/// 落库；引用值置空 → 转 stale「等待数据」且保持旧值。
#[tokio::test]
async fn meter_report_triggers_park_total_recompute() {
    let mut rig = setup_rig().await;
    let app = build_router(&rig.app_state);
    let token = token();

    // 1) 真实 API：园区物 + 电表1/2 + 用电数属性 + 派生 total_power（真实校验）
    let park = create_thing(&app, &token, "园区", None).await;
    let meter1 = create_thing(&app, &token, "电表1", None).await;
    let meter2 = create_thing(&app, &token, "电表2", None).await;
    let m1_prop = create_property(&app, &token, &meter1, json!({"name": "用电数", "data_type": "float"})).await;
    let m2_prop = create_property(&app, &token, &meter2, json!({"name": "用电数", "data_type": "float"})).await;
    let total_prop = create_property(
        &app,
        &token,
        &park,
        json!({"name": "total_power", "expression": "{电表1.用电数}+{电表2.用电数}"}),
    )
    .await;

    // 2) 缓存种子（真实 id，理据见文件头）+ 表计 ingest 载体
    register_manual_thing(
        &rig,
        mk_thing(&meter1, "电表1", vec![mk_prop(&m1_prop, &meter1, "用电数")]),
    );
    register_manual_thing(
        &rig,
        mk_thing(&meter2, "电表2", vec![mk_prop(&m2_prop, &meter2, "用电数")]),
    );
    rig.app_state.device_cache.insert(mk_thing(
        &park,
        "园区",
        vec![mk_prop(&total_prop, &park, "total_power")],
    ));

    // 3) 装配 Manager（冷启动重建索引；start 返回即订阅已建立）
    let manager = install_manager(&mut rig).await;
    let app = build_router(&rig.app_state);

    // 4) 经共享管线注入电表读数 → 确定性等待园区 total_power 更新（10+20=30）
    ingest(&rig, &meter1, "用电数", "10").await;
    ingest(&rig, &meter2, "用电数", "20").await;
    wait_until("园区 total_power=30", || {
        cached_prop(&rig, &park, "total_power").as_deref() == Some("30")
    })
    .await;
    let s = manager.derived_status(&total_prop).expect("派生状态");
    assert!(!s.stale, "求值成功后不应 stale: {s:?}");

    // 5) 事件真实落库：园区物 PropertyChange（metadata.value=30）
    wait_park_property_change(&rig.pool, &park, "30").await;

    // 6) 电表2 值置空 → 下次事件驱动重算转 stale（spec §八 step4；求值规则：
    //    任一引用无值 → 不更新保持旧值 + stale「等待数据」）
    let mut m2 = rig.app_state.device_cache.get(&meter2).expect("电表2 cached");
    m2.properties.as_mut().unwrap()[0].current_value = None;
    rig.app_state.device_cache.update(m2);
    ingest(&rig, &meter1, "用电数", "11").await;
    wait_until("total_power 转 stale", || {
        manager.derived_status(&total_prop).map(|s| s.stale) == Some(true)
    })
    .await;
    let reason = manager
        .derived_status(&total_prop)
        .and_then(|s| s.stale_reason)
        .unwrap_or_default();
    assert!(reason.contains("等待数据"), "{reason}");
    // I-2 后求值全程走 thing_id，stale 原因点名 id 级引用（spec §二 refs 以 id 为准）
    assert!(reason.contains(&meter2), "stale 原因应点名失效引用物: {reason}");
    assert!(reason.contains("用电数"), "{reason}");
    assert_eq!(
        cached_prop(&rig, &park, "total_power").as_deref(),
        Some("30"),
        "引用无值不更新，保持旧值"
    );

    // GET 真实 API 面注入 stale_reason
    let props = get_properties(&app, &token, &park).await;
    let row = props
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == total_prop)
        .unwrap();
    assert!(row["derived_status"]["stale"] == true, "{row}");
    assert!(
        row["derived_status"]["stale_reason"]
            .as_str()
            .unwrap()
            .contains("等待数据"),
        "{row}"
    );

    let _ = rig.shutdown_tx.send(());
}

/// spec §八 step3：园区下增 1 物 → create_thing 处理器 notify_tree_changed
/// → device_count（descendant_count(device)，真实递归 CTE 查库）重算 +1。
#[tokio::test]
async fn device_count_recomputed_on_tree_change() {
    let mut rig = setup_rig().await;
    let app = build_router(&rig.app_state);
    let token = token();

    let park = create_thing(&app, &token, "园区", None).await;
    create_thing(&app, &token, "电表1", Some(&park)).await;
    create_thing(&app, &token, "电表2", Some(&park)).await;
    // 类型参数为裸标识符（expression.rs 语法；spec 文本的引号写法会被 400 拒绝）
    let dc_prop = create_property(
        &app,
        &token,
        &park,
        json!({"name": "device_count", "expression": "descendant_count(device)"}),
    )
    .await;

    rig.app_state
        .device_cache
        .insert(mk_thing(&park, "园区", vec![mk_prop(&dc_prop, &park, "device_count")]));
    install_manager(&mut rig).await;
    // 含 manager 的路由——create_thing 处理器的 notify_tree_changed 钩子生效
    let app = build_router(&rig.app_state);

    // 首值补偿：2 个 device 后代
    wait_until("首值 device_count=2", || {
        cached_prop(&rig, &park, "device_count").as_deref() == Some("2")
    })
    .await;

    // 园区下增 1 物（真实 API，处理器同步 notify_tree_changed）→ 重算 +1
    create_thing(&app, &token, "电表3", Some(&park)).await;
    wait_until("子树变更 device_count=3", || {
        cached_prop(&rig, &park, "device_count").as_deref() == Some("3")
    })
    .await;
    wait_park_property_change(&rig.pool, &park, "3").await;

    let _ = rig.shutdown_tx.send(());
}

/// spec §三/§七：重启（全新 Manager 冷启动 start()）→ 索引从库重建 +
/// 首值补偿——不经任何事件 total_power 即有值。
#[tokio::test]
async fn restart_rebuilds_index_and_first_value() {
    let mut rig = setup_rig().await;
    let app = build_router(&rig.app_state);
    let token = token();

    let park = create_thing(&app, &token, "园区", None).await;
    let meter1 = create_thing(&app, &token, "电表1", None).await;
    let meter2 = create_thing(&app, &token, "电表2", None).await;
    let m1_prop = create_property(&app, &token, &meter1, json!({"name": "用电数", "data_type": "float"})).await;
    let m2_prop = create_property(&app, &token, &meter2, json!({"name": "用电数", "data_type": "float"})).await;
    let total_prop = create_property(
        &app,
        &token,
        &park,
        json!({"name": "total_power", "expression": "{电表1.用电数}+{电表2.用电数}"}),
    )
    .await;

    register_manual_thing(
        &rig,
        mk_thing(&meter1, "电表1", vec![mk_prop(&m1_prop, &meter1, "用电数")]),
    );
    register_manual_thing(
        &rig,
        mk_thing(&meter2, "电表2", vec![mk_prop(&m2_prop, &meter2, "用电数")]),
    );
    rig.app_state.device_cache.insert(mk_thing(
        &park,
        "园区",
        vec![mk_prop(&total_prop, &park, "total_power")],
    ));

    // 表计读数经真实 ingest 进缓存（模拟"重启前已上报"的存量状态）
    ingest(&rig, &meter1, "用电数", "100").await;
    ingest(&rig, &meter2, "用电数", "200").await;
    assert_eq!(cached_prop(&rig, &meter1, "用电数").as_deref(), Some("100"));
    assert_eq!(cached_prop(&rig, &meter2, "用电数").as_deref(), Some("200"));

    // 模拟重启：全新 Manager（空索引/空状态表/空 owners）冷启动 start()
    let manager = install_manager(&mut rig).await;

    // 不经任何事件，total_power 经首值补偿出值（100+200=300）
    wait_until("重启首值 total_power=300", || {
        cached_prop(&rig, &park, "total_power").as_deref() == Some("300")
    })
    .await;
    let s = manager.derived_status(&total_prop).expect("派生状态");
    assert!(!s.stale && s.last_eval_at.is_some(), "首值补偿应已真实求值: {s:?}");
    wait_park_property_change(&rig.pool, &park, "300").await;

    let _ = rig.shutdown_tx.send(());
}

/// Task 13 评审 Important-1 热路径自愈：经真实 REST API 建园区物（不经
/// bootstrap 播种、不进缓存——与场景包实例化同构的热路径缺口）→ POST
/// 派生属性（notify_config_changed 活路径）→ 不重启，注入电表读数 →
/// total_power 重算出值。
#[tokio::test]
async fn hot_path_rest_created_owner_self_heals_without_restart() {
    let mut rig = setup_rig().await;
    // Manager 先装配——POST 属性走 notify_config_changed 活路径
    let manager = install_manager(&mut rig).await;
    let app = build_router(&rig.app_state);
    let token = token();

    // 园区物只经真实 REST 落库，全程不手动播种缓存（自愈断言点）
    let park = create_thing(&app, &token, "园区", None).await;
    let meter1 = create_thing(&app, &token, "电表1", None).await;
    let meter2 = create_thing(&app, &token, "电表2", None).await;
    let m1_prop = create_property(&app, &token, &meter1, json!({"name": "用电数", "data_type": "float"})).await;
    let m2_prop = create_property(&app, &token, &meter2, json!({"name": "用电数", "data_type": "float"})).await;
    // notify_config_changed → ensure_owner cache miss → 从库补载园区物
    // （含刚创建的 total_power 属性）→ 建 defs 句柄 + 首值补偿
    let total_prop = create_property(
        &app,
        &token,
        &park,
        json!({"name": "total_power", "expression": "{电表1.用电数}+{电表2.用电数}"}),
    )
    .await;

    // 电表是数据源：ingest 载体登记（园区物仍不进缓存）
    register_manual_thing(
        &rig,
        mk_thing(&meter1, "电表1", vec![mk_prop(&m1_prop, &meter1, "用电数")]),
    );
    register_manual_thing(
        &rig,
        mk_thing(&meter2, "电表2", vec![mk_prop(&m2_prop, &meter2, "用电数")]),
    );

    // 自愈证据：园区物已补载进缓存且带派生属性（非测试播种）
    let cached_park = rig.app_state.device_cache.get(&park).expect("自愈后园区物应已进缓存");
    assert!(
        cached_park
            .properties
            .as_ref()
            .is_some_and(|ps| ps.iter().any(|p| p.name == "total_power")),
        "库载补种必须含派生属性: {cached_park:?}"
    );

    // 不重启：注入电表读数 → 重算出值 + 事件落库
    ingest(&rig, &meter1, "用电数", "10").await;
    ingest(&rig, &meter2, "用电数", "20").await;
    wait_until("热路径 total_power=30", || {
        cached_prop(&rig, &park, "total_power").as_deref() == Some("30")
    })
    .await;
    let s = manager.derived_status(&total_prop).expect("派生状态");
    assert!(!s.stale, "{s:?}");
    wait_park_property_change(&rig.pool, &park, "30").await;

    let _ = rig.shutdown_tx.send(());
}

/// 终审 I-2：引用物改名后重算仍按 thing_id 出值（spec §二 "refs 以 thing_id
/// 为准（改名不影响计算）"）。旧实现按表达式原文物名解析——重启/缓存刷新后
/// name_to_id 只认新名，派生永久 stale「等待数据」。
#[tokio::test]
async fn rename_ref_thing_recompute_still_resolves_by_id() {
    let mut rig = setup_rig().await;
    let app = build_router(&rig.app_state);
    let token = token();

    let park = create_thing(&app, &token, "园区", None).await;
    let meter1 = create_thing(&app, &token, "电表1", None).await;
    let m1_prop = create_property(&app, &token, &meter1, json!({"name": "用电数", "data_type": "float"})).await;
    let total_prop = create_property(
        &app,
        &token,
        &park,
        json!({"name": "total_power", "expression": "{电表1.用电数} * 2"}),
    )
    .await;

    register_manual_thing(
        &rig,
        mk_thing(&meter1, "电表1", vec![mk_prop(&m1_prop, &meter1, "用电数")]),
    );
    rig.app_state.device_cache.insert(mk_thing(
        &park,
        "园区",
        vec![mk_prop(&total_prop, &park, "total_power")],
    ));
    let manager = install_manager(&mut rig).await;
    let app = build_router(&rig.app_state);

    ingest(&rig, &meter1, "用电数", "21").await;
    wait_until("改名前 total_power=42", || {
        cached_prop(&rig, &park, "total_power").as_deref() == Some("42")
    })
    .await;

    // 真实 API 改名 + 缓存以新名重建（模拟重启后 load_device_cache 全量重插：
    // name_to_id 只有新名——ThingCache::update 本就不刷新 name_to_id，
    // remove+insert 等价于重启后的缓存状态）
    let response = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{meter1}"),
            &token,
            Some(json!({"name": "电表1改"})),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "改名失败: {json}");
    let mut renamed = rig.app_state.device_cache.get(&meter1).expect("电表 cached");
    renamed.name = "电表1改".to_string();
    rig.app_state.device_cache.remove(&meter1);
    rig.app_state.device_cache.insert(renamed);
    assert!(
        rig.app_state.device_cache.get_by_name("电表1").is_none(),
        "旧名不应再可解析（重启后缓存状态）"
    );

    // 改名后新读数 → 事件按 thing_id 命中、求值按 thing_id 取数 → 仍出值
    ingest(&rig, &meter1, "用电数", "22").await;
    wait_until("改名后 total_power=44", || {
        cached_prop(&rig, &park, "total_power").as_deref() == Some("44")
    })
    .await;
    let s = manager.derived_status(&total_prop).expect("派生状态");
    assert!(!s.stale, "改名后重算不应 stale: {s:?}");

    let _ = rig.shutdown_tx.send(());
}

// ── 终审 I-3：场景包实例化挂到既有园区下 = 子树变更，处理器接 ─────────────
// notify_tree_changed → device_count 重算 +N（漏接则静默陈旧到重启）。

/// 迷你场景包：根（campus/space）→ 3 个 template_ref 传感器（thing_type=device）。
const MINI_SCENE_JSON: &str = r#"{
    "name": "mini_scene",
    "display_name": {"zh": "迷你场景"},
    "category": "scenes",
    "thing_category": "campus",
    "parameters": [],
    "device_info": {"default_name_pattern": "{scene_name}"},
    "children": [
        {"key": "sensor", "template_ref": "mini_sensor", "count": 3,
         "device_info": {"default_name_pattern": "传感器{index}"}}
    ]
}"#;

async fn seed_template_category(pool: &sqlx::SqlitePool, name: &str) {
    sqlx::query("INSERT OR IGNORE INTO template_categories (name, display_name, created_at) VALUES (?, '{\"zh\":\"分类\"}', '2026-01-01 00:00:00')")
        .bind(name)
        .execute(pool)
        .await
        .expect("seed template category");
}

/// 向 thing_templates 插入一行模板（与 scene_instantiate_test 同夹具），返回模板 id。
async fn seed_thing_template(
    pool: &sqlx::SqlitePool,
    name: &str,
    category: &str,
    device_info: &str,
    properties: &str,
    actions: &str,
) -> String {
    seed_template_category(pool, category).await;
    let id = format!("tpl-{name}");
    sqlx::query(
        "INSERT INTO thing_templates (id, name, display_name, version, category, \
         tags, device_info, properties, actions, events, is_builtin, is_active, workspace_id, created_at, updated_at) \
         VALUES (?, ?, '{\"zh\":\"模板\"}', '1.0.0', ?, '[]', ?, ?, ?, '[]', 0, 1, ?, \
         '2026-01-01 00:00:00', '2026-01-01 00:00:00')",
    )
    .bind(&id)
    .bind(name)
    .bind(category)
    .bind(device_info)
    .bind(properties)
    .bind(actions)
    .bind(WS)
    .execute(pool)
    .await
    .expect("seed thing template");
    id
}

#[tokio::test]
async fn scene_instantiation_bumps_device_count() {
    let mut rig = setup_rig().await;
    // Manager 先装配——POST 属性走 notify_config_changed 活路径、实例化走
    // notify_tree_changed 活路径（均真实路由钩子）
    install_manager(&mut rig).await;
    let app = build_router(&rig.app_state);
    let token = token();

    let park = create_thing(&app, &token, "园区", None).await;
    create_property(
        &app,
        &token,
        &park,
        json!({"name": "device_count", "expression": "descendant_count(device)"}),
    )
    .await;

    // 首值补偿（ensure_owner 库载自愈）：园区下 0 个 device 后代
    wait_until("首值 device_count=0", || {
        cached_prop(&rig, &park, "device_count").as_deref() == Some("0")
    })
    .await;

    // 种子：传感器设备模板 + 场景包模板（真实表行，与 scene_instantiate_test 同法）
    seed_thing_template(&rig.pool, "mini_sensor", "sensor", "{}", "[]", "[]").await;
    let scene_tpl = seed_thing_template(&rig.pool, "mini_scene", "scenes", MINI_SCENE_JSON, "[]", "[]").await;

    // 真实 API 实例化（挂到既有园区下）→ 处理器 notify_tree_changed → 重算
    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/marketplace/thing-templates/{scene_tpl}/instantiate"),
            &token,
            Some(json!({
                "sceneName": "南区",
                "parentId": park,
                "parameterValues": {},
                "dryRun": false,
            })),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::OK, "实例化失败: {json}");
    assert_eq!(json["result"]["nodeCount"], 4, "根 + 3 传感器: {json}");

    // 实例化的 3 个 device 后代被 descendant_count(device) 真实计入
    wait_until("实例化后 device_count=3", || {
        cached_prop(&rig, &park, "device_count").as_deref() == Some("3")
    })
    .await;
    wait_park_property_change(&rig.pool, &park, "3").await;

    let _ = rig.shutdown_tx.send(());
}

/// X4 收尾（Task 3）混合模式：物理驱动物（driver_name='modbus'）上派生计算值
/// 与驱动采集值共存同一 Thing，双向互不覆盖——
///   派生写回（sink → consume wrapper 路径）不覆盖采集值；
///   驱动采集（consume 同管线）不覆盖派生值。
/// 形态适配记录：derived_validation 的 DAG 环检测为物级（owner→引用物一条边），
/// owner 引用本物属性构成自环 X→X 被拒（"引用成环"），故派生引用另一物属性——
/// 共存语义不变（简报已授权此适配）。
#[tokio::test]
async fn physical_thing_derived_property_coexists_with_driver_values() {
    let mut rig = setup_rig().await;
    let app = build_router(&rig.app_state);
    let token = token();

    // 1) 物理物（driver_name='modbus'，真实 API）+ 数据源电表
    let response = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/things",
            &token,
            Some(json!({ "name": "物理表", "driverName": "modbus" })),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(response).await;
    assert_eq!(status, StatusCode::CREATED, "创建物理驱动物失败: {json}");
    let physical = json["result"]["id"].as_str().unwrap().to_string();

    let meter = create_thing(&app, &token, "电表1", None).await;
    let m_prop = create_property(&app, &token, &meter, json!({"name": "用电数", "data_type": "float"})).await;
    // 物理物自身的驱动采集属性（非派生，驱动写值）
    let v_prop = create_property(&app, &token, &physical, json!({"name": "voltage", "data_type": "float"})).await;
    // I-1 取消：物理物配派生 → 200（引用另一物属性——物级自环被拒，见函数头）
    let d_prop = create_property(
        &app,
        &token,
        &physical,
        json!({"name": "power_double", "expression": "{电表1.用电数} * 2"}),
    )
    .await;

    // 2) 缓存种子 + ingest 载体（物理表带采集属性与派生属性，真实 id）
    register_manual_thing(
        &rig,
        mk_thing(
            &physical,
            "物理表",
            vec![
                mk_prop(&v_prop, &physical, "voltage"),
                mk_prop(&d_prop, &physical, "power_double"),
            ],
        ),
    );
    register_manual_thing(
        &rig,
        mk_thing(&meter, "电表1", vec![mk_prop(&m_prop, &meter, "用电数")]),
    );
    install_manager(&mut rig).await;

    // 3) 驱动采集注入（物理 wrapper consume 路径）→ voltage 落缓存
    ingest(&rig, &physical, "voltage", "220").await;
    assert_eq!(cached_prop(&rig, &physical, "voltage").as_deref(), Some("220"));

    // 4) 引用物上报 → 派生重算写回同一物理物（110*2=220），采集值不被覆盖
    ingest(&rig, &meter, "用电数", "110").await;
    wait_until("power_double=220", || {
        cached_prop(&rig, &physical, "power_double").as_deref() == Some("220")
    })
    .await;
    assert_eq!(
        cached_prop(&rig, &physical, "voltage").as_deref(),
        Some("220"),
        "派生写回不得覆盖驱动采集值"
    );

    // 5) 驱动再采集 → voltage 更新，派生值不被覆盖（无引用 voltage 的 def，
    //    采集事件不触发重算；即便重算也只写 defs 值）
    ingest(&rig, &physical, "voltage", "221").await;
    assert_eq!(cached_prop(&rig, &physical, "voltage").as_deref(), Some("221"));
    assert_eq!(
        cached_prop(&rig, &physical, "power_double").as_deref(),
        Some("220"),
        "驱动采集不得覆盖派生计算值"
    );

    let _ = rig.shutdown_tx.send(());
}

//! E4d Task 7 E2E：真实 Router + 真实 SQLite 迁移 + 真实 `ZoneService` /
//! `DataServer`，覆盖 spec §4 退出标准。
//!
//! 1. 地图 + 两个重叠 zone：PUT 坐标落区，多 zone 取面积最小。
//! 2. 显式 `placement.zoneRef` 优先于点选；未知引用 fail-closed。
//! 3. `zone_source` maps ↔ hierarchy 切换与回滚（LiveState 摘要 / E4b 快照面）。
//! 4. 跨工作区 map/place/position/zone 隔离。
//! 5. 驱动位置摄取门：runtime+fromSource 才写坐标；fixed 忽略 position_*；
//!    越界保留坐标但 zone=None。
//!
//! E1b hierarchy 回退、E3 World zone 字符串、E4b workspace 快照的 zone 字段
//! 均在 3 中一并回归（World 直接消费 `DeviceSnapshot.zone`）。

use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use tinyiothub_core::driver::{DriverReading, ResultValue};
use tinyiothub_core::models::spatial::LocationTrust;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_runtime::DataServer;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::maps::{TrustThresholds, ZoneService};
use tinyiothub_runtime::watch::live_state::LiveState;

use crate::shared::runtime_ports::{DbMapStore, ThingCacheAdapter};
use crate::state::AppState;
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_admin_role, seed_test_workspace,
    setup_test_app_with_pool,
};

const ADMIN: &str = "user-1";
const TENANT: &str = "tenant-1";
const WS_A: &str = "ws-e4d-a";
const WS_B: &str = "ws-e4d-b";

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .body(body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty))
        .unwrap()
}

fn admin_token(ws: &str) -> String {
    create_test_token_with_workspace(ADMIN, TENANT, ws)
}

async fn setup() -> (Router, sqlx::SqlitePool, AppState, Arc<ZoneService>) {
    let (mut state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, TENANT, WS_A).await;
    seed_test_workspace(&pool, TENANT, WS_B).await;
    seed_admin_role(&pool, ADMIN).await;
    let zones = Arc::new(ZoneService::with_ttl(
        Arc::new(DbMapStore((*state.db).clone())),
        TrustThresholds::default(),
        // 长 TTL：任何切换/CRUD 后仍能命中旧值都说明失效缺失。
        Duration::from_secs(60),
    ));
    state.set_zones(zones.clone());
    let router = Router::new()
        .nest("/api", crate::api::create_router(&state))
        .with_state(state.clone());
    (router, pool, state, zones)
}

fn map_body(name: &str, is_default: bool) -> Value {
    json!({
        "name": name,
        "bounds": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0},
        "isDefault": is_default,
    })
}

fn zone_body(name: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!({
        "name": name,
        "kind": "zone",
        "geometry": {"polygon": {"points": [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]}},
    })
}

async fn create_map(app: &Router, token: &str, body: Value) -> String {
    let r = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/maps", token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "map create failed: {json}");
    json["result"]["id"].as_str().expect("map id").to_string()
}

async fn create_zone(app: &Router, token: &str, map: &str, body: Value) -> String {
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            token,
            Some(body),
        ))
        .await
        .unwrap();
    let (status, json) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "place create failed: {json}");
    json["result"]["id"].as_str().expect("place id").to_string()
}

async fn create_thing(app: &Router, token: &str, ws: &str, name: &str) -> String {
    let body = json!({"name": name, "thingType": "device", "workspaceId": ws});
    let r = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/things", token, Some(body)))
        .await
        .unwrap();
    let (status, json) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "thing create failed: {json}");
    json["result"]["id"].as_str().expect("thing id").to_string()
}

async fn put_position(app: &Router, token: &str, thing: &str, body: Value) -> (StatusCode, Value) {
    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            token,
            Some(body),
        ))
        .await
        .unwrap();
    response_parts(r).await
}

async fn get_position(app: &Router, token: &str, thing: &str) -> (StatusCode, Value) {
    let r = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{thing}/position"),
            token,
            None,
        ))
        .await
        .unwrap();
    response_parts(r).await
}

async fn zone_for(zones: &ZoneService, ws: &str, thing: &str) -> Option<String> {
    zones
        .zone_for(ws, thing, None, chrono::Utc::now())
        .await
        .expect("zone_for")
}

async fn wait_for_position(
    db: &tinyiothub_storage::Db,
    ws: &str,
    thing: &str,
    want: impl Fn(&tinyiothub_core::models::spatial::ThingPosition) -> bool,
) -> tinyiothub_core::models::spatial::ThingPosition {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let current = db
            .get_thing_position(ws, thing)
            .await
            .expect("read position")
            .expect("thing position row");
        if want(&current) {
            return current;
        }
        assert!(std::time::Instant::now() < deadline, "超时等待位置落库: {current:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ── 1. 重叠 zone 取面积最小 ────────────────────────────

#[tokio::test]
async fn overlapping_zones_resolve_to_smallest_area() {
    let (app, _pool, state, zones) = setup().await;
    let token = admin_token(WS_A);
    state.db.set_zone_source(WS_A, "maps").await.expect("maps authority");
    let map = create_map(&app, &token, map_body("floor-1", true)).await;

    // 大 zone 2500 m²，小 zone 100 m²；(5,5) 同时落在两者内。
    create_zone(&app, &token, &map, zone_body("warehouse", 0.0, 0.0, 50.0, 50.0)).await;
    create_zone(&app, &token, &map, zone_body("rack-a", 0.0, 0.0, 10.0, 10.0)).await;
    let thing = create_thing(&app, &token, WS_A, "forklift-1").await;

    let (status, body) = put_position(&app, &token, &thing, json!({"x": 5.0, "y": 5.0})).await;
    assert_eq!(status, StatusCode::OK, "position put failed: {body}");
    let (status, body) = get_position(&app, &token, &thing).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (body["result"]["x"].as_f64(), body["result"]["y"].as_f64()),
        (Some(5.0), Some(5.0))
    );
    assert_eq!(zone_for(&zones, WS_A, &thing).await.as_deref(), Some("rack-a"));

    // 只在大 zone 内 → warehouse。
    let (status, _) = put_position(&app, &token, &thing, json!({"x": 30.0, "y": 30.0})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(zone_for(&zones, WS_A, &thing).await.as_deref(), Some("warehouse"));

    // 图内但所有 zone 外 → None。
    let (status, _) = put_position(&app, &token, &thing, json!({"x": 90.0, "y": 90.0})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(zone_for(&zones, WS_A, &thing).await, None);
}

// ── 2. 显式 zoneRef 优先 / 未知 fail-closed ────────────

#[tokio::test]
async fn explicit_zone_ref_wins_and_unknown_ref_fails_closed() {
    let (app, _pool, state, zones) = setup().await;
    let token = admin_token(WS_A);
    state.db.set_zone_source(WS_A, "maps").await.expect("maps authority");
    let map = create_map(&app, &token, map_body("floor-1", true)).await;
    create_zone(&app, &token, &map, zone_body("zone-a", 0.0, 0.0, 10.0, 10.0)).await;
    create_zone(&app, &token, &map, zone_body("zone-b", 50.0, 50.0, 60.0, 60.0)).await;
    let thing = create_thing(&app, &token, WS_A, "robot-1").await;
    put_position(&app, &token, &thing, json!({"x": 5.0, "y": 5.0})).await;

    // 点选 = zone-a。
    assert_eq!(zone_for(&zones, WS_A, &thing).await.as_deref(), Some("zone-a"));

    // 显式引用（placement 物化列）指向 zone-b：优先于点选。
    zones
        .set_zone_ref(WS_A, &thing, Some("zone-b"))
        .await
        .expect("zone ref");
    assert_eq!(zone_for(&zones, WS_A, &thing).await.as_deref(), Some("zone-b"));

    // 未知引用 fail-closed：不得回退到点选命中的 zone-a。
    zones
        .set_zone_ref(WS_A, &thing, Some("does-not-exist"))
        .await
        .expect("zone ref");
    assert_eq!(zone_for(&zones, WS_A, &thing).await, None);

    // 清空后恢复点选。
    zones.set_zone_ref(WS_A, &thing, None).await.expect("clear zone ref");
    assert_eq!(zone_for(&zones, WS_A, &thing).await.as_deref(), Some("zone-a"));
}

// ── 3. maps ↔ hierarchy 切换 / 回滚 + E1b/E3/E4b 回归 ──

#[tokio::test]
async fn zone_source_switch_and_rollback_keeps_hierarchy_and_snapshot_strings() {
    let (app, _pool, state, zones) = setup().await;
    let token = admin_token(WS_A);
    let map = create_map(&app, &token, map_body("floor-1", true)).await;
    create_zone(&app, &token, &map, zone_body("map-zone", 0.0, 0.0, 100.0, 100.0)).await;
    let thing = create_thing(&app, &token, WS_A, "robot-1").await;
    put_position(&app, &token, &thing, json!({"x": 1.0, "y": 1.0})).await;

    // 层级真相在 cache（E1b）：Building A > robot-1。
    let parent = "hier-building-a";
    state.device_cache.insert(Thing {
        id: parent.into(),
        name: "Building A".into(),
        workspace_id: Some(WS_A.into()),
        ..Thing::default()
    });
    state.device_cache.insert(Thing {
        id: thing.clone(),
        name: "robot-1".into(),
        workspace_id: Some(WS_A.into()),
        parent_id: Some(parent.into()),
        ..Thing::default()
    });

    let mut live = LiveState::with_zone_service(Arc::new(ThingCacheAdapter(state.device_cache.clone())), zones.clone());
    live.handle_lagged();

    // 默认 hierarchy（迁移期回退）→ E1b 祖先链。
    assert_eq!(
        live.summary(&thing).await.expect("live summary").zone,
        "Building A",
        "default zone_source must stay hierarchy"
    );

    // maps → 多边形 zone；E4b 快照 / E3 World 读到同一字符串。
    state.db.set_zone_source(WS_A, "maps").await.expect("maps");
    zones.invalidate(WS_A);
    assert_eq!(live.summary(&thing).await.expect("live summary").zone, "map-zone");
    let snapshot = live.workspace_snapshot(WS_A).await;
    let device = snapshot
        .devices
        .iter()
        .find(|d| d.thing_id == thing)
        .expect("device in snapshot");
    assert_eq!(device.zone, "map-zone", "E3 World consumes snapshot zone string");

    // 回退 hierarchy → 祖先链恢复；快照同步。
    state.db.set_zone_source(WS_A, "hierarchy").await.expect("hierarchy");
    zones.invalidate(WS_A);
    assert_eq!(live.summary(&thing).await.expect("live summary").zone, "Building A");
    let snapshot = live.workspace_snapshot(WS_A).await;
    let device = snapshot.devices.iter().find(|d| d.thing_id == thing).expect("device");
    assert_eq!(device.zone, "Building A");

    // 切回 maps（回滚）→ 多边形 zone 再次生效。
    state.db.set_zone_source(WS_A, "maps").await.expect("maps again");
    zones.invalidate(WS_A);
    assert_eq!(live.summary(&thing).await.expect("live summary").zone, "map-zone");
}

// ── 4. 跨工作区隔离 ────────────────────────────────────

#[tokio::test]
async fn maps_places_positions_and_zones_are_workspace_isolated() {
    let (app, _pool, state, zones) = setup().await;
    let token_a = admin_token(WS_A);
    let token_b = admin_token(WS_B);
    state.db.set_zone_source(WS_A, "maps").await.expect("maps authority");
    // ws-b 同名图/zone：隔离不能靠名称碰巧不同。
    state.db.set_zone_source(WS_B, "maps").await.expect("maps authority");

    let map_a = create_map(&app, &token_a, map_body("floor-1", true)).await;
    let place_a = create_zone(&app, &token_a, &map_a, zone_body("zone-a", 0.0, 0.0, 10.0, 10.0)).await;
    let thing_a = create_thing(&app, &token_a, WS_A, "robot-a").await;
    put_position(&app, &token_a, &thing_a, json!({"x": 5.0, "y": 5.0})).await;
    zones
        .set_zone_ref(WS_A, &thing_a, Some("zone-a"))
        .await
        .expect("zone ref");

    let map_b = create_map(&app, &token_b, map_body("floor-1", true)).await;
    create_zone(&app, &token_b, &map_b, zone_body("zone-a", 50.0, 50.0, 60.0, 60.0)).await;
    let thing_b = create_thing(&app, &token_b, WS_B, "robot-b").await;

    // ws-b 看不到 ws-a 的图/地点/坐标（404）。
    for (method, uri, body) in [
        ("GET", format!("/api/v1/maps/{map_a}"), None),
        ("GET", format!("/api/v1/maps/{map_a}/places"), None),
        (
            "PATCH",
            format!("/api/v1/maps/{map_a}/places/{place_a}"),
            Some(json!({"name": "hijack"})),
        ),
        ("GET", format!("/api/v1/things/{thing_a}/position"), None),
        (
            "PUT",
            format!("/api/v1/things/{thing_a}/position"),
            Some(json!({"x": 1.0, "y": 1.0})),
        ),
    ] {
        let r = app
            .clone()
            .oneshot(auth_request(method, &uri, &token_b, body))
            .await
            .unwrap();
        assert_eq!(
            response_parts(r).await.0,
            StatusCode::NOT_FOUND,
            "{method} {uri} must be invisible across workspaces"
        );
    }

    // 同名 zone/坐标不串：各自解析到本工作区的位置。
    assert_eq!(zone_for(&zones, WS_A, &thing_a).await.as_deref(), Some("zone-a"));
    assert_eq!(
        zone_for(&zones, WS_B, &thing_b).await,
        None,
        "ws-b thing has no position"
    );
    assert_eq!(
        zone_for(&zones, WS_B, &thing_a).await,
        None,
        "ws-a thing must not resolve under ws-b scope"
    );
}

// ── 5. 驱动位置摄取门 + 越界 ───────────────────────────

fn position_reading(thing: &str, x: &str, y: &str, trust: Option<&str>) -> DriverReading {
    let mut values = vec![
        ResultValue::new("position_x".into(), "float".into(), Some(x.into())),
        ResultValue::new("position_y".into(), "float".into(), Some(y.into())),
    ];
    if let Some(trust) = trust {
        values.push(ResultValue::new(
            "position_trust".into(),
            "string".into(),
            Some(trust.into()),
        ));
    }
    DriverReading {
        thing_id: thing.into(),
        values,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn driver_position_ingest_gate_and_out_of_bounds() {
    let (app, _pool, state, zones) = setup().await;
    let token = admin_token(WS_A);
    state.db.set_zone_source(WS_A, "maps").await.expect("maps authority");
    let map = create_map(&app, &token, map_body("floor-1", true)).await;
    create_zone(&app, &token, &map, zone_body("zone-a", 0.0, 0.0, 10.0, 10.0)).await;
    let thing = create_thing(&app, &token, WS_A, "agv-1").await;

    // DataServer 的真实消费管线（value_sink → consume_driver_reading_with）。
    let cache: Arc<dyn tinyiothub_runtime::ports::ThingCacheSource> =
        Arc::new(ThingCacheAdapter(state.device_cache.clone()));
    state.device_cache.insert(Thing {
        id: thing.clone(),
        name: "agv-1".into(),
        workspace_id: Some(WS_A.into()),
        ..Thing::default()
    });
    let server = DataServer::new(cache, Arc::new(EventBus::new()));
    server.set_zone_service(zones.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    server.run(shutdown_rx).await.expect("data server run");
    let sink = server.value_sink();

    // 摄取门默认关闭（fixed/declared）：position_* 读数一律忽略。
    sink.send(position_reading(&thing, "1", "1", None))
        .await
        .expect("send reading");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        state.db.get_thing_position(WS_A, &thing).await.unwrap().unwrap().x,
        None,
        "gate off must ignore driver position"
    );

    // runtime + fromSource 开启摄取门 → 坐标/trust 落库。
    assert!(state.db.set_thing_position_ingest(WS_A, &thing, true).await.unwrap());
    zones.invalidate(WS_A);
    sink.send(position_reading(&thing, "2", "3", Some("uncertain")))
        .await
        .expect("send reading");
    let written = wait_for_position(&state.db, WS_A, &thing, |p| p.x == Some(2.0)).await;
    assert_eq!((written.x, written.y), (Some(2.0), Some(3.0)));
    assert_eq!(
        written.trust,
        LocationTrust::Uncertain,
        "driver trust caps at uncertain"
    );
    assert!(written.updated_at.is_some(), "position_updated_at=now");
    assert_eq!(zone_for(&zones, WS_A, &thing).await.as_deref(), Some("zone-a"));

    // 越界：保留坐标，zone=None（不拒绝读数）。
    sink.send(position_reading(&thing, "200", "200", None))
        .await
        .expect("send reading");
    wait_for_position(&state.db, WS_A, &thing, |p| p.x == Some(200.0)).await;
    assert_eq!(zone_for(&zones, WS_A, &thing).await, None, "out of bounds → zone None");

    // 关闭摄取门（fixed）：后续读数不得漂移静态位置。
    state.db.set_thing_position_ingest(WS_A, &thing, false).await.unwrap();
    zones.invalidate(WS_A);
    sink.send(position_reading(&thing, "1", "1", None))
        .await
        .expect("send reading");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let untouched = state.db.get_thing_position(WS_A, &thing).await.unwrap().unwrap();
    assert_eq!(
        (untouched.x, untouched.y),
        (Some(200.0), Some(200.0)),
        "fixed placement ignores position_* readings"
    );

    let _ = shutdown_tx.send(());
}

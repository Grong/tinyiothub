//! E4d Task 5 API tests: `/v1/maps`, `/v1/maps/{id}/places`,
//! `/v1/things/{id}/position` — CRUD、校验、admin 闸门、租户隔离与
//! ZoneService 缓存失效。

use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::shared::runtime_ports::DbMapStore;
use crate::state::AppState;
use crate::test_utils::{
    auth_header, create_test_token_with_workspace, response_parts, seed_admin_role, seed_test_workspace,
    setup_test_app_with_pool,
};
use tinyiothub_runtime::maps::{TrustThresholds, ZoneService};

const ADMIN: &str = "user-1";
const VIEWER: &str = "user-2";

fn auth_request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json");
    builder
        .body(body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty))
        .unwrap()
}

fn admin_token(ws: &str) -> String {
    create_test_token_with_workspace(ADMIN, "tenant-1", ws)
}

fn viewer_token(ws: &str) -> String {
    create_test_token_with_workspace(VIEWER, "tenant-1", ws)
}

async fn setup() -> (Router, sqlx::SqlitePool, AppState) {
    let (state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-a").await;
    seed_test_workspace(&pool, "tenant-1", "ws-b").await;
    seed_admin_role(&pool, ADMIN).await;
    sqlx::query("INSERT OR IGNORE INTO users (id, username, password_hash) VALUES (?, ?, 'x')")
        .bind(VIEWER)
        .bind("viewer")
        .execute(&pool)
        .await
        .expect("seed viewer");
    let router = Router::new()
        .nest("/api", crate::api::create_router(&state))
        .with_state(state.clone());
    (router, pool, state)
}

/// Same as [`setup`] but with a long-TTL ZoneService wired (cache invalidation
/// would otherwise be hidden by the 5s default).
async fn setup_with_zones() -> (Router, sqlx::SqlitePool, AppState, Arc<ZoneService>) {
    let (mut state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-a").await;
    seed_admin_role(&pool, ADMIN).await;
    let zones = Arc::new(ZoneService::with_ttl(
        Arc::new(DbMapStore((*state.db).clone())),
        TrustThresholds::default(),
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

fn zone_body(name: &str) -> Value {
    json!({
        "name": name,
        "kind": "zone",
        "geometry": {"polygon": {"points": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]}},
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

// ── Maps CRUD ─────────────────────────────────────────

#[tokio::test]
async fn map_crud_roundtrip_and_single_default() {
    let (app, _pool, _state) = setup().await;
    let token = admin_token("ws-a");

    let first = create_map(&app, &token, map_body("floor-1", false)).await;
    let second = create_map(&app, &token, map_body("floor-2", true)).await;

    let r = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/maps", &viewer_token("ws-a"), None))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK);
    let maps = body["result"].as_array().expect("map list");
    assert_eq!(maps.len(), 2);
    let defaults: Vec<_> = maps.iter().filter(|m| m["isDefault"] == json!(true)).collect();
    assert_eq!(defaults.len(), 1, "only one default map: {body}");
    assert_eq!(defaults[0]["id"], json!(second));

    let r = app
        .clone()
        .oneshot(auth_request("GET", &format!("/api/v1/maps/{first}"), &token, None))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["name"], json!("floor-1"));

    // PATCH name/bounds
    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{first}"),
            &token,
            Some(json!({"name": "floor-1-renamed", "bounds": {"x": -5.0, "y": 0.0, "w": 20.0, "h": 10.0}})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "patch failed: {body}");
    assert_eq!(body["result"]["name"], json!("floor-1-renamed"));
    assert_eq!(body["result"]["bounds"]["w"], json!(20.0));

    // PATCH 三态：省略锚图 = 不变；显式 null = 清空。
    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{first}"),
            &token,
            Some(json!({"anchorMapId": second})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "set anchor failed: {body}");
    assert_eq!(body["result"]["anchorMapId"], json!(second));

    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{first}"),
            &token,
            Some(json!({"name": "floor-1-renamed"})),
        ))
        .await
        .unwrap();
    let (_, body) = response_parts(r).await;
    assert_eq!(body["result"]["anchorMapId"], json!(second), "omitted keeps anchor");

    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{first}"),
            &token,
            Some(json!({"anchorMapId": null})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "clear anchor failed: {body}");
    assert_eq!(body["result"]["anchorMapId"], Value::Null);

    // 默认图不能删除，直到另一张图被提升
    let r = app
        .clone()
        .oneshot(auth_request("DELETE", &format!("/api/v1/maps/{second}"), &token, None))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::CONFLICT);

    let r = app
        .clone()
        .oneshot(auth_request("DELETE", &format!("/api/v1/maps/{first}"), &token, None))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::OK);
    let r = app
        .clone()
        .oneshot(auth_request("GET", &format!("/api/v1/maps/{first}"), &token, None))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn map_validation_rejects_invalid_input() {
    let (app, _pool, _state) = setup().await;
    let token = admin_token("ws-a");

    // 空名称
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/maps",
            &token,
            Some(json!({"name": "  ", "bounds": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // 非正尺寸
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/maps",
            &token,
            Some(json!({"name": "bad", "bounds": {"x": 0.0, "y": 0.0, "w": 0.0, "h": 10.0}})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // x+w 溢出为 inf → 拒绝（否则 within_bounds 恒真）
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/maps",
            &token,
            Some(json!({"name": "overflow", "bounds": {"x": 1e308, "y": 0.0, "w": 1e308, "h": 10.0}})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    let _ = create_map(&app, &token, map_body("floor-1", false)).await;

    // 重名
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/maps",
            &token,
            Some(map_body("floor-1", false)),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::CONFLICT);

    // 锚图不存在（同工作区校验）
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            "/api/v1/maps",
            &token,
            Some(json!({
                "name": "anchored",
                "bounds": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0},
                "anchorMapId": "missing",
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // 默认图不能原地降级
    let default = create_map(&app, &token, map_body("default", true)).await;
    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{default}"),
            &token,
            Some(json!({"isDefault": false})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::CONFLICT);
}

// ── Places CRUD ───────────────────────────────────────

#[tokio::test]
async fn place_crud_and_geometry_validation() {
    let (app, _pool, _state) = setup().await;
    let token = admin_token("ws-a");
    let map = create_map(&app, &token, map_body("floor-1", true)).await;

    // zone 少于 3 点
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(json!({
                "name": "bad-zone",
                "kind": "zone",
                "geometry": {"polygon": {"points": [[0.0, 0.0], [1.0, 1.0]]}},
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // landmark 必须是点
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(json!({
                "name": "bad-landmark",
                "kind": "landmark",
                "geometry": {"polygon": {"points": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]}},
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // 共线 3 点 = 零面积 → 拒绝（否则该 zone 永不命中）
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(json!({
                "name": "collinear",
                "kind": "zone",
                "geometry": {"polygon": {"points": [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]]}},
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // 相邻重复点 → 拒绝
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(json!({
                "name": "duplicate-point",
                "kind": "zone",
                "geometry": {"polygon": {"points": [[0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]}},
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(zone_body("zone-a")),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "place create failed: {body}");
    let place = body["result"]["id"].as_str().unwrap().to_string();

    // heading 三态：值 → 设置；null → 清空；省略 → 不变。
    for (patch, expected) in [
        (json!({"heading": 30.0}), json!(30.0)),
        (json!({"name": "zone-a"}), json!(30.0)),
        (json!({"heading": null}), Value::Null),
    ] {
        let r = app
            .clone()
            .oneshot(auth_request(
                "PATCH",
                &format!("/api/v1/maps/{map}/places/{place}"),
                &token,
                Some(patch),
            ))
            .await
            .unwrap();
        let (status, body) = response_parts(r).await;
        assert_eq!(status, StatusCode::OK, "heading patch failed: {body}");
        assert_eq!(body["result"]["heading"], expected);
    }

    // 同图重名
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(zone_body("zone-a")),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::CONFLICT);

    let r = app
        .clone()
        .oneshot(auth_request("GET", &format!("/api/v1/maps/{map}/places"), &token, None))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"].as_array().unwrap().len(), 1);

    // PATCH 几何/名称
    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{map}/places/{place}"),
            &token,
            Some(json!({
                "name": "zone-b",
                "geometry": {"polygon": {"points": [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]]}},
            })),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "place patch failed: {body}");
    assert_eq!(body["result"]["name"], json!("zone-b"));

    // 非法 PATCH 几何（合并后仍是 zone，2 点）
    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{map}/places/{place}"),
            &token,
            Some(json!({"geometry": {"polygon": {"points": [[0.0, 0.0], [1.0, 1.0]]}}})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    let r = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/maps/{map}/places/{place}"),
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::OK);

    let r = app
        .clone()
        .oneshot(auth_request(
            "PATCH",
            &format!("/api/v1/maps/{map}/places/{place}"),
            &token,
            Some(json!({"name": "gone"})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::NOT_FOUND);
}

// ── 授权 / 隔离 ───────────────────────────────────────

#[tokio::test]
async fn writes_require_admin_but_reads_are_workspace_scoped() {
    let (app, _pool, _state) = setup().await;
    let admin = admin_token("ws-a");
    let viewer = viewer_token("ws-a");
    let map = create_map(&app, &admin, map_body("floor-1", true)).await;
    let place_body = zone_body("zone-a");
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &admin,
            Some(place_body),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "place seed failed: {body}");
    let place = body["result"]["id"].as_str().unwrap().to_string();

    let cases = [
        ("POST", "/api/v1/maps".to_string(), Some(map_body("nope", false))),
        ("PATCH", format!("/api/v1/maps/{map}"), Some(json!({"name": "nope"}))),
        ("DELETE", format!("/api/v1/maps/{map}"), None),
        ("POST", format!("/api/v1/maps/{map}/places"), Some(zone_body("nope"))),
        (
            "PATCH",
            format!("/api/v1/maps/{map}/places/{place}"),
            Some(json!({"name": "nope"})),
        ),
        ("DELETE", format!("/api/v1/maps/{map}/places/{place}"), None),
    ];
    for (method, uri, body) in cases {
        let r = app
            .clone()
            .oneshot(auth_request(method, &uri, &viewer, body))
            .await
            .unwrap();
        assert_eq!(
            response_parts(r).await.0,
            StatusCode::FORBIDDEN,
            "{method} {uri} must require admin"
        );
    }

    let r = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/maps", &viewer, None))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::OK);
}

#[tokio::test]
async fn maps_and_places_are_tenant_isolated() {
    let (app, _pool, _state) = setup().await;
    let token_a = admin_token("ws-a");
    let token_b = admin_token("ws-b");
    let map = create_map(&app, &token_a, map_body("floor-1", true)).await;
    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token_a,
            Some(zone_body("zone-a")),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "place seed failed: {body}");
    let place = body["result"]["id"].as_str().unwrap().to_string();

    // ws-b 看不到 ws-a 的图/地点
    for (method, uri, body) in [
        ("GET", format!("/api/v1/maps/{map}"), None),
        ("PATCH", format!("/api/v1/maps/{map}"), Some(json!({"name": "hijack"}))),
        ("DELETE", format!("/api/v1/maps/{map}"), None),
        ("GET", format!("/api/v1/maps/{map}/places"), None),
        ("POST", format!("/api/v1/maps/{map}/places"), Some(zone_body("hijack"))),
        (
            "PATCH",
            format!("/api/v1/maps/{map}/places/{place}"),
            Some(json!({"name": "hijack"})),
        ),
        ("DELETE", format!("/api/v1/maps/{map}/places/{place}"), None),
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

    let r = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/maps", &token_b, None))
        .await
        .unwrap();
    let (_, body) = response_parts(r).await;
    assert_eq!(body["result"], json!([]));
}

// ── Position ──────────────────────────────────────────

#[tokio::test]
async fn position_get_put_validation_and_isolation() {
    let (app, _pool, state) = setup().await;
    let token_a = admin_token("ws-a");
    let token_b = admin_token("ws-b");
    let _ = create_map(&app, &token_a, map_body("floor-1", true)).await;
    let thing = create_thing(&app, &token_a, "ws-a", "bot-1").await;
    // API 校正不得触碰驱动摄取门 / 显式 zoneRef。
    state
        .db
        .set_thing_position_ingest("ws-a", &thing, true)
        .await
        .expect("enable ingest");
    state
        .db
        .set_thing_zone_ref("ws-a", &thing, Some("zone-a"))
        .await
        .expect("seed zone ref");

    let r = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{thing}/position"),
            &token_a,
            None,
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "position get failed: {body}");
    assert_eq!(body["result"]["x"], Value::Null);

    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            &token_a,
            Some(json!({"x": 10.0, "y": 20.0, "level": 2, "heading": 90.0})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "position put failed: {body}");
    assert_eq!(body["result"]["x"], json!(10.0));
    assert_eq!(body["result"]["y"], json!(20.0));
    assert_eq!(body["result"]["level"], json!(2));
    assert_eq!(body["result"]["trust"], json!("trusted"));
    assert!(body["result"]["updatedAt"].is_string());

    let r = app
        .clone()
        .oneshot(auth_request(
            "GET",
            &format!("/api/v1/things/{thing}/position"),
            &token_a,
            None,
        ))
        .await
        .unwrap();
    let (_, body) = response_parts(r).await;
    assert_eq!(body["result"]["x"], json!(10.0));
    assert_eq!(body["result"]["y"], json!(20.0));
    assert!(
        state.db.get_thing_position_ingest("ws-a", &thing).await.unwrap(),
        "API correction must not change position_ingest"
    );
    assert_eq!(
        state.db.get_thing_zone_ref("ws-a", &thing).await.unwrap().as_deref(),
        Some("zone-a"),
        "API correction must not change zone_ref"
    );

    // 越界 → 400
    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            &token_a,
            Some(json!({"x": 200.0, "y": 20.0})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    // 跨工作区：读/写都 404（物不可见）
    for method in ["GET", "PUT"] {
        let body = (method == "PUT").then(|| json!({"x": 1.0, "y": 1.0}));
        let r = app
            .clone()
            .oneshot(auth_request(
                method,
                &format!("/api/v1/things/{thing}/position"),
                &token_b,
                body,
            ))
            .await
            .unwrap();
        assert_eq!(response_parts(r).await.0, StatusCode::NOT_FOUND);
    }

    // 无默认图 + 无 map_id → 400（无法校验 bounds）
    let lonely = create_thing(&app, &token_b, "ws-b", "bot-2").await;
    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{lonely}/position"),
            &token_b,
            Some(json!({"x": 1.0, "y": 1.0})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn position_put_requires_admin() {
    let (app, _pool, _state) = setup().await;
    let admin = admin_token("ws-a");
    let _ = create_map(&app, &admin, map_body("floor-1", true)).await;
    let thing = create_thing(&app, &admin, "ws-a", "bot-1").await;

    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            &viewer_token("ws-a"),
            Some(json!({"x": 1.0, "y": 1.0})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::FORBIDDEN);
}

/// 物已有非默认 `map_id` 时，PUT 按该图校验（而非默认图）。
#[tokio::test]
async fn position_put_uses_thing_map_not_default() {
    let (app, pool, _state) = setup().await;
    let token = admin_token("ws-a");
    let _ = create_map(&app, &token, map_body("floor-1", true)).await;
    let annex = create_map(
        &app,
        &token,
        json!({"name": "annex", "bounds": {"x": 1000.0, "y": 1000.0, "w": 50.0, "h": 50.0}}),
    )
    .await;
    let thing = create_thing(&app, &token, "ws-a", "bot-annex").await;
    sqlx::query("UPDATE things SET map_id = ? WHERE id = ?")
        .bind(&annex)
        .bind(&thing)
        .execute(&pool)
        .await
        .expect("assign annex map");

    // 默认图内、annex 外 → 400（证明按物所属图校验）
    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            &token,
            Some(json!({"x": 10.0, "y": 10.0})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::BAD_REQUEST);

    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            &token,
            Some(json!({"x": 1010.0, "y": 1010.0})),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::OK, "annex position rejected: {body}");
    assert_eq!(body["result"]["mapId"], json!(annex));
}

/// 并发重名：预检查之外由 DB UNIQUE 兜底，HTTP 必须是一个 201 + 一个 409
/// （不能出现 500）。
#[tokio::test]
async fn concurrent_duplicate_map_create_returns_conflict_not_500() {
    let (app, _pool, _state) = setup().await;
    let token = admin_token("ws-a");
    let body = map_body("race", false);
    let (first, second) = tokio::join!(
        app.clone()
            .oneshot(auth_request("POST", "/api/v1/maps", &token, Some(body.clone()))),
        app.clone()
            .oneshot(auth_request("POST", "/api/v1/maps", &token, Some(body)))
    );
    let a = response_parts(first.unwrap()).await.0;
    let b = response_parts(second.unwrap()).await.0;
    assert!(
        matches!(
            (a, b),
            (StatusCode::CREATED, StatusCode::CONFLICT) | (StatusCode::CONFLICT, StatusCode::CREATED)
        ),
        "expected one create + one conflict, got {a} / {b}"
    );
}

// ── 缓存失效 ──────────────────────────────────────────

#[tokio::test]
async fn place_writes_invalidate_zone_cache() {
    let (app, _pool, state, zones) = setup_with_zones().await;
    let token = admin_token("ws-a");
    state
        .db
        .set_zone_source("ws-a", "maps")
        .await
        .expect("enable maps zone authority");
    let map = create_map(&app, &token, map_body("floor-1", true)).await;
    let thing = create_thing(&app, &token, "ws-a", "bot-1").await;
    let now = chrono::Utc::now();

    let r = app
        .clone()
        .oneshot(auth_request(
            "PUT",
            &format!("/api/v1/things/{thing}/position"),
            &token,
            Some(json!({"x": 2.0, "y": 2.0})),
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::OK);

    // 首次解析填充缓存（此时地图上还没有 zone）。
    assert_eq!(zones.zone_for("ws-a", &thing, None, now).await.unwrap(), None);

    let r = app
        .clone()
        .oneshot(auth_request(
            "POST",
            &format!("/api/v1/maps/{map}/places"),
            &token,
            Some(zone_body("zone-a")),
        ))
        .await
        .unwrap();
    let (status, body) = response_parts(r).await;
    assert_eq!(status, StatusCode::CREATED, "place create failed: {body}");
    let place = body["result"]["id"].as_str().unwrap().to_string();

    // 60s TTL 缓存已被写穿失效：新 zone 立即可见。
    assert_eq!(
        zones.zone_for("ws-a", &thing, None, now).await.unwrap().as_deref(),
        Some("zone-a"),
        "CRUD must invalidate the workspace zone cache"
    );

    let r = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/maps/{map}/places/{place}"),
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response_parts(r).await.0, StatusCode::OK);
    assert_eq!(
        zones.zone_for("ws-a", &thing, None, now).await.unwrap(),
        None,
        "delete must invalidate as well"
    );
}

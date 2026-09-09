use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tempfile::TempDir;
use tinyiothub_marketplace::{AppState, build_app, cache::SledCache, service::SyncService};
use tower::ServiceExt;

const TEMPLATE_1: &str = r#"{
  "name": "temperature_sensor",
  "display_name": { "zh": "温度传感器", "en": "Temperature Sensor" },
  "description": { "zh": "工业级温度传感器", "en": "Industrial temperature sensor" },
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "sensor",
  "manufacturer": "TinyIoT",
  "device_type": "sensor",
  "protocol_type": "modbus",
  "driver_name": "modbus_rtu",
  "tags": ["temperature", "sensor"],
  "device_info": { "default_name_pattern": "temp_{index}" }
}"#;

const TEMPLATE_2: &str = r#"{
  "name": "onvif_camera",
  "display_name": { "zh": "ONVIF摄像头", "en": "ONVIF Camera" },
  "description": { "zh": "网络摄像头", "en": "IP camera" },
  "version": "2.0.0",
  "author": "TinyIoT",
  "category": "camera",
  "manufacturer": "Generic",
  "device_type": "camera",
  "protocol_type": "onvif",
  "driver_name": "onvif_generic",
  "tags": ["camera", "video"]
}"#;

const DRIVER_1: &str = r#"{
  "id": "bacnet",
  "name": "BACnet Driver",
  "version": "2.1.0",
  "protocol": "bacnet",
  "description": "BACnet protocol driver",
  "tags": ["bacnet"],
  "author_name": "Test Team",
  "author_email": "test@test.com",
  "license": "MIT",
  "updated_at": "2025-01-15T12:00:00Z"
}"#;

const DRIVER_2: &str = r#"{
  "id": "opcua",
  "name": "OPC UA Driver",
  "version": "3.0.0",
  "protocol": "opcua",
  "description": "OPC UA protocol driver",
  "tags": ["opcua"],
  "author_name": "Test Team",
  "license": "MIT",
  "updated_at": "2024-12-01T10:00:00Z"
}"#;

async fn setup() -> (axum::Router, TempDir) {
    let tmp = TempDir::new().expect("create temp dir");

    let templates_dir = tmp.path().join("templates");
    std::fs::create_dir(&templates_dir).unwrap();
    std::fs::write(templates_dir.join("temp_sensor.json"), TEMPLATE_1).unwrap();
    std::fs::write(templates_dir.join("onvif_camera.json"), TEMPLATE_2).unwrap();

    let drivers_dir = tmp.path().join("drivers");
    std::fs::create_dir(&drivers_dir).unwrap();
    std::fs::write(drivers_dir.join("bacnet.json"), DRIVER_1).unwrap();
    std::fs::write(drivers_dir.join("opcua.json"), DRIVER_2).unwrap();

    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));
    sync.load_local_data().await.expect("load seed data");

    (build_app(AppState::new(cache, sync)), tmp)
}

fn empty_body() -> Body {
    Body::empty()
}

async fn read_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_list_templates() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(Request::builder().uri("/api/v1/templates").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(json["result"]["total"], 2);
}

#[tokio::test]
async fn test_list_templates_with_pagination() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?per_page=1&page=1")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["result"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(json["result"]["total"], 2);
    assert_eq!(json["result"]["page"], 1);
}

#[tokio::test]
async fn test_list_templates_page2() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?per_page=1&page=2")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["result"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(json["result"]["page"], 2);
}

#[tokio::test]
async fn test_list_templates_filter_by_category() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?category=camera")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    let items = json["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "onvif_camera");
}

#[tokio::test]
async fn test_list_templates_filter_by_protocol() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?protocol=modbus")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    let items = json["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "temperature_sensor");
}

#[tokio::test]
async fn test_list_templates_search() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?search=temperature")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    let items = json["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "temperature_sensor");
}

#[tokio::test]
async fn test_get_template() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates/temperature_sensor")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["name"], "temperature_sensor");
    assert_eq!(json["result"]["version"], "1.0.0");
    assert_eq!(json["result"]["protocol_type"], "modbus");
}

#[tokio::test]
async fn test_get_template_not_found() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates/nonexistent")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_drivers() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(Request::builder().uri("/api/v1/drivers").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_drivers_filter_by_protocol() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/drivers?protocol=opcua")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    let items = json["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "opcua");
}

#[tokio::test]
async fn test_get_driver() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/drivers/bacnet")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["id"], "bacnet");
    assert_eq!(json["result"]["version"], "2.1.0");
}

#[tokio::test]
async fn test_get_driver_not_found() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/drivers/nonexistent")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_templates_invalid_pagination() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?page=0")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_health_endpoint() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(Request::builder().uri("/health").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ── 场景包模板（category = "scenes"，无 device_type/protocol_type/driver_name）──

const SCENE_PACK: &str = r#"{
  "name": "smart_building",
  "display_name": { "zh": "智慧楼宇", "en": "Smart Building" },
  "description": { "zh": "楼栋 + N 层", "en": "Building with N floors" },
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "building",
  "tags": ["building", "space"],
  "parameters": [
    { "name": "floor_count", "type": "int", "default": 10, "min": 1, "max": 15 }
  ],
  "children": [
    { "key": "floor", "category": "floor", "count_param": "floor_count" }
  ]
}"#;

async fn setup_scene() -> (axum::Router, TempDir) {
    let tmp = TempDir::new().expect("create temp dir");

    let templates_dir = tmp.path().join("templates");
    std::fs::create_dir(&templates_dir).unwrap();
    std::fs::write(templates_dir.join("smart_building.json"), SCENE_PACK).unwrap();

    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));
    sync.load_local_data().await.expect("load seed data");

    (build_app(AppState::new(cache, sync)), tmp)
}

#[tokio::test]
async fn test_scene_pack_listed() {
    let (app, _tmp) = setup_scene().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?category=scenes")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["code"], 0);
    assert_eq!(json["result"]["total"], 1);
    assert_eq!(json["result"]["items"][0]["name"], "smart_building");
}

#[tokio::test]
async fn test_scene_pack_roundtrip_preserves_extra_fields() {
    let (app, _tmp) = setup_scene().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates/smart_building")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["code"], 0);
    let result = &json["result"];
    assert_eq!(result["name"], "smart_building");
    assert_eq!(result["thing_category"], "building");
    assert_eq!(result["parameters"][0]["name"], "floor_count");
    assert_eq!(result["children"][0]["key"], "floor");
}

// ── 真实发布文件校验（防止"内联 fixture 全绿、真实文件炸运行时"）──

/// 遍历目录下所有 JSON 并反序列化到运行时类型 T——任一失败即 panic。
fn assert_dir_matches_schema<T: serde::de::DeserializeOwned>(dir: &str, type_name: &str) {
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{} missing: {}", dir, e)) {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("{:?} is not valid JSON: {}", path, e));
        serde_json::from_value::<T>(value).unwrap_or_else(|e| panic!("{:?} failed {} schema: {}", path, type_name, e));
        count += 1;
    }
    assert!(count > 0, "no shipped files found in {}", dir);
}

#[test]
fn test_shipped_template_files_match_schema() {
    assert_dir_matches_schema::<tinyiothub_marketplace::types::Template>(
        concat!(env!("CARGO_MANIFEST_DIR"), "/templates"),
        "Template",
    );
}

#[test]
fn test_shipped_driver_files_match_schema() {
    assert_dir_matches_schema::<tinyiothub_marketplace::types::Driver>(
        concat!(env!("CARGO_MANIFEST_DIR"), "/drivers"),
        "Driver",
    );
}

// ── 参数别名（前端经 cloud proxy 透传 page_size / protocol_type）──

#[tokio::test]
async fn test_list_templates_page_size_alias() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates?page_size=1&page=2")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["result"]["per_page"], 1);
    assert_eq!(json["result"]["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_list_drivers_protocol_type_alias() {
    let (app, _tmp) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/drivers?protocol_type=bacnet")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["result"]["total"], 1);
    assert_eq!(json["result"]["items"][0]["id"], "bacnet");
}

// ── 冷缓存语义（数据未加载 ≠ 资源不存在）──

/// 只建缓存、不加载数据，模拟启动时 load_local_data 失败后的冷缓存状态
async fn setup_cold() -> (axum::Router, TempDir) {
    let tmp = TempDir::new().expect("create temp dir");
    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));
    (build_app(AppState::new(cache, sync)), tmp)
}

#[tokio::test]
async fn test_get_template_cold_cache_returns_503() {
    let (app, _tmp) = setup_cold().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/templates/temperature_sensor")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers().get("x-cache-stale").unwrap(), "true");
    let json = read_body(response).await;
    assert_eq!(json["code"], 50301);
}

#[tokio::test]
async fn test_list_templates_cold_cache_empty_with_stale_header() {
    let (app, _tmp) = setup_cold().await;

    let response = app
        .oneshot(Request::builder().uri("/api/v1/templates").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("x-cache-stale").unwrap(), "true");
    let json = read_body(response).await;
    assert_eq!(json["result"]["total"], 0);
}

#[tokio::test]
async fn test_health_degraded_when_cold() {
    let (app, _tmp) = setup_cold().await;

    let response = app
        .oneshot(Request::builder().uri("/health").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json = read_body(response).await;
    assert_eq!(json["result"]["status"], "degraded");
    assert_eq!(json["result"]["reason"], "cache_cold");
}

// ── 加载期去重/丢弃（dedup_and_sort）──

#[tokio::test]
async fn test_duplicate_template_name_deduped() {
    let tmp = TempDir::new().expect("create temp dir");
    let templates_dir = tmp.path().join("templates");
    std::fs::create_dir(&templates_dir).unwrap();
    // 两个文件、同一个 name —— 列表只能出现一次
    std::fs::write(templates_dir.join("a.json"), TEMPLATE_1).unwrap();
    std::fs::write(templates_dir.join("b.json"), TEMPLATE_1).unwrap();

    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));
    sync.load_local_data().await.expect("load seed data");
    let app = build_app(AppState::new(cache, sync));

    let response = app
        .oneshot(Request::builder().uri("/api/v1/templates").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_body(response).await;
    assert_eq!(json["result"]["total"], 1);
}

#[tokio::test]
async fn test_template_missing_name_dropped() {
    let tmp = TempDir::new().expect("create temp dir");
    let templates_dir = tmp.path().join("templates");
    std::fs::create_dir(&templates_dir).unwrap();
    std::fs::write(templates_dir.join("temp_sensor.json"), TEMPLATE_1).unwrap();
    // 缺 name 字段的条目应被丢弃并告警
    std::fs::write(templates_dir.join("nameless.json"), r#"{"category": "sensor"}"#).unwrap();

    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));
    sync.load_local_data().await.expect("load seed data");
    let app = build_app(AppState::new(cache, sync));

    let response = app
        .oneshot(Request::builder().uri("/api/v1/templates").body(empty_body()).unwrap())
        .await
        .unwrap();

    let json = read_body(response).await;
    assert_eq!(json["result"]["total"], 1);
    assert_eq!(json["result"]["items"][0]["name"], "temperature_sensor");
}

// ── drivers 冷缓存（与 templates 镜像对称）──

#[tokio::test]
async fn test_get_driver_cold_cache_returns_503() {
    let (app, _tmp) = setup_cold().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/drivers/bacnet")
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers().get("x-cache-stale").unwrap(), "true");
    let json = read_body(response).await;
    assert_eq!(json["code"], 50301);
}

#[tokio::test]
async fn test_list_drivers_cold_cache_empty_with_stale_header() {
    let (app, _tmp) = setup_cold().await;

    let response = app
        .oneshot(Request::builder().uri("/api/v1/drivers").body(empty_body()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("x-cache-stale").unwrap(), "true");
    let json = read_body(response).await;
    assert_eq!(json["result"]["total"], 0);
}

// ── 红队场景：数据源目录缺失不得呈现为"健康的空目录" ──

#[tokio::test]
async fn test_load_fails_when_data_dirs_missing() {
    let tmp = TempDir::new().expect("create temp dir");
    // 故意不创建 templates/ 和 drivers/ 子目录
    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));

    let result = sync.load_local_data().await;
    assert!(
        result.is_err(),
        "missing data dirs must fail the load, not produce a healthy empty catalog"
    );

    // 缓存保持冷态：list 带 stale 头、health degraded —— 降级对调用方可见
    let app = build_app(AppState::new(cache, sync));
    let response = app
        .oneshot(Request::builder().uri("/api/v1/templates").body(empty_body()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers().get("x-cache-stale").unwrap(), "true");
    assert_eq!(read_body(response).await["result"]["total"], 0);
}

// ── 红队场景：schema 不合规文件不入缓存（total 不虚增）──

#[tokio::test]
async fn test_schema_invalid_file_dropped_from_total() {
    let tmp = TempDir::new().expect("create temp dir");
    let templates_dir = tmp.path().join("templates");
    std::fs::create_dir(&templates_dir).unwrap();
    std::fs::write(templates_dir.join("temp_sensor.json"), TEMPLATE_1).unwrap();
    // 缺 version/author 等必填字段 —— 不合规，应被丢弃而非占住 total
    std::fs::write(
        templates_dir.join("broken.json"),
        r#"{"name": "broken", "display_name": {"zh": "坏", "en": "Broken"}, "description": {"zh": "x", "en": "x"}, "category": "sensor"}"#,
    )
    .unwrap();

    let drivers_dir = tmp.path().join("drivers");
    std::fs::create_dir(&drivers_dir).unwrap();

    let sled_path = tmp.path().join("cache.sled");
    let cache = Arc::new(SledCache::new(sled_path.to_str().unwrap()).expect("create sled cache"));
    let sync = Arc::new(SyncService::new(Arc::clone(&cache), tmp.path().to_path_buf()));
    sync.load_local_data().await.expect("load seed data");
    let app = build_app(AppState::new(cache, sync));

    let response = app
        .oneshot(Request::builder().uri("/api/v1/templates").body(empty_body()).unwrap())
        .await
        .unwrap();
    let json = read_body(response).await;
    assert_eq!(json["result"]["total"], 1, "invalid file must not inflate total");
    assert_eq!(json["result"]["items"][0]["name"], "temperature_sensor");
}

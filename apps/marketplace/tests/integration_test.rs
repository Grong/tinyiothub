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

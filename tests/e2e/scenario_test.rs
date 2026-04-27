//! End-to-end scenario tests.
//!
//! These tests run against a fully deployed TinyIoTHub instance
//! (local or Docker Compose) and exercise realistic user workflows.
//!
//! Run with:
//!   cargo test --test scenario_test -- --ignored
//!
//! (Tests are `#[ignore]` by default because they require external services.)

const BASE_URL: &str = "http://localhost:3002";

#[ignore = "requires running TinyIoTHub server"]
#[tokio::test]
async fn test_health_endpoint() {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health", BASE_URL))
        .send()
        .await
        .expect("failed to connect");

    assert!(resp.status().is_success());
}

#[ignore = "requires running TinyIoTHub server"]
#[tokio::test]
async fn test_device_lifecycle() {
    // 1. Create device
    // 2. Query device list
    // 3. Update device
    // 4. Delete device
}

#[ignore = "requires running TinyIoTHub server"]
#[tokio::test]
async fn test_telemetry_ingestion() {
    // 1. Publish MQTT message
    // 2. Verify device property updated
    // 3. Verify alarm triggered if threshold exceeded
}

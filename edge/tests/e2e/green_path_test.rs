use super::*;

/// Green path: Edge boots in authenticated mode (pre-saved credentials),
/// connects to MQTT broker, publishes heartbeat on the status topic, and
/// publishes telemetry on the telemetry topic.
///
/// Prerequisites:
///   docker compose -f edge/tests/e2e/docker-compose.yml up -d
///   cargo build -p tinyiothub-edge
///
/// Run with:
///   cargo test -p tinyiothub-edge --test e2e_tests -- green_path --ignored --test-threads=1
#[tokio::test]
#[ignore = "requires docker: docker compose -f edge/tests/e2e/docker-compose.yml up -d"]
async fn test_green_path_pairing_to_telemetry() {
    let mqtt = MqttTestClient::new().await;

    let credentials = serde_json::json!({
        "device_id": "test-gw-1",
        "client_id": "test-gw-1-client",
        "username": "test-user",
        "password": "test-pass",
        "workspace_id": "ws-e2e-test",
    });
    let credentials_json = serde_json::to_string_pretty(&credentials).unwrap();

    // Start edge in authenticated mode (has credentials.json)
    let _edge = EdgeProcess::start(&credentials_json);

    // Wait for heartbeat on status topic
    let status_filter = "tinyiothub/ws-e2e-test/gateway/test-gw-1/status";
    mqtt.subscribe(status_filter).await;

    let heartbeat = mqtt.wait_for_message(status_filter, 20).await;
    assert!(
        heartbeat.is_some(),
        "Edge should publish heartbeat on {status_filter} within 20s"
    );

    let (_topic, payload) = heartbeat.unwrap();
    let report: serde_json::Value = serde_json::from_slice(&payload).expect("Heartbeat payload should be valid JSON");
    assert_eq!(report["status"], "online", "Health report status should be 'online'");
    assert!(
        report["uptime_secs"].as_u64().is_some(),
        "Health report should include uptime_secs"
    );

    // Wait for telemetry on telemetry topic
    let telemetry_filter = "tinyiothub/ws-e2e-test/gateway/test-gw-1/telemetry";
    mqtt.subscribe(telemetry_filter).await;

    let telemetry = mqtt.wait_for_message(telemetry_filter, 15).await;
    assert!(
        telemetry.is_some(),
        "Edge should publish telemetry on {telemetry_filter} within 15s"
    );

    eprintln!("Green path test completed: heartbeat + telemetry verified");
}

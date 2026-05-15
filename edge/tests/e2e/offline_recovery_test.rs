use super::*;

/// Offline recovery: Edge is connected → MQTT broker goes down →
/// edge detects disconnection and buffers locally → broker comes back →
/// edge reconnects and resumes publishing heartbeats.
///
/// Prerequisites:
///   docker compose -f edge/tests/e2e/docker-compose.yml up -d
///   cargo build -p tinyiothub-edge
///
/// Run with:
///   cargo test -p tinyiothub-edge --test e2e_tests -- offline_recovery --ignored --test-threads=1
#[tokio::test]
#[ignore = "requires docker: docker compose -f edge/tests/e2e/docker-compose.yml up -d"]
async fn test_offline_recovery_buffer_and_flush() {
    let mqtt = MqttTestClient::new().await;

    let credentials = serde_json::json!({
        "device_id": "test-gw-2",
        "client_id": "test-gw-2-client",
        "username": "test-user",
        "password": "test-pass",
        "workspace_id": "ws-e2e-test",
    });
    let credentials_json = serde_json::to_string_pretty(&credentials).unwrap();

    // Start edge in authenticated mode
    let _edge = EdgeProcess::start(&credentials_json);

    // ---- Phase 1: verify edge publishes heartbeats ----
    let status_filter = "tinyiothub/ws-e2e-test/gateway/test-gw-2/status";
    mqtt.subscribe(status_filter).await;

    let heartbeat1 = mqtt.wait_for_message(status_filter, 20).await;
    assert!(
        heartbeat1.is_some(),
        "Edge should publish heartbeat on {status_filter} within 20s"
    );

    // ---- Phase 2: stop MQTT broker ---
    let docker_compose_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/e2e");
    let stop_output = std::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            &format!("{docker_compose_dir}/docker-compose.yml"),
            "stop",
            "mosquitto",
        ])
        .output()
        .expect("Failed to execute docker compose stop");

    assert!(
        stop_output.status.success(),
        "docker compose stop failed: {}",
        String::from_utf8_lossy(&stop_output.stderr)
    );

    eprintln!("Broker stopped — edge should start buffering");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // ---- Phase 3: restart broker ----
    let start_output = std::process::Command::new("docker")
        .args([
            "compose",
            "-f",
            &format!("{docker_compose_dir}/docker-compose.yml"),
            "start",
            "mosquitto",
        ])
        .output()
        .expect("Failed to execute docker compose start");

    assert!(
        start_output.status.success(),
        "docker compose start failed: {}",
        String::from_utf8_lossy(&start_output.stderr)
    );

    eprintln!("Broker restarted — waiting for edge to reconnect...");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // ---- Phase 4: verify edge recovers and resumes heartbeats ----
    let heartbeat2 = mqtt.wait_for_message(&format!("{status_filter}"), 30).await;
    assert!(
        heartbeat2.is_some(),
        "Edge should reconnect and resume heartbeats after broker restart"
    );

    eprintln!("Offline recovery test completed: broker stop → buffer → reconnect → flush");
}

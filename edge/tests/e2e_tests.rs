#[tokio::test]
#[ignore = "requires mosquitto container — will be implemented in Task 13"]
async fn test_pairing_to_telemetry_green_path() {
    // E2E: Edge boots → pairs → scans → collects telemetry → heartbeats
    // Requires mosquitto container + test edge binary
}

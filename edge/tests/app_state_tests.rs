use tinyiothub_edge::app_state::AppState;
use tinyiothub_edge::config::EdgeConfig;

fn test_config() -> EdgeConfig {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("edge.db");
    // Leak the temp dir so it lives for the duration of the test
    std::mem::forget(dir);
    EdgeConfig {
        db_path,
        ..EdgeConfig::default()
    }
}

#[tokio::test]
async fn test_app_state_init_success() {
    let config = test_config();
    let creds = tinyiothub_edge::config::GatewayCredentials {
        device_id: "test-dev".into(),
        client_id: "test-client".into(),
        username: "user".into(),
        password: "pass".into(),
        workspace_id: "ws-1".into(),
    };
    let state = AppState::new(config, creds).await;
    assert!(
        state.is_ok(),
        "AppState should init successfully, got: {:?}",
        state.err()
    );
}

#[tokio::test]
async fn test_app_state_is_cloneable() {
    let config = test_config();
    let creds = tinyiothub_edge::config::GatewayCredentials {
        device_id: "test-dev".into(),
        client_id: "test-client".into(),
        username: "user".into(),
        password: "pass".into(),
        workspace_id: "ws-1".into(),
    };
    let state = AppState::new(config, creds).await.unwrap();
    let _state2 = state.clone();
    // Verify services are accessible after clone
    assert!(_state2.device_service.list_devices(None).await.is_ok());
}

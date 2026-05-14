use std::path::PathBuf;
use tinyiothub_edge::config::EdgeConfig;

#[test]
fn test_default_config_has_new_fields() {
    let config = EdgeConfig::default();
    assert_eq!(config.telemetry_interval_secs, 30);
    assert_eq!(config.intelligence_interval_secs, 60);
    assert_eq!(config.offline_buffer_max_telemetry, 100_000);
    assert_eq!(config.offline_buffer_disk_min_percent, 10);
    assert_eq!(config.offline_buffer_reserved_mb, 5);
    assert_eq!(config.local_api_enabled, false);
    assert_eq!(config.config_file.to_string_lossy(), "/app/data/config.yaml");
    assert_eq!(config.scan_timeout_secs, 10);
    assert_eq!(config.mqtt_reconnect_max_backoff_secs, 300);
}

#[test]
fn test_load_from_yaml_file() {
    let tmp = std::env::temp_dir().join("test_edge_config.yaml");
    let yaml = r#"
mqtt_broker: "test.mqtt.com"
mqtt_port: 8883
pairing_interval_secs: 60
heartbeat_interval_secs: 45
telemetry_interval_secs: 15
intelligence_interval_secs: 120
offline_buffer_max_telemetry: 50000
offline_buffer_disk_min_percent: 20
offline_buffer_reserved_mb: 10
local_api_enabled: true
local_api_port: 9090
scan_timeout_secs: 15
mqtt_reconnect_max_backoff_secs: 600
"#;
    std::fs::write(&tmp, yaml).unwrap();
    let config = EdgeConfig::load_from_file(&tmp).unwrap();
    assert_eq!(config.mqtt_broker, "test.mqtt.com");
    assert_eq!(config.mqtt_port, 8883);
    assert_eq!(config.telemetry_interval_secs, 15);
    assert_eq!(config.local_api_enabled, true);
    assert_eq!(config.local_api_port, 9090);
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_load_from_file_not_found_returns_default() {
    let config = EdgeConfig::load_from_file(&PathBuf::from("/nonexistent/path/config.yaml")).unwrap();
    assert_eq!(config.mqtt_broker, "mqtt.tinyiothub.com"); // default
}

#[test]
fn test_credentials_save_and_load() {
    use tinyiothub_edge::config::GatewayCredentials;
    let creds = GatewayCredentials {
        device_id: "d1".into(),
        client_id: "c1".into(),
        username: "u1".into(),
        password: "p1".into(),
        workspace_id: "ws1".into(),
    };
    let tmp = std::env::temp_dir().join("test_creds.json");
    creds.save(&tmp).unwrap();
    let loaded = GatewayCredentials::load(&tmp).unwrap();
    assert_eq!(loaded.device_id, "d1");
    assert_eq!(loaded.workspace_id, "ws1");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_credentials_validate_rejects_empty() {
    use tinyiothub_edge::config::GatewayCredentials;
    let creds = GatewayCredentials {
        device_id: "".into(),
        client_id: "c1".into(),
        username: "u1".into(),
        password: "p1".into(),
        workspace_id: "ws1".into(),
    };
    assert!(creds.validate().is_err());
}

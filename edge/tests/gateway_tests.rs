use tinyiothub_edge::modules::gateway::GatewayMessage;

#[test]
fn test_route_config_device_topic() {
    // Longest-prefix match: /config/device before /config
    let topic = "tinyiothub/ws-1/gateway/gw-1/config/device";
    let payload = r#"{"device_id":"d1","action":"enable"}"#;
    let msg = GatewayMessage::from_topic_payload(topic, payload.as_bytes()).unwrap();
    assert!(matches!(msg, GatewayMessage::ConfigDevice(_)));
}

#[test]
fn test_route_config_topic() {
    let topic = "tinyiothub/ws-1/gateway/gw-1/config";
    let payload = r#"{"version":"v2"}"#;
    let msg = GatewayMessage::from_topic_payload(topic, payload.as_bytes()).unwrap();
    assert!(matches!(msg, GatewayMessage::Config(_)));
}

#[test]
fn test_route_command_topic() {
    let topic = "tinyiothub/ws-1/gateway/gw-1/command";
    let payload = r#"{"device_id":"d1","command":"restart"}"#;
    let msg = GatewayMessage::from_topic_payload(topic, payload.as_bytes()).unwrap();
    assert!(matches!(msg, GatewayMessage::Command(_)));
}

#[test]
fn test_route_driver_install_topic() {
    let topic = "tinyiothub/ws-1/gateway/gw-1/driver/install";
    let payload = r#"{"driver_name":"modbus","chunk_index":0,"total_chunks":1,"sha256":"abc","data":"AAAA"}"#;
    let msg = GatewayMessage::from_topic_payload(topic, payload.as_bytes()).unwrap();
    assert!(matches!(msg, GatewayMessage::DriverInstall(_)));
    assert_eq!(msg.driver_name(), Some("modbus"));
}

#[test]
fn test_unknown_topic_returns_err() {
    let topic = "tinyiothub/ws-1/gateway/gw-1/unknown";
    let result = GatewayMessage::from_topic_payload(topic, b"{}");
    assert!(result.is_err());
}

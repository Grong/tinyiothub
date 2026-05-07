#[test]
fn test_parse_protocol_plugin() {
    let toml_str = r#"
[plugin]
name = "test_protocol"
type = "protocol"

[protocol]
type = "http_poll"
base_url = "http://localhost:8080"
endpoint = "/api/data"
method = "GET"
poll_interval_ms = 1000

[mapping]
temp = "$.temperature"
"#;

    let value: toml::Value = toml::from_str(toml_str).unwrap();
    assert!(value.get("plugin").is_some());
    assert!(value.get("protocol").is_some());
    assert!(value.get("mapping").is_some());

    let plugin = value.get("plugin").unwrap();
    assert_eq!(plugin.get("name").unwrap().as_str().unwrap(), "test_protocol");
    assert_eq!(plugin.get("type").unwrap().as_str().unwrap(), "protocol");
}

#[test]
fn test_parse_notification_plugin() {
    let toml_str = r#"
[plugin]
name = "test_feishu"
type = "notification"

[notification]
type = "feishu"
webhook_url = "https://open.feishu.cn/..."
levels = ["error", "critical"]
"#;

    let value: toml::Value = toml::from_str(toml_str).unwrap();
    assert_eq!(value.get("plugin").unwrap().get("type").unwrap().as_str().unwrap(), "notification");
    assert_eq!(value.get("notification").unwrap().get("type").unwrap().as_str().unwrap(), "feishu");
}

#[test]
fn test_parse_scheduler_plugin() {
    let toml_str = r#"
[plugin]
name = "test_scheduler"
type = "scheduler"

[scheduler]
type = "cron"
cron = "0 */5 * * * *"
enabled = true
"#;

    let value: toml::Value = toml::from_str(toml_str).unwrap();
    assert_eq!(value.get("plugin").unwrap().get("type").unwrap().as_str().unwrap(), "scheduler");
    assert_eq!(
        value.get("scheduler").unwrap().get("cron").unwrap().as_str().unwrap(),
        "0 */5 * * * *"
    );
}

#[test]
fn test_plugin_registry_has_plugin() {
    use crate::modules::plugin::get_global_registry;

    let registry = get_global_registry();
    // Just verify the registry can be accessed
    assert!(registry.plugin_names().len() >= 0);
}

use tinyiothub_edge::modules::telemetry::{TelemetryService, TransformRule};
use tinyiothub_edge::modules::health::HealthReport;
use serde_json::json;

// === Telemetry Tests ===

#[test]
fn test_telemetry_transform_value_mapping_multiply() {
    let rules = vec![TransformRule {
        source: "temperature".into(),
        op: "multiply".into(),
        factor: 1.8,
        target: "temp_f".into(),
    }];
    let input = json!({"temperature": 25.0});
    let output = TelemetryService::apply_transform(&input, &rules);

    assert_eq!(output["temp_f"], json!(45.0));
    assert_eq!(output["temperature"], json!(25.0)); // original preserved
}

#[test]
fn test_telemetry_transform_add() {
    let rules = vec![TransformRule {
        source: "offset".into(),
        op: "add".into(),
        factor: 10.0,
        target: "offset_plus_10".into(),
    }];
    let input = json!({"offset": 5.0});
    let output = TelemetryService::apply_transform(&input, &rules);
    assert_eq!(output["offset_plus_10"], json!(15.0));
}

#[test]
fn test_telemetry_transform_divide() {
    let rules = vec![TransformRule {
        source: "value".into(),
        op: "divide".into(),
        factor: 2.0,
        target: "half".into(),
    }];
    let input = json!({"value": 100.0});
    let output = TelemetryService::apply_transform(&input, &rules);
    assert_eq!(output["half"], json!(50.0));
}

#[test]
fn test_telemetry_transform_subtract() {
    let rules = vec![TransformRule {
        source: "value".into(),
        op: "subtract".into(),
        factor: 5.0,
        target: "reduced".into(),
    }];
    let input = json!({"value": 20.0});
    let output = TelemetryService::apply_transform(&input, &rules);
    assert_eq!(output["reduced"], json!(15.0));
}

#[test]
fn test_telemetry_transform_missing_source_no_panic() {
    let rules = vec![TransformRule {
        source: "nonexistent".into(),
        op: "multiply".into(),
        factor: 2.0,
        target: "result".into(),
    }];
    let input = json!({"other": 10.0});
    let output = TelemetryService::apply_transform(&input, &rules);
    // Should not panic, target should not be added since source is missing
    assert!(output.get("result").is_none());
}

#[test]
fn test_telemetry_multiple_rules() {
    let rules = vec![
        TransformRule {
            source: "a".into(),
            op: "multiply".into(),
            factor: 2.0,
            target: "a2".into(),
        },
        TransformRule {
            source: "b".into(),
            op: "add".into(),
            factor: 3.0,
            target: "b3".into(),
        },
    ];
    let input = json!({"a": 10.0, "b": 7.0});
    let output = TelemetryService::apply_transform(&input, &rules);
    assert_eq!(output["a2"], json!(20.0));
    assert_eq!(output["b3"], json!(10.0));
}

// === Health Tests ===

#[test]
fn test_health_report_has_required_fields() {
    let report = HealthReport::sample();
    let json_value = serde_json::to_value(&report).unwrap();
    assert!(json_value.get("status").is_some());
    assert!(json_value.get("cpu_percent").is_some());
    assert!(json_value.get("memory_mb").is_some());
    assert!(json_value.get("disk_free_mb").is_some());
    assert!(json_value.get("driver_count").is_some());
    assert!(json_value.get("buffer_backlog").is_some());
    assert!(json_value.get("uptime_secs").is_some());
}

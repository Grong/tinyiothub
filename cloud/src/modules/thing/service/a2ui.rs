// A2UI Ontology-Driven Rendering — maps ThingProfileResponse to A2UI component JSON
//
// Three component builders:
//   build_device_card   → DeviceCard (identity, status, key properties)
//   build_data_chart    → DataChart (numeric properties as chart series)
//   build_control_panel → ControlPanel (available actions/controls)
//
// Plus safe wrapper: render_a2ui_safe() → { type: "a2ui", components: {...} }
//                                        or { type: "fallback", data, message }

use serde_json::{Value, json};

use crate::modules::thing::types::ThingProfileResponse;

// ──────────────────────────────────────────────
// State → status string mapping
// ──────────────────────────────────────────────

fn state_to_status(state: i32) -> &'static str {
    match state {
        1 => "online",
        _ => "offline",
    }
}

// ──────────────────────────────────────────────
// Property extraction helpers (serde_json::Value)
// ──────────────────────────────────────────────

fn prop_name(p: &Value) -> &str {
    p.get("display_name").or_else(|| p.get("name")).and_then(|v| v.as_str()).unwrap_or("")
}

fn prop_value(p: &Value) -> &str {
    p.get("default_value")
        .or_else(|| p.get("value"))
        .or_else(|| p.get("current_value"))
        .and_then(|v| v.as_str())
        .unwrap_or("-")
}

fn prop_unit(p: &Value) -> Option<&str> {
    p.get("unit").and_then(|v| v.as_str())
}

fn prop_data_type(p: &Value) -> &str {
    p.get("data_type").and_then(|v| v.as_str()).unwrap_or("")
}

fn is_numeric_type(dt: &str) -> bool {
    matches!(
        dt.to_lowercase().as_str(),
        "int" | "integer" | "float" | "double" | "number" | "numeric"
    )
}

fn is_numeric_value(v: &Value) -> bool {
    // Check if the default_value looks like a number
    v.get("default_value")
        .or_else(|| v.get("value"))
        .map(|dv| dv.is_number() || dv.as_str().map(|s| s.parse::<f64>().is_ok()).unwrap_or(false))
        .unwrap_or(false)
}

// ──────────────────────────────────────────────
// Color palette for chart series
// ──────────────────────────────────────────────

const CHART_COLORS: &[&str] = &[
    "#3b82f6", "#ef4444", "#10b981", "#f59e0b", "#8b5cf6", "#ec4899", "#06b6d4", "#f97316",
    "#6366f1", "#14b8a6",
];

// ──────────────────────────────────────────────
// build_device_card
// ──────────────────────────────────────────────

/// Build A2UI DeviceCard JSON from thing profile.
pub fn build_device_card(profile: &ThingProfileResponse) -> Value {
    let t = &profile.thing;
    let properties = profile.properties.as_ref();

    let key_properties: Vec<Value> = properties
        .map(|props| {
            props
                .iter()
                .take(5)
                .map(|p| {
                    json!({
                        "name": prop_name(p),
                        "value": prop_value(p),
                        "unit": prop_unit(p),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let tags: Vec<&str> = {
        let mut v = Vec::new();
        v.push(t.thing_type.as_str());
        if let Some(ref dt) = t.device_type {
            v.push(dt.as_str());
        }
        if let Some(ref proto) = t.protocol_type {
            v.push(proto.as_str());
        }
        v
    };

    json!({
        "component": "DeviceCard",
        "props": {
            "id": t.id,
            "name": t.name,
            "deviceType": t.thing_type,
            "status": state_to_status(t.state),
            "breadcrumb": t.breadcrumb.iter().map(|b| json!({"name": b.name})).collect::<Vec<_>>(),
            "summary": t.ontology_summary,
            "tags": tags,
            "lastSeen": t.updated_at,
            "keyProperties": key_properties,
        }
    })
}

// ──────────────────────────────────────────────
// build_data_chart
// ──────────────────────────────────────────────

/// Build A2UI DataChart JSON from thing profile.
///
/// Selects numeric properties and renders them as chart series. Each property
/// becomes a series with a single data point (the current/default value).
pub fn build_data_chart(profile: &ThingProfileResponse) -> Value {
    let t = &profile.thing;

    let numeric_props: Vec<&Value> = profile
        .properties
        .as_ref()
        .map(|props| {
            props
                .iter()
                .filter(|p| is_numeric_type(prop_data_type(p)) || is_numeric_value(p))
                .collect()
        })
        .unwrap_or_default();

    let series: Vec<Value> = numeric_props
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let raw_value = p
                .get("default_value")
                .or_else(|| p.get("value"))
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .or_else(|| p.get("default_value").and_then(|v| v.as_f64().map(|n| n.to_string())))
                .or_else(|| p.get("value").and_then(|v| v.as_f64().map(|n| n.to_string())))
                .unwrap_or_default();
            let parsed: f64 = raw_value.parse().unwrap_or(0.0);
            json!({
                "name": prop_name(p),
                "color": CHART_COLORS[i % CHART_COLORS.len()],
                "data": [{
                    "time": t.updated_at,
                    "value": parsed,
                }],
            })
        })
        .collect();

    let unit = numeric_props.first().and_then(|p| prop_unit(p)).unwrap_or("");

    json!({
        "component": "DataChart",
        "props": {
            "thingId": t.id,
            "thingName": t.name,
            "title": format!("{} 数据概览", t.name),
            "unit": unit,
            "timeRange": "1h",
            "series": series,
        }
    })
}

// ──────────────────────────────────────────────
// build_control_panel
// ──────────────────────────────────────────────

/// Build A2UI ControlPanel JSON from thing profile.
///
/// Provides default controls for things. Device-type things get richer controls
/// (on/off, reboot); other types get a basic view action.
pub fn build_control_panel(profile: &ThingProfileResponse) -> Value {
    let t = &profile.thing;

    let controls: Vec<Value> = if t.thing_type == "device" {
        vec![
            json!({
                "id": "toggle_power",
                "type": "toggle",
                "label": "电源开关",
                "checked": t.state == 1,
            }),
            json!({
                "id": "reboot",
                "type": "button",
                "label": "重启设备",
                "variant": "secondary",
                "confirmMessage": "确认重启该设备？",
            }),
            json!({
                "id": "set_interval",
                "type": "slider",
                "label": "上报间隔",
                "min": 1,
                "max": 60,
                "step": 1,
                "value": 10,
                "unit": "秒",
            }),
        ]
    } else {
        vec![json!({
            "id": "view_details",
            "type": "button",
            "label": "查看详情",
            "variant": "primary",
        })]
    };

    json!({
        "component": "ControlPanel",
        "props": {
            "thingId": t.id,
            "thingName": t.name,
            "thingType": t.thing_type,
            "controls": controls,
            "actions": [],
        }
    })
}

// ──────────────────────────────────────────────
// build_a2ui_components
// ──────────────────────────────────────────────

/// Build all three A2UI components from profile.
pub fn build_a2ui_components(profile: &ThingProfileResponse) -> Value {
    json!({
        "deviceCard": build_device_card(profile),
        "dataChart": build_data_chart(profile),
        "controlPanel": build_control_panel(profile),
    })
}

// ──────────────────────────────────────────────
// render_a2ui_safe — fallback wrapper
// ──────────────────────────────────────────────

/// Wrap A2UI rendering with a catch_unwind fallback.
///
/// If the A2UI builders panic, returns the raw profile as JSON fallback so the
/// frontend can still display the data (rendered by the a2ui-fallback component).
pub fn render_a2ui_safe(profile: &ThingProfileResponse) -> Value {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build_a2ui_components(profile)))
    {
        Ok(components) => json!({"type": "a2ui", "components": components}),
        Err(_) => {
            json!({
                "type": "fallback",
                "data": profile,
                "message": "A2UI render failed, showing raw data"
            })
        }
    }
}

// ──────────────────────────────────────────────
// A2UI JSONL builder — for direct canvas tool use
// ──────────────────────────────────────────────

/// Build a complete A2UI JSONL string that creates a surface and pushes all
/// three components. This is the format expected by the canvas tool's
/// `jsonl` parameter.
pub fn build_a2ui_jsonl(
    profile: &ThingProfileResponse,
    surface_id: &str,
    surface_kind: &str,
) -> String {
    let components = build_a2ui_components(profile);

    let create_surface = json!({
        "createSurface": {
            "id": surface_id,
            "surfaceKind": surface_kind,
        }
    });

    let update_components = json!({
        "updateComponents": {
            "surfaceId": surface_id,
            "components": [
                {
                    "id": format!("{}-card", surface_id),
                    "componentKind": "DeviceCard",
                    "dataModel": components["deviceCard"]["props"],
                },
                {
                    "id": format!("{}-chart", surface_id),
                    "componentKind": "DataChart",
                    "dataModel": components["dataChart"]["props"],
                },
                {
                    "id": format!("{}-control", surface_id),
                    "componentKind": "ControlPanel",
                    "dataModel": components["controlPanel"]["props"],
                },
            ],
        }
    });

    format!(
        "{}\n{}",
        serde_json::to_string(&create_surface).unwrap_or_default(),
        serde_json::to_string(&update_components).unwrap_or_default(),
    )
}

// ──────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::thing::types::{BreadcrumbNode, ThingResponse};

    fn make_profile(state: i32, thing_type: &str) -> ThingProfileResponse {
        ThingProfileResponse {
            thing: ThingResponse {
                id: "thing-1".into(),
                workspace_id: Some("ws-1".into()),
                name: "温度传感器 A".into(),
                display_name: Some("温度传感器 A".into()),
                address: None,
                device_type: Some("temperature".into()),
                thing_type: thing_type.into(),
                parent_id: Some("parent-1".into()),
                template_id: None,
                state,
                driver_name: Some("modbus".into()),
                protocol_type: Some("modbus-tcp".into()),
                ontology_summary: Some("温湿度传感器，用于车间环境监测".into()),
                summary_status: Some("ok".into()),
                tags: vec![],
                breadcrumb: vec![
                    BreadcrumbNode {
                        id: "b-1".into(),
                        name: "工厂1".into(),
                        thing_type: "building".into(),
                    },
                    BreadcrumbNode {
                        id: "b-2".into(),
                        name: "车间A".into(),
                        thing_type: "space".into(),
                    },
                ],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-07-23T12:00:00Z".into(),
            },
            properties: Some(vec![
                json!({"name": "temperature", "display_name": "温度", "data_type": "float", "unit": "°C", "default_value": "25.5"}),
                json!({"name": "humidity", "display_name": "湿度", "data_type": "float", "unit": "%RH", "default_value": "60"}),
                json!({"name": "status", "display_name": "状态", "data_type": "string", "unit": null, "default_value": "running"}),
            ]),
            actions: None,
            recent_events: None,
            knowledge_docs: None,
        }
    }

    #[test]
    fn test_build_device_card_online() {
        let profile = make_profile(1, "device");
        let card = build_device_card(&profile);
        assert_eq!(card["component"], "DeviceCard");
        assert_eq!(card["props"]["id"], "thing-1");
        assert_eq!(card["props"]["name"], "温度传感器 A");
        assert_eq!(card["props"]["status"], "online");
        assert_eq!(card["props"]["deviceType"], "device");
        let kp = card["props"]["keyProperties"].as_array().unwrap();
        assert_eq!(kp.len(), 3);
        assert_eq!(kp[0]["name"], "温度");
        assert_eq!(kp[0]["value"], "25.5");
        assert_eq!(kp[0]["unit"], "°C");
    }

    #[test]
    fn test_build_device_card_offline() {
        let profile = make_profile(0, "device");
        let card = build_device_card(&profile);
        assert_eq!(card["props"]["status"], "offline");
    }

    #[test]
    fn test_build_data_chart() {
        let profile = make_profile(1, "device");
        let chart = build_data_chart(&profile);
        assert_eq!(chart["component"], "DataChart");
        assert_eq!(chart["props"]["thingId"], "thing-1");
        let series = chart["props"]["series"].as_array().unwrap();
        // temperature and humidity are numeric, status is string
        assert_eq!(series.len(), 2);
        assert_eq!(series[0]["name"], "温度");
    }

    #[test]
    fn test_build_control_panel_device() {
        let profile = make_profile(1, "device");
        let panel = build_control_panel(&profile);
        assert_eq!(panel["component"], "ControlPanel");
        let controls = panel["props"]["controls"].as_array().unwrap();
        // Device type gets toggle + button + slider = 3 controls
        assert!(controls.len() >= 2);
        assert_eq!(controls[0]["type"], "toggle");
    }

    #[test]
    fn test_build_control_panel_space() {
        let profile = make_profile(0, "space");
        let panel = build_control_panel(&profile);
        let controls = panel["props"]["controls"].as_array().unwrap();
        // Non-device type gets just the view button
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0]["type"], "button");
    }

    #[test]
    fn test_build_a2ui_components() {
        let profile = make_profile(1, "device");
        let all = build_a2ui_components(&profile);
        assert!(all["deviceCard"].is_object());
        assert!(all["dataChart"].is_object());
        assert!(all["controlPanel"].is_object());
    }

    #[test]
    fn test_render_a2ui_safe_success() {
        let profile = make_profile(1, "device");
        let result = render_a2ui_safe(&profile);
        assert_eq!(result["type"], "a2ui");
        assert!(result["components"].is_object());
    }

    #[test]
    fn test_build_a2ui_jsonl() {
        let profile = make_profile(1, "device");
        let jsonl = build_a2ui_jsonl(&profile, "test-surface", "insight");
        assert!(jsonl.contains("createSurface"));
        assert!(jsonl.contains("updateComponents"));
        assert!(jsonl.contains("test-surface-card"));
        assert!(jsonl.contains("DeviceCard"));
    }

    #[test]
    fn test_no_properties() {
        let mut profile = make_profile(1, "device");
        profile.properties = None;
        let card = build_device_card(&profile);
        assert!(card["props"]["keyProperties"].as_array().unwrap().is_empty());
        let chart = build_data_chart(&profile);
        assert!(chart["props"]["series"].as_array().unwrap().is_empty());
    }
}

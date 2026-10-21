//! Built-in driver instance declarations (E4a): derive from Thing config rows.

use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::manifest::{ManifestTool, NumericRange, StateFieldDecl, ToolKindDecl, ToolSafetyDecl};
use tinyiothub_core::models::thing::Thing;

pub fn declaration_from_thing(thing: &Thing, driver: &str, version: &str) -> DeviceDeclaration {
    let state_fields = thing
        .properties
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|p| StateFieldDecl {
            name: p.name.clone(),
            data_type: p.data_type.clone().unwrap_or_else(|| "string".into()),
            unit: p.unit.clone(),
            range: match (p.min_value, p.max_value) {
                (None, None) => None,
                (min, max) => Some(NumericRange { min, max }),
            },
            read_only: p.is_read_only != 0,
            required: true,
            source: Some("telemetry".into()),
        })
        .collect();
    let tools = thing
        .commands
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|c| ManifestTool {
            name: c.name.clone(),
            kind: ToolKindDecl::Action,
            target_field: None,
            parameters: c
                .parameters
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| serde_json::json!({"type":"object"})),
            timeout: Default::default(),
            safety: ToolSafetyDecl::Control,
            resources: vec![],
            cancellable: true,
        })
        .collect();
    DeviceDeclaration {
        manifest_version: "1.0".into(),
        driver: driver.into(),
        driver_version: version.into(),
        state_fields,
        tools,
    }
}

pub fn builtin_manifest_jsons() -> [(&'static str, &'static str); 3] {
    [
        ("simulator", include_str!("../../manifests/simulator.json")),
        ("modbus_rtu", include_str!("../../manifests/modbus_rtu.json")),
        ("snmp", include_str!("../../manifests/snmp.json")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::models::thing::{Thing, ThingStatus};
    use tinyiothub_core::models::thing_command::ThingCommand;
    use tinyiothub_core::models::thing_property::ThingProperty;

    fn thing() -> Thing {
        Thing {
            id: "t1".into(),
            name: "t1".into(),
            status: ThingStatus::Online,
            driver_name: Some("simulator".into()),
            properties: Some(vec![ThingProperty {
                id: "p1".into(),
                thing_id: "t1".into(),
                name: "temperature".into(),
                display_name: None,
                description: None,
                data_type: Some("float".into()),
                unit: Some("C".into()),
                min_value: Some(-40.0),
                max_value: Some(125.0),
                default_value: None,
                is_read_only: 1,
                source: Some("template".into()),
                expression: None,
                refs: None,
                created_at: None,
                updated_at: None,
                current_value: None,
                alarm_status: None,
                derived_status: None,
            }]),
            commands: Some(vec![ThingCommand {
                id: "c1".into(),
                thing_id: "t1".into(),
                name: "restart".into(),
                display_name: None,
                description: None,
                parameters: Some("{\"type\":\"object\"}".into()),
                created_at: "2026-10-21".into(),
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn maps_properties_to_state_fields() {
        let d = declaration_from_thing(&thing(), "simulator", "2.0.0");
        assert_eq!(d.state_fields[0].name, "temperature");
        assert_eq!(d.state_fields[0].range.as_ref().unwrap().max, Some(125.0));
    }

    #[test]
    fn maps_commands_to_action_tools() {
        let d = declaration_from_thing(&thing(), "simulator", "2.0.0");
        assert_eq!(d.tools[0].name, "restart");
        assert_eq!(d.tools[0].kind, tinyiothub_core::driver::manifest::ToolKindDecl::Action);
    }
}

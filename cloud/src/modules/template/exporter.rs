// cloud/src/modules/template/exporter.rs

use std::collections::HashMap;

use tinyiothub_core::models::device::Device;

use super::types::{CommandTemplate, DeviceInfo, DeviceTemplate, PropertyTemplate};

pub struct TemplateExporter;

impl TemplateExporter {
    /// Export a configured device as a template.
    pub fn export_from_device(device: &Device) -> Result<DeviceTemplate, String> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let name = format!("{}_template", device.name);
        let display_name = serde_json::json!({
            "zh": format!("{} 模板", device.display_name.as_deref().unwrap_or(&device.name)),
            "en": format!("{} Template", device.display_name.as_deref().unwrap_or(&device.name)),
        });

        let driver_options = Self::sanitize_driver_options(device.driver_options.as_deref());

        let device_info = DeviceInfo {
            default_name_pattern: format!("{}_{{index}}", device.name),
            default_display_name_pattern: None,
            default_description: None,
            default_position: device.position.clone(),
            default_driver_options: driver_options,
            required_fields: vec!["name".to_string()],
        };

        let properties = Self::map_properties(device.properties.as_ref());
        let commands = Self::map_commands(device.commands.as_ref());

        Ok(DeviceTemplate {
            id: format!("tpl_{}", uuid::Uuid::new_v4()),
            name,
            display_name: display_name.to_string(),
            description: device.description.clone(),
            version: "1.0.0".to_string(),
            author: None,
            category: "exported".to_string(),
            manufacturer: device.factory_name.clone(),
            device_type: device.device_type.clone().unwrap_or_default(),
            protocol_type: device.protocol_type.clone(),
            driver_name: device.driver_name.clone(),
            tags: "[]".to_string(),
            device_info: serde_json::to_string(&device_info).unwrap_or_default(),
            properties: serde_json::to_string(&properties).unwrap_or_default(),
            commands: serde_json::to_string(&commands).unwrap_or_default(),
            is_builtin: 0,
            is_active: 1,
            created_at: now.clone(),
            updated_at: now,
            workspace_id: device.workspace_id.clone(),
        })
    }

    /// Strip sensitive keys from driver_options JSON recursively.
    fn sanitize_driver_options(options_json: Option<&str>) -> Option<String> {
        let mut value: serde_json::Value = serde_json::from_str(options_json?).ok()?;
        Self::redact_sensitive_values(&mut value);
        serde_json::to_string(&value).ok()
    }

    fn redact_sensitive_values(value: &mut serde_json::Value) {
        let sensitive = [
            "password", "passwd", "secret", "api_key", "token", "auth", "private_key", "key",
            "credential", "cert",
        ];
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    let key_lower = k.to_lowercase();
                    let is_sensitive = sensitive.iter().any(|s| key_lower.contains(s));
                    if is_sensitive && !v.is_object() && !v.is_array() {
                        *v = serde_json::Value::String("__REDACTED__".into());
                    } else {
                        Self::redact_sensitive_values(v);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr.iter_mut() {
                    Self::redact_sensitive_values(item);
                }
            }
            _ => {}
        }
    }

    fn map_properties(
        props: Option<&Vec<tinyiothub_core::models::device_property::DeviceProperty>>,
    ) -> Vec<PropertyTemplate> {
        let Some(props) = props else { return Vec::new() };
        props
            .iter()
            .map(|p| PropertyTemplate {
                name: p.name.clone(),
                display_name: Self::to_localized_map(p.display_name.as_deref()).unwrap_or_default(),
                description: Self::to_localized_map(p.description.as_deref()),
                data_type: p.data_type.clone().unwrap_or_default(),
                unit: p.unit.clone(),
                min_value: p.min_value,
                max_value: p.max_value,
                default_value: p.default_value.clone(),
                is_read_only: p.is_read_only != 0,
                is_required: false,
                validation_rules: None,
            })
            .collect()
    }

    fn map_commands(
        cmds: Option<&Vec<tinyiothub_core::models::device_command::DeviceCommand>>,
    ) -> Vec<CommandTemplate> {
        let Some(cmds) = cmds else { return Vec::new() };
        cmds
            .iter()
            .map(|c| CommandTemplate {
                name: c.name.clone(),
                display_name: Self::to_localized_map(c.display_name.as_deref()).unwrap_or_default(),
                description: Self::to_localized_map(c.description.as_deref()),
                parameters: c.parameters.clone(),
                parameter_schema: None,
                is_required: false,
            })
            .collect()
    }

    fn to_localized_map(text: Option<&str>) -> Option<HashMap<String, String>> {
        let mut map = HashMap::new();
        if let Some(t) = text {
            map.insert("zh".to_string(), t.to_string());
            map.insert("en".to_string(), t.to_string());
            Some(map)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_driver_options_top_level() {
        let input = r#"{"host":"192.168.1.1","password":"secret123","port":502}"#;
        let result = TemplateExporter::sanitize_driver_options(Some(input));
        let result_str = result.unwrap();
        assert!(result_str.contains("192.168.1.1"));
        assert!(result_str.contains("__REDACTED__"));
        assert!(!result_str.contains("secret123"));
    }

    #[test]
    fn test_sanitize_nested_password() {
        let input = r#"{"auth":{"username":"admin","password":"secret123"},"host":"1.2.3.4"}"#;
        let result = TemplateExporter::sanitize_driver_options(Some(input)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["auth"]["password"], "__REDACTED__");
        assert_eq!(parsed["auth"]["username"], "admin");
        assert_eq!(parsed["host"], "1.2.3.4");
    }

    #[test]
    fn test_sanitize_expanded_keys() {
        let input = r#"{"api_key":"ak","private_key":"pk","passwd":"pw","credential":"cred_value","cert":"crt","key":"k"}"#;
        let result = TemplateExporter::sanitize_driver_options(Some(input)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["api_key"], "__REDACTED__");
        assert_eq!(parsed["private_key"], "__REDACTED__");
        assert_eq!(parsed["passwd"], "__REDACTED__");
        assert_eq!(parsed["credential"], "__REDACTED__");
        assert_eq!(parsed["cert"], "__REDACTED__");
        assert_eq!(parsed["key"], "__REDACTED__");
    }

    #[test]
    fn test_sanitize_array_with_secrets() {
        let input = r#"[{"host":"h1","password":"p1"},{"host":"h2","api_key":"k2"}]"#;
        let result = TemplateExporter::sanitize_driver_options(Some(input)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["password"], "__REDACTED__");
        assert_eq!(parsed[1]["api_key"], "__REDACTED__");
    }

    #[test]
    fn test_sanitize_none() {
        assert!(TemplateExporter::sanitize_driver_options(None).is_none());
    }

    #[test]
    fn test_map_properties() {
        let props = vec![tinyiothub_core::models::device_property::DeviceProperty {
            id: "p1".to_string(),
            device_id: "d1".to_string(),
            name: "temperature".to_string(),
            display_name: Some("温度".to_string()),
            description: Some("当前温度".to_string()),
            data_type: Some("float".to_string()),
            unit: Some("°C".to_string()),
            min_value: Some(-40.0),
            max_value: Some(80.0),
            default_value: Some("25.0".to_string()),
            is_read_only: 1,
            created_at: None,
            updated_at: None,
            current_value: None,
            alarm_status: None,
        }];
        let result = TemplateExporter::map_properties(Some(&props));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "temperature");
        assert_eq!(result[0].data_type, "float");
        assert_eq!(result[0].unit, Some("°C".to_string()));
        assert_eq!(result[0].is_read_only, true);
        assert_eq!(result[0].display_name.get("zh"), Some(&"温度".to_string()));
    }

    #[test]
    fn test_map_properties_none() {
        assert!(TemplateExporter::map_properties(None).is_empty());
    }

    #[test]
    fn test_map_commands() {
        let cmds = vec![tinyiothub_core::models::device_command::DeviceCommand {
            id: "c1".to_string(),
            device_id: "d1".to_string(),
            name: "reboot".to_string(),
            display_name: Some("重启".to_string()),
            description: Some("重启设备".to_string()),
            parameters: Some(r#"{"delay":{"type":"integer"}}"#.to_string()),
            created_at: "2024-01-01".to_string(),
        }];
        let result = TemplateExporter::map_commands(Some(&cmds));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "reboot");
        assert_eq!(result[0].parameters, Some(r#"{"delay":{"type":"integer"}}"#.to_string()));
        assert_eq!(result[0].display_name.get("zh"), Some(&"重启".to_string()));
    }

    #[test]
    fn test_map_commands_none() {
        assert!(TemplateExporter::map_commands(None).is_empty());
    }
}

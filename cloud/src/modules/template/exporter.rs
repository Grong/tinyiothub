// cloud/src/modules/template/exporter.rs

use tinyiothub_core::models::device::Device;

use super::types::{DeviceInfo, DeviceTemplate};

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

        let properties: Vec<super::types::PropertyTemplate> = Vec::new();
        let commands: Vec<super::types::CommandTemplate> = Vec::new();

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

    /// Strip sensitive keys from driver_options JSON.
    fn sanitize_driver_options(options_json: Option<&str>) -> Option<String> {
        let mut value: serde_json::Value = serde_json::from_str(options_json?).ok()?;
        let sensitive = ["password", "secret", "api_key", "token", "auth", "private_key"];
        if let serde_json::Value::Object(ref mut map) = value {
            for key in sensitive {
                if map.contains_key(key) {
                    map.insert(key.to_string(), serde_json::Value::String("__REDACTED__".into()));
                }
            }
        }
        serde_json::to_string(&value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_driver_options() {
        let input = r#"{"host":"192.168.1.1","password":"secret123","port":502}"#;
        let result = TemplateExporter::sanitize_driver_options(Some(input));
        let result_str = result.unwrap();
        assert!(result_str.contains("192.168.1.1"));
        assert!(result_str.contains("__REDACTED__"));
        assert!(!result_str.contains("secret123"));
    }

    #[test]
    fn test_sanitize_none() {
        assert!(TemplateExporter::sanitize_driver_options(None).is_none());
    }
}

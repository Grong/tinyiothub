//! Entity Unit Tests
//! 实体单元测试

use serde_json::json;

// ==================== Device Tests ====================

#[cfg(test)]
mod device_tests {
    use super::*;

    #[test]
    fn test_device_query_params_default() {
        let params = crate::dto::entity::device::DeviceQueryParams::default();
        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 20);
    }

    #[test]
    fn test_device_create_request() {
        let req = crate::dto::entity::device::CreateDeviceRequest {
            name: "Test Device".to_string(),
            device_type: Some("sensor".to_string()),
            driver_name: Some("mqtt".to_string()),
            description: Some("Test description".to_string()),
            metadata: None,
            tags: None,
            parent_id: None,
        };
        
        assert_eq!(req.name, "Test Device");
        assert_eq!(req.device_type, Some("sensor".to_string()));
    }

    #[test]
    fn test_device_update_request() {
        let req = crate::dto::entity::device::UpdateDeviceRequest {
            name: Some("Updated Device".to_string()),
            display_name: Some("Display Name".to_string()),
            description: Some("New description".to_string()),
            state: Some(1),
            driver_options: None,
        };
        
        assert_eq!(req.name, Some("Updated Device".to_string()));
    }
}

// ==================== User Tests ====================

#[cfg(test)]
mod user_tests {
    use super::*;

    #[test]
    fn test_user_create_request() {
        let req = crate::dto::entity::user::CreateUserRequest {
            username: "testuser".to_string(),
            email: Some("test@example.com".to_string()),
            phone: Some("13800138000".to_string()),
            password: "password123".to_string(),
            role_ids: None,
            organization_id: None,
        };
        
        assert_eq!(req.username, "testuser");
        assert_eq!(req.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_user_login_request() {
        let req = crate::dto::entity::user::LoginRequest {
            username: "testuser".to_string(),
            password: "password123".to_string(),
            captcha: None,
            captcha_id: None,
        };
        
        assert_eq!(req.username, "testuser");
    }
}

// ==================== Alarm Tests ====================

#[cfg(test)]
mod alarm_tests {
    use super::*;

    #[test]
    fn test_device_alarm() {
        let alarm = crate::dto::entity::device_alarm::DeviceAlarm {
            id: "alarm-001".to_string(),
            device_id: "device-001".to_string(),
            alarm_type: "threshold".to_string(),
            level: 2,
            message: "Temperature too high".to_string(),
            value: Some("50".to_string()),
            threshold: Some("40".to_string()),
            acknowledged: false,
            acknowledged_at: None,
            acknowledged_by: None,
            resolved: false,
            resolved_at: None,
            resolved_by: None,
            created_at: "2026-03-16 12:00:00".to_string(),
        };
        
        assert_eq!(alarm.id, "alarm-001");
        assert_eq!(alarm.level, 2);
        assert!(!alarm.acknowledged);
    }

    #[test]
    fn test_create_alarm_rule_request() {
        let req = crate::dto::entity::device_alarm_rule::CreateAlarmRuleRequest {
            name: "High Temperature Alert".to_string(),
            device_id: "device-001".to_string(),
            alarm_type: "threshold".to_string(),
            property_name: "temperature".to_string(),
            operator: ">".to_string(),
            threshold: "40".to_string(),
            level: 2,
            message: Some("Temperature too high".to_string()),
            enabled: true,
            notification_channels: Some(vec!["email".to_string()]),
        };
        
        assert_eq!(req.name, "High Temperature Alert");
        assert_eq!(req.threshold, "40");
        assert!(req.enabled);
    }
}

// ==================== Template Tests ====================

#[cfg(test)]
mod template_tests {
    use super::*;

    #[test]
    fn test_device_template() {
        let template = json!({
            "id": "tmpl-001",
            "name": "Temperature Sensor",
            "category": "sensor",
            "properties": [
                {"name": "temperature", "type": "number", "unit": "℃"}
            ]
        });
        
        assert_eq!(template["id"], "tmpl-001");
        assert_eq!(template["name"], "Temperature Sensor");
    }
}

// ==================== Tag Tests ====================

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn test_tag() {
        let tag = crate::dto::entity::tag::Tag {
            id: "tag-001".to_string(),
            name: "important".to_string(),
            tag_type: Some("custom".to_string()),
            color: Some("#FF0000".to_string()),
            description: Some("Important devices".to_string()),
            created_at: "2026-03-16 12:00:00".to_string(),
        };
        
        assert_eq!(tag.name, "important");
        assert_eq!(tag.color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_tag_binding() {
        let binding = crate::dto::entity::tag::TagBinding {
            id: "binding-001".to_string(),
            tag_id: "tag-001".to_string(),
            target_id: "device-001".to_string(),
            target_type: "device".to_string(),
            created_at: "2026-03-16 12:00:00".to_string(),
        };
        
        assert_eq!(binding.target_id, "device-001");
        assert_eq!(binding.target_type, "device");
    }
}

// ==================== Role Tests ====================

#[cfg(test)]
mod role_tests {
    use super::*;

    #[test]
    fn test_role() {
        let role = crate::dto::entity::role::Role {
            id: "role-001".to_string(),
            name: "admin".to_string(),
            description: Some("Administrator role".to_string()),
            is_system: true,
            created_at: "2026-03-16 12:00:00".to_string(),
            updated_at: "2026-03-16 12:00:00".to_string(),
        };
        
        assert_eq!(role.name, "admin");
        assert!(role.is_system);
    }

    #[test]
    fn test_permission() {
        let perm = crate::dto::entity::permission::Permission {
            id: "perm-001".to_string(),
            name: "device:read".to_string(),
            resource: "device".to_string(),
            action: "read".to_string(),
            description: Some("Read device permission".to_string()),
            created_at: "2026-03-16 12:00:00".to_string(),
        };
        
        assert_eq!(perm.resource, "device");
        assert_eq!(perm.action, "read");
    }
}

// ==================== Driver Tests ====================

#[cfg(test)]
mod driver_tests {
    use super::*;

    #[test]
    fn test_driver_info() {
        let info = crate::domain::device::driver::DriverInfo {
            name: "mqtt".to_string(),
            display_name: "MQTT Driver".to_string(),
            description: Some("MQTT protocol driver".to_string()),
            version: "1.0.0".to_string(),
            author: Some("TinyIoTHub".to_string()),
            license: Some("MIT".to_string()),
        };
        
        assert_eq!(info.name, "mqtt");
        assert_eq!(info.version, "1.0.0");
    }

    #[test]
    fn test_driver_config() {
        let config = crate::domain::device::driver::DriverConfig {
            name: "mqtt".to_string(),
            enabled: true,
            options: json!({
                "host": "localhost",
                "port": 1883
            }),
        };
        
        assert!(config.enabled);
    }
}

// ==================== Event Tests ====================

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn test_event_level() {
        use crate::domain::event::value_objects::EventLevel;
        
        assert_eq!(EventLevel::Debug.as_str(), "debug");
        assert_eq!(EventLevel::Info.as_str(), "info");
        assert_eq!(EventLevel::Warning.as_str(), "warning");
        assert_eq!(EventLevel::Error.as_str(), "error");
        assert_eq!(EventLevel::Critical.as_str(), "critical");
    }

    #[test]
    fn test_event_source() {
        use crate::domain::event::value_objects::EventSource;
        
        let source = EventSource {
            source_type: "device".to_string(),
            source_id: "device-001".to_string(),
            source_name: Some("Test Device".to_string()),
        };
        
        assert_eq!(source.source_type, "device");
        assert_eq!(source.source_id, "device-001");
    }

    #[test]
    fn test_event_type() {
        use crate::domain::event::value_objects::EventType;
        
        let event_type = EventType {
            category: "alarm".to_string(),
            name: "threshold_exceeded".to_string(),
        };
        
        assert_eq!(event_type.category, "alarm");
    }
}

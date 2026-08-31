//! 驱动SDK基础类型定义

use serde::{Deserialize, Serialize};

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thing {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_options: Option<String>,
    pub address: Option<String>,
    pub enabled: bool,
}

/// 设备命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingCommand {
    pub id: String,
    pub name: String,
    pub command_type: String,
    pub parameters: Option<String>,
}

/// 读取结果值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultValue {
    pub name: String,
    pub value_type: String,
    pub value: Option<String>,
}

impl ResultValue {
    pub fn new(name: String, value_type: String, value: Option<String>) -> Self {
        Self {
            name,
            value_type,
            value,
        }
    }

    pub fn integer(name: String, value: i64) -> Self {
        Self::new(name, "int".to_string(), Some(value.to_string()))
    }

    pub fn float(name: String, value: f64) -> Self {
        Self::new(name, "float".to_string(), Some(value.to_string()))
    }

    pub fn float_with_precision(name: String, value: f64, decimal_places: u32) -> Self {
        let multiplier = 10_f64.powi(decimal_places as i32);
        let rounded = (value * multiplier).round() / multiplier;
        Self::new(
            name,
            "float".to_string(),
            Some(format!("{:.precision$}", rounded, precision = decimal_places as usize)),
        )
    }

    pub fn string(name: String, value: String) -> Self {
        Self::new(name, "string".to_string(), Some(value))
    }

    pub fn boolean(name: String, value: bool) -> Self {
        Self::new(name, "boolean".to_string(), Some(value.to_string()))
    }

    pub fn enum_value(name: String, value: String) -> Self {
        Self::new(name, "enum".to_string(), Some(value))
    }
}

/// 组件选项（驱动配置项）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentOption {
    pub label: String,
    pub name: String,
    pub default_value: String,
    pub option_type: String,
    pub required: bool,
}

impl ComponentOption {
    pub fn new(label: String, name: String, default_value: String, option_type: String, required: bool) -> Self {
        Self {
            label,
            name,
            default_value,
            option_type,
            required,
        }
    }
}

/// 创建组件请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateComponentRequest {
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub thing_num: Option<u32>,
    pub description: Option<String>,
    pub options_descriptors: Vec<ComponentOption>,
    pub location: Option<String>,
}

/// 组件信息（驱动元数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub thing_num: u32,
    pub description: Option<String>,
    pub options_descriptors: Vec<ComponentOption>,
    pub location: Option<String>,
}

impl ComponentInfo {
    /// 从创建请求构造组件信息
    pub fn new(request: CreateComponentRequest) -> Self {
        Self {
            name: request.name,
            version: request.version,
            class_name: request.class_name,
            thing_num: request.thing_num.unwrap_or(0),
            description: request.description,
            options_descriptors: request.options_descriptors,
            location: request.location,
        }
    }
}

// === Core interop conversions ===

#[cfg(feature = "core-interop")]
mod core_interop {
    use super::*;

    impl From<tinyiothub_core::models::thing::Thing> for Thing {
        fn from(core: tinyiothub_core::models::thing::Thing) -> Self {
            let enabled = core.is_online();
            Thing {
                id: core.id,
                name: core.name,
                display_name: core.display_name,
                protocol_type: core.protocol_type,
                driver_options: core.driver_options,
                address: core.address,
                enabled,
            }
        }
    }

    impl From<Thing> for tinyiothub_core::models::thing::Thing {
        fn from(sdk: Thing) -> Self {
            tinyiothub_core::models::thing::Thing {
                id: sdk.id,
                name: sdk.name,
                display_name: sdk.display_name,
                category: None,
                address: sdk.address,
                description: None,
                position: None,
                driver_name: None,
                device_model: None,
                protocol_type: sdk.protocol_type,
                factory_name: None,
                linked_data: None,
                driver_options: sdk.driver_options,
                status: if sdk.enabled {
                    tinyiothub_core::models::thing::ThingStatus::Online
                } else {
                    tinyiothub_core::models::thing::ThingStatus::Offline
                },
                parent_id: None,
                template_id: None,
                workspace_id: None,
                linked_gateway: None,
                fingerprint: None,
                created_at: None,
                updated_at: None,
                tags: None,
                properties: None,
                commands: None,
                last_heartbeat: None,
            }
        }
    }

    impl From<tinyiothub_core::models::thing_command::ThingCommand> for ThingCommand {
        fn from(core: tinyiothub_core::models::thing_command::ThingCommand) -> Self {
            ThingCommand {
                id: core.id,
                name: core.name,
                command_type: String::new(),
                parameters: core.parameters,
            }
        }
    }

    impl From<ThingCommand> for tinyiothub_core::models::thing_command::ThingCommand {
        fn from(sdk: ThingCommand) -> Self {
            tinyiothub_core::models::thing_command::ThingCommand {
                id: sdk.id,
                thing_id: String::new(),
                name: sdk.name,
                display_name: None,
                description: None,
                parameters: sdk.parameters,
                created_at: String::new(),
            }
        }
    }
}

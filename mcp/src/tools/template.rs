//! 模板相关 MCP Tools 定义

use crate::tools::ToolMeta;

/// list_templates - 获取模板列表
pub fn list_templates() -> ToolMeta {
    ToolMeta {
        name: "list_templates".to_string(),
        description: "获取系统中所有设备模板，支持分页和过滤".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "page": {
                    "type": "integer",
                    "description": "页码，从 1 开始",
                    "default": 1
                },
                "page_size": {
                    "type": "integer",
                    "description": "每页数量",
                    "default": 20,
                    "maximum": 100
                },
                "name": {
                    "type": "string",
                    "description": "按模板名称模糊搜索"
                },
                "device_type": {
                    "type": "string",
                    "description": "按设备类型过滤（如 sensor, actuator, gateway）"
                },
                "driver_name": {
                    "type": "string",
                    "description": "按关联驱动名称过滤"
                },
                "include_properties": {
                    "type": "boolean",
                    "description": "是否包含模板属性定义",
                    "default": false
                }
            }
        }),
    }
}

/// get_template - 获取模板详情
pub fn get_template() -> ToolMeta {
    ToolMeta {
        name: "get_template".to_string(),
        description: "获取单个设备模板的完整详细信息，包括属性定义和配置".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "模板唯一标识（UUID）或名称"
                },
                "include_properties": {
                    "type": "boolean",
                    "description": "是否包含属性定义",
                    "default": true
                },
                "include_commands": {
                    "type": "boolean",
                    "description": "是否包含命令定义",
                    "default": true
                }
            },
            "required": ["id"]
        }),
    }
}

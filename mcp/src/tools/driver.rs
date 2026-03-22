//! 驱动相关 MCP Tools 定义

use crate::tools::ToolMeta;

/// list_drivers - 获取驱动列表
pub fn list_drivers() -> ToolMeta {
    ToolMeta {
        name: "list_drivers".to_string(),
        description: "获取系统中所有已注册的驱动列表".to_string(),
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
                    "description": "按驱动名称模糊搜索"
                },
                "protocol": {
                    "type": "string",
                    "description": "按协议类型过滤（如 modbus_tcp, onvif, snmp, opcua）"
                },
                "state": {
                    "type": "integer",
                    "description": "按状态过滤：0=禁用, 1=启用"
                }
            }
        }),
    }
}

/// get_driver_info - 获取驱动详情
pub fn get_driver_info() -> ToolMeta {
    ToolMeta {
        name: "get_driver_info".to_string(),
        description: "获取单个驱动的完整详细信息".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "驱动名称（如 modbus_tcp, onvif, snmp）"
                },
                "include_capabilities": {
                    "type": "boolean",
                    "description": "是否包含驱动能力描述",
                    "default": true
                },
                "include_statistics": {
                    "type": "boolean",
                    "description": "是否包含运行时统计信息",
                    "default": false
                }
            },
            "required": ["name"]
        }),
    }
}

//! 设备相关 MCP Tools 定义

use crate::tools::ToolMeta;

/// list_devices - 获取设备列表
pub fn list_devices() -> ToolMeta {
    ToolMeta {
        name: "list_devices".to_string(),
        description: "列出所有 IoT 设备，支持分页和过滤".to_string(),
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
                    "description": "按设备名称模糊搜索"
                },
                "device_type": {
                    "type": "string",
                    "description": "按设备类型过滤（如 sensor, actuator, gateway）"
                },
                "driver_name": {
                    "type": "string",
                    "description": "按驱动名称过滤（如 modbus_tcp, onvif, snmp）"
                },
                "state": {
                    "type": "integer",
                    "description": "按状态过滤：0=离线, 1=在线"
                },
                "include_properties": {
                    "type": "boolean",
                    "description": "是否包含实时属性数据",
                    "default": false
                }
            }
        }),
    }
}

/// get_device - 获取设备详情
pub fn get_device() -> ToolMeta {
    ToolMeta {
        name: "get_device".to_string(),
        description: "获取单个设备的完整详细信息".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "设备唯一标识（UUID）或名称"
                },
                "include_properties": {
                    "type": "boolean",
                    "description": "是否包含实时属性",
                    "default": true
                }
            },
            "required": ["device_id"]
        }),
    }
}

/// get_device_status - 获取设备状态
pub fn get_device_status() -> ToolMeta {
    ToolMeta {
        name: "get_device_status".to_string(),
        description: "快速获取设备的在线状态".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "设备唯一标识（UUID）或名称"
                }
            },
            "required": ["device_id"]
        }),
    }
}

/// read_sensor_data - 读取传感器数据
pub fn read_sensor_data() -> ToolMeta {
    ToolMeta {
        name: "read_sensor_data".to_string(),
        description: "读取传感器的实时数据".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "设备唯一标识"
                },
                "properties": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "要读取的属性名称列表，如 [\"temperature\", \"humidity\"]"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "读取超时时间（毫秒）",
                    "default": 5000,
                    "minimum": 1000,
                    "maximum": 30000
                }
            },
            "required": ["device_id"]
        }),
    }
}

/// send_command - 发送控制命令
pub fn send_command() -> ToolMeta {
    ToolMeta {
        name: "send_command".to_string(),
        description: "向设备发送控制命令".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "设备唯一标识"
                },
                "command": {
                    "type": "string",
                    "description": "命令名称（如 reboot, set_value, toggle）"
                },
                "parameters": {
                    "type": "object",
                    "description": "命令参数字典"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "命令超时时间（毫秒）",
                    "default": 10000
                }
            },
            "required": ["device_id", "command"]
        }),
    }
}

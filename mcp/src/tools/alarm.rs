//! 告警相关 MCP Tools 定义

use crate::tools::ToolMeta;

/// list_alarms - 获取告警列表
pub fn list_alarms() -> ToolMeta {
    ToolMeta {
        name: "list_alarms".to_string(),
        description: "列出告警事件".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "acknowledged", "resolved", "all"],
                    "description": "告警状态",
                    "default": "active"
                },
                "device_id": {
                    "type": "string",
                    "description": "按设备过滤"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回数量限制",
                    "default": 20,
                    "maximum": 100
                }
            }
        }),
    }
}

/// acknowledge_alarm - 确认告警
pub fn acknowledge_alarm() -> ToolMeta {
    ToolMeta {
        name: "acknowledge_alarm".to_string(),
        description: "确认告警".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "alarm_id": {
                    "type": "string",
                    "description": "告警唯一标识"
                },
                "comment": {
                    "type": "string",
                    "description": "处理备注"
                }
            },
            "required": ["alarm_id"]
        }),
    }
}

/// get_alarm_statistics - 获取告警统计
pub fn get_alarm_statistics() -> ToolMeta {
    ToolMeta {
        name: "get_alarm_statistics".to_string(),
        description: "获取告警统计信息".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "time_range": {
                    "type": "string",
                    "enum": ["today", "week", "month", "all"],
                    "description": "时间范围",
                    "default": "today"
                }
            }
        }),
    }
}

//! MCP Tools 定义
//! 
//! 定义所有可用的 MCP 工具及其元数据

pub mod device;
pub mod alarm;

// ==================== 工具定义 ====================

/// MCP 工具元数据（用于 tools/list 响应）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// 获取所有工具元数据
pub fn get_all_tools() -> Vec<ToolMeta> {
    vec![
        device::list_devices(),
        device::get_device(),
        device::get_device_status(),
        device::read_sensor_data(),
        device::send_command(),
        alarm::list_alarms(),
        alarm::acknowledge_alarm(),
        alarm::get_alarm_statistics(),
    ]
}

/// 获取所有工具 JSON（用于 MCP 协议）
pub fn get_all_tools_json() -> Vec<serde_json::Value> {
    get_all_tools()
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema
            })
        })
        .collect()
}

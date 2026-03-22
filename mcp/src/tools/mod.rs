//! MCP Tools 定义
//! 
//! 定义所有可用的 MCP 工具及其元数据

pub mod device;
pub mod alarm;
pub mod driver;
pub mod template;
pub mod user;

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
        // 设备相关
        device::list_devices(),
        device::get_device(),
        device::get_device_status(),
        device::read_sensor_data(),
        device::send_command(),
        device::get_device_history(),
        device::get_device_latest(),
        // 告警相关
        alarm::list_alarms(),
        alarm::acknowledge_alarm(),
        alarm::get_alarm_statistics(),
        // 驱动相关
        driver::list_drivers(),
        driver::get_driver_info(),
        // 模板相关
        template::list_templates(),
        template::get_template(),
        // 用户相关
        user::get_current_user(),
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

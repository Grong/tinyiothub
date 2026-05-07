use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 设备模板实体 - 使用 snake_case 数据库字段
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceTemplate {
    pub id: String,
    pub name: String,
    pub display_name: String,        // JSON格式的多语言显示名称
    pub description: Option<String>, // JSON格式的多语言描述
    pub version: String,
    pub author: Option<String>,
    pub category: String,
    pub manufacturer: Option<String>,
    pub device_type: String,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: String,        // JSON数组格式
    pub device_info: String, // JSON格式的DeviceInfo
    pub properties: String,  // JSON数组格式的PropertyTemplate
    pub commands: String,    // JSON数组格式的CommandTemplate
    pub is_builtin: i32,     // 是否为内置模板
    pub is_active: i32,      // 是否激活
    pub created_at: String,
    pub updated_at: String,
}

/// 设备信息模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceInfo {
    pub default_name_pattern: String, // 例如: "{manufacturer}_{device_type}_{index}"
    pub default_display_name_pattern: Option<String>,
    pub default_description: Option<HashMap<String, String>>,
    pub default_position: Option<String>,
    pub default_driver_options: Option<String>,
    pub required_fields: Vec<String>, // 用户必须填写的字段
}

/// 属性模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PropertyTemplate {
    pub name: String,
    pub display_name: HashMap<String, String>,
    pub description: Option<HashMap<String, String>>,
    pub data_type: String,
    pub unit: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<String>,
    pub is_read_only: bool,
    pub is_required: bool,
    pub validation_rules: Option<String>, // JSON格式的验证规则
}

/// 命令模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandTemplate {
    pub name: String,
    pub display_name: HashMap<String, String>,
    pub description: Option<HashMap<String, String>>,
    pub parameters: Option<String>,       // JSON格式的参数定义
    pub parameter_schema: Option<String>, // JSON Schema格式的参数验证
    pub is_required: bool,
}

/// 设备模板查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TemplateQueryParams {
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub keyword: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 模板分类
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemplateCategory {
    pub name: String,
    pub display_name: String,        // JSON格式的多语言显示名称
    pub description: Option<String>, // JSON格式的多语言描述
    pub sort_order: i32,
    pub is_active: i32,
    pub created_at: String,
    /// 模板数量 (不存储在数据库中，通过关联查询获取)
    pub template_count: i64,
}

/// 创建设备模板请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceTemplateRequest {
    pub name: String,
    pub display_name: HashMap<String, String>,
    pub description: Option<HashMap<String, String>>,
    pub version: String,
    pub author: Option<String>,
    pub category: String,
    pub manufacturer: Option<String>,
    pub device_type: String,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: Vec<String>,
    pub device_info: DeviceInfo,
    pub properties: Vec<PropertyTemplate>,
    pub commands: Vec<CommandTemplate>,
}

/// 更新设备模板请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceTemplateRequest {
    pub name: Option<String>,
    pub display_name: Option<HashMap<String, String>>,
    pub description: Option<HashMap<String, String>>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub device_info: Option<DeviceInfo>,
    pub properties: Option<Vec<PropertyTemplate>>,
    pub commands: Option<Vec<CommandTemplate>>,
}

/// 设备创建输入
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceCreationInput {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub property_values: HashMap<String, String>, // 属性默认值覆盖
    pub enabled_commands: Vec<String>,            // 用户选择启用的命令
}

/// 设备预览
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevicePreview {
    pub device_info: crate::models::device::CreateDeviceRequest,
    pub properties: Vec<crate::models::device_property::CreateDevicePropertyRequest>,
    pub commands: Vec<crate::models::device_command::CreateDeviceCommandRequest>,
    pub warnings: Vec<String>,
}

/// 基于模板创建设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceFromTemplateRequest {
    pub template_id: String,
    pub device_input: DeviceCreationInput,
}

impl Default for DeviceTemplate {
    fn default() -> Self {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            display_name: "{}".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            author: None,
            category: String::new(),
            manufacturer: None,
            device_type: String::new(),
            protocol_type: None,
            driver_name: None,
            tags: "[]".to_string(),
            device_info: "{}".to_string(),
            properties: "[]".to_string(),
            commands: "[]".to_string(),
            is_builtin: 0,
            is_active: 1,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
/// 模板需求信息 (用于设备创建向导)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemplateRequirements {
    pub template_id: String,
    pub template_name: String,
    pub display_name: String,
    pub required_fields: Vec<String>,
    pub available_properties: Vec<PropertyInfo>,
    pub available_commands: Vec<CommandInfo>,
}

/// 属性信息 (用于向导)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PropertyInfo {
    pub name: String,
    pub display_name: String,
    pub data_type: String,
    pub is_required: bool,
    pub default_value: Option<String>,
    pub validation_rules: Option<String>,
}

/// 命令信息 (用于向导)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandInfo {
    pub name: String,
    pub display_name: String,
    pub is_required: bool,
    pub parameters: Option<String>,
}

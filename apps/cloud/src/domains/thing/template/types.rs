// Template types — DTOs and request/response types.
//
// DB 行类型与模板请求类型（ThingTemplate/ThingInfo/PropertyTemplate/
// CommandTemplate/TemplateQueryParams/TemplateCategory/
// CreateThingTemplateRequest/UpdateThingTemplateRequest/TemplateFilters）
// 已迁入 tinyiothub_storage::thing_template（Task 12），此处 re-export
// 保持既有路径。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use tinyiothub_storage::thing_template::{
    CommandTemplate, CreateThingTemplateRequest, PropertyTemplate, TemplateCategory, TemplateFilters,
    TemplateQueryParams, ThingInfo, ThingTemplate, UpdateThingTemplateRequest,
};

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
    pub template_id: Option<String>,
    pub property_values: HashMap<String, String>, // 属性默认值覆盖
    pub enabled_commands: Vec<String>,            // 用户选择启用的命令
    pub tenant_id: Option<String>,                // Will be set from claims, not from request
    pub workspace_id: Option<String>,             // Will be set from X-Workspace-Id header
}

/// 设备预览
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevicePreview {
    pub device_info: tinyiothub_core::models::thing::CreateThingRequest,
    pub properties: Vec<tinyiothub_core::models::thing_property::CreateThingPropertyRequest>,
    pub commands: Vec<tinyiothub_core::models::thing_command::CreateThingCommandRequest>,
    pub warnings: Vec<String>,
}

/// 基于模板创建设备请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateThingFromTemplateRequest {
    pub template_id: String,
    pub device_input: DeviceCreationInput,
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

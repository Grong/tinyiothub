use std::collections::{HashMap, HashSet};

use tracing::{debug, info, warn};

use crate::dto::entity::{
    device_template::{
        CommandTemplate, CreateDeviceTemplateRequest, DeviceCreationInput, DeviceTemplate,
        PropertyTemplate,
    },
    template_error::{ValidationError, ValidationResult},
};

/// 模板验证器 - 负责验证模板格式和内容的正确性
#[derive(Debug)]
pub struct TemplateValidator;

impl TemplateValidator {
    /// 创建新的模板验证器实例
    pub fn new() -> Self {
        Self
    }

    /// 验证设备模板 (需求 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7)
    pub fn validate_template(&self, template: &DeviceTemplate) -> ValidationResult {
        info!("验证设备模板: {}", template.name);

        let mut result = ValidationResult::success();

        // 需求 6.2: 检查必需字段是否存在
        self.validate_required_fields(template, &mut result);

        // 需求 6.1: 验证JSON格式的正确性
        self.validate_json_fields(template, &mut result);

        // 需求 6.3: 验证属性模板的数据类型和范围
        self.validate_property_templates_internal(template, &mut result);

        // 需求 6.4: 验证命令模板的参数定义完整性
        self.validate_command_templates_internal(template, &mut result);

        // 需求 6.6: 检查重复的属性或命令名称
        self.validate_unique_names(template, &mut result);

        // 需求 6.5: 验证驱动引用
        self.validate_driver_reference(template, &mut result);

        debug!("模板验证完成，错误数: {}, 警告数: {}", result.errors.len(), result.warnings.len());

        // 需求 6.7: 如果验证失败，记录详细错误日志
        if result.has_errors() {
            warn!("模板验证失败: {}, 错误: {:?}", template.name, result.errors);
        }

        result
    }

    /// 验证JSON格式 (需求 6.1)
    pub fn validate_json_format(
        &self,
        json: &str,
    ) -> Result<CreateDeviceTemplateRequest, ValidationError> {
        serde_json::from_str(json).map_err(|e| {
            ValidationError::new("json", &format!("JSON格式错误: {}", e), "INVALID_JSON")
        })
    }

    /// 验证属性模板 (需求 6.3)
    pub fn validate_property_templates(&self, properties: &[PropertyTemplate]) -> ValidationResult {
        let mut result = ValidationResult::success();

        for (index, property) in properties.iter().enumerate() {
            let field_prefix = format!("properties[{}]", index);

            // 验证属性名称
            if property.name.trim().is_empty() {
                result.add_error(
                    &format!("{}.name", field_prefix),
                    "属性名称不能为空",
                    "REQUIRED_FIELD",
                );
            }

            // 验证数据类型
            if !self.is_valid_data_type(&property.data_type) {
                result.add_error(
                    &format!("{}.data_type", field_prefix),
                    &format!("无效的数据类型: {}", property.data_type),
                    "INVALID_DATA_TYPE",
                );
            }

            // 验证数值范围
            if property.data_type == "number"
                && let (Some(min), Some(max)) = (property.min_value, property.max_value)
                    && min >= max {
                        result.add_error(
                            &format!("{}.range", field_prefix),
                            "最小值必须小于最大值",
                            "INVALID_RANGE",
                        );
                    }

            // 验证默认值与数据类型的匹配
            if let Some(default_value) = &property.default_value
                && !self.validate_value_type(default_value, &property.data_type) {
                    result.add_warning(
                        &format!("{}.default_value", field_prefix),
                        &format!(
                            "默认值 '{}' 与数据类型 '{}' 不匹配",
                            default_value, property.data_type
                        ),
                        "TYPE_MISMATCH",
                    );
                }

            // 验证多语言显示名称
            if property.display_name.is_empty() {
                result.add_error(
                    &format!("{}.display_name", field_prefix),
                    "显示名称不能为空",
                    "REQUIRED_FIELD",
                );
            } else if !property.display_name.contains_key("zh") {
                result.add_warning(
                    &format!("{}.display_name", field_prefix),
                    "建议提供中文显示名称",
                    "MISSING_DEFAULT_LANGUAGE",
                );
            }
        }

        result
    }

    /// 验证命令模板 (需求 6.4)
    pub fn validate_command_templates(&self, commands: &[CommandTemplate]) -> ValidationResult {
        let mut result = ValidationResult::success();

        for (index, command) in commands.iter().enumerate() {
            let field_prefix = format!("commands[{}]", index);

            // 验证命令名称
            if command.name.trim().is_empty() {
                result.add_error(
                    &format!("{}.name", field_prefix),
                    "命令名称不能为空",
                    "REQUIRED_FIELD",
                );
            }

            // 验证多语言显示名称
            if command.display_name.is_empty() {
                result.add_error(
                    &format!("{}.display_name", field_prefix),
                    "显示名称不能为空",
                    "REQUIRED_FIELD",
                );
            } else if !command.display_name.contains_key("zh") {
                result.add_warning(
                    &format!("{}.display_name", field_prefix),
                    "建议提供中文显示名称",
                    "MISSING_DEFAULT_LANGUAGE",
                );
            }

            // 验证参数定义JSON格式
            if let Some(parameters) = &command.parameters
                && !parameters.trim().is_empty()
                    && let Err(e) = serde_json::from_str::<serde_json::Value>(parameters) {
                        result.add_error(
                            &format!("{}.parameters", field_prefix),
                            &format!("参数定义JSON格式错误: {}", e),
                            "INVALID_JSON",
                        );
                    }

            // 验证参数Schema格式
            if let Some(schema) = &command.parameter_schema
                && !schema.trim().is_empty()
                    && let Err(e) = serde_json::from_str::<serde_json::Value>(schema) {
                        result.add_error(
                            &format!("{}.parameter_schema", field_prefix),
                            &format!("参数Schema JSON格式错误: {}", e),
                            "INVALID_JSON",
                        );
                    }
        }

        result
    }

    /// 验证用户输入
    pub fn validate_user_input(
        &self,
        template: &DeviceTemplate,
        input: &DeviceCreationInput,
    ) -> ValidationResult {
        info!("验证用户输入，模板: {}", template.name);

        let mut result = ValidationResult::success();

        // 验证必填字段
        if input.name.trim().is_empty() {
            result.add_error("name", "设备名称不能为空", "REQUIRED_FIELD");
        }

        // 验证设备名称长度
        if input.name.len() > 100 {
            result.add_error("name", "设备名称长度不能超过100个字符", "FIELD_TOO_LONG");
        }

        // 验证模板要求的必填字段
        if let Ok(device_info) = template.get_device_info() {
            for required_field in &device_info.required_fields {
                match required_field.as_str() {
                    "driver_options"
                        if input.driver_options.as_ref().is_none_or(|opt| opt.trim().is_empty()) => {
                            result.add_error(
                                "driver_options",
                                "驱动选项是必填字段",
                                "REQUIRED_FIELD",
                            );
                        }
                    "parent_id"
                        if input.parent_id.as_ref().is_none_or(|id| id.trim().is_empty()) => {
                            result.add_error("parent_id", "父设备ID是必填字段", "REQUIRED_FIELD");
                        }
                    "product_id"
                        if input.product_id.as_ref().is_none_or(|id| id.trim().is_empty()) => {
                            result.add_error("product_id", "产品ID是必填字段", "REQUIRED_FIELD");
                        }
                    _ => {
                        // 其他自定义必填字段的验证可以在这里扩展
                    }
                }
            }
        }

        // 验证属性值覆盖
        if let Ok(properties) = template.get_properties() {
            for (prop_name, prop_value) in &input.property_values {
                if let Some(property) = properties.iter().find(|p| p.name == *prop_name) {
                    if !self.validate_value_type(prop_value, &property.data_type) {
                        result.add_error(
                            &format!("property_values.{}", prop_name),
                            &format!(
                                "属性值 '{}' 与数据类型 '{}' 不匹配",
                                prop_value, property.data_type
                            ),
                            "TYPE_MISMATCH",
                        );
                    }

                    // 验证数值范围
                    if property.data_type == "number"
                        && let Ok(value) = prop_value.parse::<f64>() {
                            if let Some(min) = property.min_value
                                && value < min {
                                    result.add_error(
                                        &format!("property_values.{}", prop_name),
                                        &format!("属性值 {} 小于最小值 {}", value, min),
                                        "VALUE_TOO_SMALL",
                                    );
                                }
                            if let Some(max) = property.max_value
                                && value > max {
                                    result.add_error(
                                        &format!("property_values.{}", prop_name),
                                        &format!("属性值 {} 大于最大值 {}", value, max),
                                        "VALUE_TOO_LARGE",
                                    );
                                }
                        }
                } else {
                    result.add_warning(
                        &format!("property_values.{}", prop_name),
                        &format!("属性 '{}' 在模板中不存在", prop_name),
                        "UNKNOWN_PROPERTY",
                    );
                }
            }
        }

        // 验证启用的命令
        if let Ok(commands) = template.get_commands() {
            for enabled_command in &input.enabled_commands {
                if !commands.iter().any(|c| c.name == *enabled_command) {
                    result.add_warning(
                        "enabled_commands",
                        &format!("命令 '{}' 在模板中不存在", enabled_command),
                        "UNKNOWN_COMMAND",
                    );
                }
            }
        }

        debug!("用户输入验证完成，错误数: {}", result.errors.len());
        result
    }

    /// 验证必需字段 (需求 6.2)
    fn validate_required_fields(&self, template: &DeviceTemplate, result: &mut ValidationResult) {
        if template.name.trim().is_empty() {
            result.add_error("name", "模板名称不能为空", "REQUIRED_FIELD");
        }

        if template.category.trim().is_empty() {
            result.add_error("category", "模板分类不能为空", "REQUIRED_FIELD");
        }

        if template.device_type.trim().is_empty() {
            result.add_error("device_type", "设备类型不能为空", "REQUIRED_FIELD");
        }

        if template.version.trim().is_empty() {
            result.add_error("version", "模板版本不能为空", "REQUIRED_FIELD");
        }

        // 验证版本格式 (语义化版本)
        if !self.is_valid_version(&template.version) {
            result.add_warning(
                "version",
                "建议使用语义化版本格式 (如: 1.0.0)",
                "INVALID_VERSION_FORMAT",
            );
        }

        // 验证名称格式 (只允许字母、数字、下划线、连字符)
        if !self.is_valid_name(&template.name) {
            result.add_error(
                "name",
                "模板名称只能包含字母、数字、下划线和连字符",
                "INVALID_NAME_FORMAT",
            );
        }
    }

    /// 验证JSON字段 (需求 6.1)
    fn validate_json_fields(&self, template: &DeviceTemplate, result: &mut ValidationResult) {
        // 验证显示名称JSON
        if let Err(e) = serde_json::from_str::<HashMap<String, String>>(&template.display_name) {
            result.add_error(
                "display_name",
                &format!("显示名称JSON格式错误: {}", e),
                "INVALID_JSON",
            );
        }

        // 验证描述JSON
        if let Some(description) = &template.description
            && let Err(e) = serde_json::from_str::<HashMap<String, String>>(description) {
                result.add_error(
                    "description",
                    &format!("描述JSON格式错误: {}", e),
                    "INVALID_JSON",
                );
            }

        // 验证标签JSON
        if let Err(e) = serde_json::from_str::<Vec<String>>(&template.tags) {
            result.add_error("tags", &format!("标签JSON格式错误: {}", e), "INVALID_JSON");
        }

        // 验证设备信息JSON
        if let Err(e) = template.get_device_info() {
            result.add_error(
                "device_info",
                &format!("设备信息JSON格式错误: {}", e),
                "INVALID_JSON",
            );
        }

        // 验证属性JSON
        if let Err(e) = template.get_properties() {
            result.add_error("properties", &format!("属性模板JSON格式错误: {}", e), "INVALID_JSON");
        }

        // 验证命令JSON
        if let Err(e) = template.get_commands() {
            result.add_error("commands", &format!("命令模板JSON格式错误: {}", e), "INVALID_JSON");
        }
    }

    /// 验证属性模板 (需求 6.3)
    fn validate_property_templates_internal(
        &self,
        template: &DeviceTemplate,
        result: &mut ValidationResult,
    ) {
        if let Ok(properties) = template.get_properties() {
            let property_result = self.validate_property_templates(&properties);
            result.merge(property_result);
        }
    }

    /// 验证命令模板 (需求 6.4)
    fn validate_command_templates_internal(
        &self,
        template: &DeviceTemplate,
        result: &mut ValidationResult,
    ) {
        if let Ok(commands) = template.get_commands() {
            let command_result = self.validate_command_templates(&commands);
            result.merge(command_result);
        }
    }

    /// 验证重复名称 (需求 6.6)
    fn validate_unique_names(&self, template: &DeviceTemplate, result: &mut ValidationResult) {
        // 验证属性名称唯一性
        if let Ok(properties) = template.get_properties() {
            let mut property_names = HashSet::new();
            for property in &properties {
                if !property_names.insert(&property.name) {
                    result.add_error(
                        "properties",
                        &format!("属性名称 '{}' 重复", property.name),
                        "DUPLICATE_PROPERTY_NAME",
                    );
                }
            }
        }

        // 验证命令名称唯一性
        if let Ok(commands) = template.get_commands() {
            let mut command_names = HashSet::new();
            for command in &commands {
                if !command_names.insert(&command.name) {
                    result.add_error(
                        "commands",
                        &format!("命令名称 '{}' 重复", command.name),
                        "DUPLICATE_COMMAND_NAME",
                    );
                }
            }
        }
    }

    /// 验证驱动引用 (需求 6.5)
    fn validate_driver_reference(&self, template: &DeviceTemplate, result: &mut ValidationResult) {
        if let Some(driver_name) = &template.driver_name
            && !driver_name.trim().is_empty() {
                // 检查驱动名称是否在已知驱动列表中
                let known_drivers = vec![
                    "modbus_rtu",
                    "modbus_tcp",
                    "onvif",
                    "snmp",
                    "mqtt",
                    "http",
                    "tcp",
                    "udp",
                    "serial",
                    "custom",
                ];

                if !known_drivers.contains(&driver_name.as_str()) {
                    result.add_warning(
                        "driver_name",
                        &format!("驱动 '{}' 可能不存在，请确认驱动已正确安装", driver_name),
                        "UNKNOWN_DRIVER",
                    );
                }
            }
    }

    /// 验证数据类型是否有效
    fn is_valid_data_type(&self, data_type: &str) -> bool {
        matches!(
            data_type,
            "string" | "number" | "boolean" | "integer" | "float" | "array" | "object"
        )
    }

    /// 验证值是否符合指定的数据类型
    fn validate_value_type(&self, value: &str, data_type: &str) -> bool {
        match data_type {
            "string" => true, // 字符串总是有效的
            "number" | "float" => value.parse::<f64>().is_ok(),
            "integer" => value.parse::<i64>().is_ok(),
            "boolean" => matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0"),
            "array" => serde_json::from_str::<Vec<serde_json::Value>>(value).is_ok(),
            "object" => serde_json::from_str::<serde_json::Value>(value).is_ok(),
            _ => false,
        }
    }

    /// 验证版本格式是否有效
    fn is_valid_version(&self, version: &str) -> bool {
        // 简单的语义化版本验证 (major.minor.patch)
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u32>().is_ok())
    }

    /// 验证名称格式是否有效
    fn is_valid_name(&self, name: &str) -> bool {
        // 只允许字母、数字、下划线、连字符
        name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    }

    /// 验证单个字段 (用于向导的实时验证)
    pub fn validate_field(
        &self,
        template: &DeviceTemplate,
        field_name: &str,
        field_value: &str,
    ) -> ValidationResult {
        debug!("验证单个字段: 模板={}, 字段={}, 值={}", template.name, field_name, field_value);

        let mut result = ValidationResult::success();

        // 验证基本字段
        match field_name {
            "name" => {
                if field_value.trim().is_empty() {
                    result.add_error("name", "设备名称不能为空", "FIELD_REQUIRED");
                } else if field_value.len() > 100 {
                    result.add_error("name", "设备名称长度不能超过100个字符", "FIELD_TOO_LONG");
                } else if !field_value.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    result.add_error(
                        "name",
                        "设备名称只能包含字母、数字、下划线和连字符",
                        "FIELD_INVALID_FORMAT",
                    );
                }
            }
            "display_name" => {
                if !field_value.trim().is_empty() && field_value.len() > 200 {
                    result.add_error(
                        "display_name",
                        "显示名称长度不能超过200个字符",
                        "FIELD_TOO_LONG",
                    );
                }
            }
            "address" => {
                if field_value.trim().is_empty() {
                    result.add_warning("address", "建议填写设备地址以便连接", "FIELD_RECOMMENDED");
                } else if field_value.len() > 255 {
                    result.add_error("address", "设备地址长度不能超过255个字符", "FIELD_TOO_LONG");
                }
            }
            "description" => {
                if !field_value.trim().is_empty() && field_value.len() > 1000 {
                    result.add_error("description", "描述长度不能超过1000个字符", "FIELD_TOO_LONG");
                }
            }
            _ => {
                // 对于其他字段，进行基本的长度验证
                if field_value.len() > 500 {
                    result.add_error(field_name, "字段值过长", "FIELD_TOO_LONG");
                }
            }
        }

        // 检查是否为必需字段
        if let Ok(device_info) = template.get_device_info()
            && device_info.required_fields.contains(&field_name.to_string())
                && field_value.trim().is_empty()
            {
                result.add_error(field_name, "此字段为必填项", "FIELD_REQUIRED");
            }

        debug!(
            "单个字段验证完成: 字段={}, 错误数={}, 警告数={}",
            field_name,
            result.errors.len(),
            result.warnings.len()
        );

        result
    }
}

impl Default for TemplateValidator {
    fn default() -> Self {
        Self::new()
    }
}

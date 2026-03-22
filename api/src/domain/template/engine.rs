use crate::domain::template::repository::TemplateRepository;
use crate::domain::template::validator::TemplateValidator;
use crate::dto::entity::device::CreateDeviceRequest;
use crate::dto::entity::device_command::CreateDeviceCommandRequest;
use crate::dto::entity::device_property::CreateDevicePropertyRequest;
use crate::dto::entity::device_template::{
    CommandInfo, DeviceCreationInput, DevicePreview, DeviceTemplate, PropertyInfo,
    TemplateRequirements,
};
use crate::dto::entity::template_error::{TemplateError, ValidationResult};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// 模板引擎 - 负责解析和应用设备模板的核心组件
#[derive(Debug)]
pub struct TemplateEngine {
    template_repository: Arc<TemplateRepository>,
    validator: Arc<TemplateValidator>,
}

impl TemplateEngine {
    /// 创建新的模板引擎实例
    pub fn new(
        template_repository: Arc<TemplateRepository>,
        validator: Arc<TemplateValidator>,
    ) -> Self {
        Self {
            template_repository,
            validator,
        }
    }

    /// 应用模板创建设备 (需求 3.6)
    pub async fn apply_template(
        &self,
        template_id: &str,
        user_input: &DeviceCreationInput,
    ) -> Result<CreateDeviceRequest, TemplateError> {
        info!(
            "应用模板创建设备: template_id={}, device_name={}",
            template_id, user_input.name
        );

        // 获取模板
        let template = self
            .template_repository
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| TemplateError::TemplateNotFound {
                id: template_id.to_string(),
            })?;

        // 验证用户输入 (需求 3.7)
        let validation_result = self.validator.validate_user_input(&template, user_input);
        if validation_result.has_errors() {
            return Err(TemplateError::ValidationFailed {
                errors: validation_result.errors,
            });
        }

        // 解析模板信息
        let device_info =
            template
                .get_device_info()
                .map_err(|e| TemplateError::JsonFormatError {
                    message: format!("设备信息解析失败: {}", e),
                })?;

        // 应用模板创建设备请求 (需求 3.5)
        let device_request = CreateDeviceRequest {
            name: user_input.name.clone(),
            display_name: user_input.display_name.clone().or_else(|| {
                self.apply_name_pattern(&device_info.default_display_name_pattern, user_input)
            }),
            device_type: Some(template.device_type.clone()),
            address: user_input.address.clone(),
            description: user_input.description.clone().or_else(|| {
                device_info
                    .default_description
                    .as_ref()
                    .and_then(|desc| self.get_localized_description(desc, "zh"))
            }),
            position: user_input
                .position
                .clone()
                .or_else(|| device_info.default_position.clone()),
            driver_name: user_input
                .driver_name
                .clone()
                .or_else(|| template.driver_name.clone()),
            device_model: template.manufacturer.clone(),
            protocol_type: template.protocol_type.clone(),
            factory_name: template.manufacturer.clone(),
            linked_data: None,
            driver_options: user_input
                .driver_options
                .clone()
                .or_else(|| device_info.default_driver_options.clone()),
            parent_id: user_input.parent_id.clone(),
            product_id: user_input.product_id.clone(),
            organization_id: user_input.organization_id.clone(),
        };

        debug!("设备创建请求已生成: {:?}", device_request);
        Ok(device_request)
    }

    /// 预览基于模板的设备创建 (需求 3.4)
    pub async fn preview_template(
        &self,
        template_id: &str,
        user_input: &DeviceCreationInput,
    ) -> Result<DevicePreview, TemplateError> {
        info!(
            "预览模板设备创建: template_id={}, device_name={}",
            template_id, user_input.name
        );

        // 获取模板
        let template = self
            .template_repository
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| TemplateError::TemplateNotFound {
                id: template_id.to_string(),
            })?;

        // 验证用户输入
        let validation_result = self.validator.validate_user_input(&template, user_input);

        // 生成设备信息
        let device_info = self.apply_template(template_id, user_input).await?;

        // 生成属性列表
        let properties = self
            .generate_device_properties(&template, user_input, "temp_device_id")
            .await?;

        // 生成命令列表
        let commands = self
            .generate_device_commands(&template, user_input, "temp_device_id")
            .await?;

        // 收集警告信息
        let mut warnings = Vec::new();
        if validation_result.has_warnings() {
            warnings.extend(validation_result.warnings.iter().map(|w| w.message.clone()));
        }

        // 添加模板相关的警告
        if template.driver_name.is_none() {
            warnings.push("模板未指定驱动程序，可能需要手动配置".to_string());
        }

        let preview = DevicePreview {
            device_info,
            properties,
            commands,
            warnings,
        };

        debug!(
            "设备预览已生成: 属性数={}, 命令数={}, 警告数={}",
            preview.properties.len(),
            preview.commands.len(),
            preview.warnings.len()
        );

        Ok(preview)
    }

    /// 验证用户输入 (需求 3.7)
    pub async fn validate_user_input(
        &self,
        template_id: &str,
        user_input: &DeviceCreationInput,
    ) -> Result<ValidationResult, TemplateError> {
        info!("验证用户输入: template_id={}", template_id);

        // 获取模板
        let template = self
            .template_repository
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| TemplateError::TemplateNotFound {
                id: template_id.to_string(),
            })?;

        // 使用验证器验证输入
        let result = self.validator.validate_user_input(&template, user_input);

        debug!(
            "用户输入验证完成: 错误数={}, 警告数={}",
            result.errors.len(),
            result.warnings.len()
        );
        Ok(result)
    }

    /// 根据模板生成设备属性
    pub async fn generate_device_properties(
        &self,
        template: &DeviceTemplate,
        user_input: &DeviceCreationInput,
        device_id: &str,
    ) -> Result<Vec<CreateDevicePropertyRequest>, TemplateError> {
        let properties = template
            .get_properties()
            .map_err(|e| TemplateError::JsonFormatError {
                message: format!("属性模板解析失败: {}", e),
            })?;

        let mut device_properties = Vec::new();

        for property in properties {
            // 获取用户覆盖的属性值或使用默认值
            let default_value = user_input
                .property_values
                .get(&property.name)
                .cloned()
                .or_else(|| property.default_value.clone());

            let device_property = CreateDevicePropertyRequest {
                device_id: device_id.to_string(),
                name: property.name.clone(),
                display_name: Some(self.get_localized_display_name(&property.display_name, "zh")),
                description: property
                    .description
                    .as_ref()
                    .and_then(|desc| self.get_localized_description(desc, "zh")),
                data_type: Some(property.data_type.clone()),
                unit: property.unit.clone(),
                min_value: property.min_value,
                max_value: property.max_value,
                default_value,
                is_read_only: Some(if property.is_read_only { 1 } else { 0 }),
            };

            device_properties.push(device_property);
        }

        debug!("生成了 {} 个设备属性", device_properties.len());
        Ok(device_properties)
    }

    /// 根据模板生成设备命令
    pub async fn generate_device_commands(
        &self,
        template: &DeviceTemplate,
        user_input: &DeviceCreationInput,
        device_id: &str,
    ) -> Result<Vec<CreateDeviceCommandRequest>, TemplateError> {
        let commands = template
            .get_commands()
            .map_err(|e| TemplateError::JsonFormatError {
                message: format!("命令模板解析失败: {}", e),
            })?;

        let mut device_commands = Vec::new();

        for command in commands {
            // 检查用户是否启用了此命令
            let is_enabled = user_input.enabled_commands.is_empty()
                || user_input.enabled_commands.contains(&command.name)
                || command.is_required;

            if is_enabled {
                let device_command = CreateDeviceCommandRequest {
                    device_id: device_id.to_string(),
                    name: command.name.clone(),
                    display_name: Some(
                        self.get_localized_display_name(&command.display_name, "zh"),
                    ),
                    description: command
                        .description
                        .as_ref()
                        .and_then(|desc| self.get_localized_description(desc, "zh")),
                    parameters: command.parameters.clone(),
                };

                device_commands.push(device_command);
            }
        }

        debug!("生成了 {} 个设备命令", device_commands.len());
        Ok(device_commands)
    }

    /// 应用名称模式
    fn apply_name_pattern(
        &self,
        pattern: &Option<String>,
        user_input: &DeviceCreationInput,
    ) -> Option<String> {
        pattern.as_ref().map(|p| {
            let mut result = p.clone();

            // 替换占位符
            result = result.replace("{name}", &user_input.name);
            if let Some(display_name) = &user_input.display_name {
                result = result.replace("{display_name}", display_name);
            }

            // 可以添加更多占位符替换逻辑
            result = result.replace("{index}", "1"); // 简单的索引替换

            result
        })
    }

    /// 获取本地化显示名称
    fn get_localized_display_name(
        &self,
        display_names: &HashMap<String, String>,
        language: &str,
    ) -> String {
        display_names
            .get(language)
            .or_else(|| display_names.get("zh")) // 回退到中文
            .or_else(|| display_names.values().next()) // 回退到任意语言
            .cloned()
            .unwrap_or_else(|| "未命名".to_string())
    }

    /// 获取本地化描述
    fn get_localized_description(
        &self,
        descriptions: &HashMap<String, String>,
        language: &str,
    ) -> Option<String> {
        descriptions
            .get(language)
            .or_else(|| descriptions.get("zh")) // 回退到中文
            .or_else(|| descriptions.values().next()) // 回退到任意语言
            .cloned()
    }

    /// 获取模板仓库引用
    pub fn get_repository(&self) -> &TemplateRepository {
        &self.template_repository
    }

    /// 获取验证器引用
    pub fn get_validator(&self) -> &TemplateValidator {
        &self.validator
    }

    /// 获取模板的必需字段信息 (用于设备创建向导)
    pub async fn get_template_requirements(
        &self,
        template_id: &str,
    ) -> Result<TemplateRequirements, TemplateError> {
        info!("获取模板必需字段信息: template_id={}", template_id);

        // 获取模板
        let template = self
            .template_repository
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| TemplateError::TemplateNotFound {
                id: template_id.to_string(),
            })?;

        // 解析设备信息
        let device_info =
            template
                .get_device_info()
                .map_err(|e| TemplateError::JsonFormatError {
                    message: format!("设备信息解析失败: {}", e),
                })?;

        // 解析属性模板
        let properties = template
            .get_properties()
            .map_err(|e| TemplateError::JsonFormatError {
                message: format!("属性模板解析失败: {}", e),
            })?;

        // 解析命令模板
        let commands = template
            .get_commands()
            .map_err(|e| TemplateError::JsonFormatError {
                message: format!("命令模板解析失败: {}", e),
            })?;

        let requirements = TemplateRequirements {
            template_id: template_id.to_string(),
            template_name: template.name.clone(),
            display_name: self.get_localized_display_name(
                &serde_json::from_str(&template.display_name).unwrap_or_default(),
                "zh",
            ),
            required_fields: device_info.required_fields,
            available_properties: properties
                .iter()
                .map(|p| PropertyInfo {
                    name: p.name.clone(),
                    display_name: self.get_localized_display_name(&p.display_name, "zh"),
                    data_type: p.data_type.clone(),
                    is_required: p.is_required,
                    default_value: p.default_value.clone(),
                    validation_rules: p.validation_rules.clone(),
                })
                .collect(),
            available_commands: commands
                .iter()
                .map(|c| CommandInfo {
                    name: c.name.clone(),
                    display_name: self.get_localized_display_name(&c.display_name, "zh"),
                    is_required: c.is_required,
                    parameters: c.parameters.clone(),
                })
                .collect(),
        };

        debug!(
            "模板必需字段信息已生成: 必需字段数={}, 属性数={}, 命令数={}",
            requirements.required_fields.len(),
            requirements.available_properties.len(),
            requirements.available_commands.len()
        );

        Ok(requirements)
    }

    /// 验证单个字段 (用于设备创建向导的实时验证)
    pub async fn validate_field(
        &self,
        template_id: &str,
        field_name: &str,
        field_value: &str,
    ) -> Result<ValidationResult, TemplateError> {
        info!(
            "验证单个字段: template_id={}, field={}, value={}",
            template_id, field_name, field_value
        );

        // 获取模板
        let template = self
            .template_repository
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| TemplateError::TemplateNotFound {
                id: template_id.to_string(),
            })?;

        // 使用验证器验证单个字段
        let result = self
            .validator
            .validate_field(&template, field_name, field_value);

        debug!(
            "单个字段验证完成: 字段={}, 错误数={}, 警告数={}",
            field_name,
            result.errors.len(),
            result.warnings.len()
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::entity::device_template::{CommandTemplate, DeviceInfo, PropertyTemplate};
    use std::collections::HashMap;

    fn create_test_template() -> DeviceTemplate {
        let mut template = DeviceTemplate::default();
        template.id = "test-template".to_string();
        template.name = "test_sensor".to_string();
        template.device_type = "sensor".to_string();
        template.category = "sensors".to_string();
        template.version = "1.0.0".to_string();

        // 设置显示名称
        let mut display_name = HashMap::new();
        display_name.insert("zh".to_string(), "测试传感器".to_string());
        display_name.insert("en".to_string(), "Test Sensor".to_string());
        template.display_name = serde_json::to_string(&display_name).unwrap();

        // 设置设备信息
        let device_info = DeviceInfo {
            default_name_pattern: "sensor_{index}".to_string(),
            default_display_name_pattern: Some("传感器 {index}".to_string()),
            default_description: None,
            default_position: None,
            default_driver_options: None,
            required_fields: vec!["name".to_string()],
        };
        template.device_info = serde_json::to_string(&device_info).unwrap();

        // 设置属性
        let mut property_display_name = HashMap::new();
        property_display_name.insert("zh".to_string(), "温度".to_string());
        property_display_name.insert("en".to_string(), "Temperature".to_string());

        let property = PropertyTemplate {
            name: "temperature".to_string(),
            display_name: property_display_name,
            description: None,
            data_type: "number".to_string(),
            unit: Some("°C".to_string()),
            min_value: Some(-50.0),
            max_value: Some(200.0),
            default_value: Some("25.0".to_string()),
            is_read_only: true,
            is_required: true,
            validation_rules: None,
        };
        template.properties = serde_json::to_string(&vec![property]).unwrap();

        // 设置命令
        let mut command_display_name = HashMap::new();
        command_display_name.insert("zh".to_string(), "读取温度".to_string());
        command_display_name.insert("en".to_string(), "Read Temperature".to_string());

        let command = CommandTemplate {
            name: "read_temperature".to_string(),
            display_name: command_display_name,
            description: None,
            parameters: Some("{}".to_string()),
            parameter_schema: None,
            is_required: true,
        };
        template.commands = serde_json::to_string(&vec![command]).unwrap();

        template
    }

    fn create_test_user_input() -> DeviceCreationInput {
        DeviceCreationInput {
            name: "test_device".to_string(),
            display_name: Some("测试设备".to_string()),
            description: Some("这是一个测试设备".to_string()),
            position: None,
            address: Some(
                std::env::var("TEST_DEVICE_ADDRESS").unwrap_or_else(|_| "127.0.0.1".to_string()),
            ),
            driver_name: Some("".to_string()),
            driver_options: None,
            parent_id: None,
            product_id: None,
            organization_id: None,
            property_values: HashMap::new(),
            enabled_commands: vec!["read_temperature".to_string()],
        }
    }

    // Tests are commented out for now as they require database setup
    // TODO: Implement proper unit tests with mocked dependencies

    /*
    #[test]
    fn test_apply_name_pattern() {
        let validator = Arc::new(TemplateValidator::new());
        let database = Arc::new(crate::infrastructure::persistence::database::Database::new("test.db").unwrap());
        let repository = Arc::new(TemplateRepository::new(database, std::path::PathBuf::from("test_templates")));
        let engine = TemplateEngine::new(repository, validator);
        let user_input = create_test_user_input();

        let pattern = Some("sensor_{name}".to_string());
        let result = engine.apply_name_pattern(&pattern, &user_input);

        assert_eq!(result, Some("sensor_test_device".to_string()));
    }
    */
}

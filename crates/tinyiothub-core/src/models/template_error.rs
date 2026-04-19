use serde::{Deserialize, Serialize};

/// 设备模板系统错误类型
#[derive(Debug)]
pub enum TemplateError {
    TemplateNotFound { id: String },
    ValidationFailed { errors: Vec<ValidationError> },
    JsonFormatError { message: String },
    DependencyConflict { message: String },
    DatabaseError { message: String },
    FileSystemError { message: String },
    EngineError { message: String },
    InvalidUserInput { field: String, message: String },
    SerializationError { message: String },
    TemplateNameExists { name: String },
    CategoryNotFound { category: String },
    TemplateInUse { template_id: String },
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::TemplateNotFound { id } => write!(f, "模板不存在: {}", id),
            TemplateError::ValidationFailed { errors } => write!(f, "模板验证失败: {:?}", errors),
            TemplateError::JsonFormatError { message } => write!(f, "JSON格式错误: {}", message),
            TemplateError::DependencyConflict { message } => write!(f, "模板依赖冲突: {}", message),
            TemplateError::DatabaseError { message } => write!(f, "数据库操作失败: {}", message),
            TemplateError::FileSystemError { message } => write!(f, "文件系统操作失败: {}", message),
            TemplateError::EngineError { message } => write!(f, "模板引擎错误: {}", message),
            TemplateError::InvalidUserInput { field, message } => write!(f, "用户输入无效: {} - {}", field, message),
            TemplateError::SerializationError { message } => write!(f, "序列化错误: {}", message),
            TemplateError::TemplateNameExists { name } => write!(f, "模板名称已存在: {}", name),
            TemplateError::CategoryNotFound { category } => write!(f, "模板分类不存在: {}", category),
            TemplateError::TemplateInUse { template_id } => write!(f, "模板正在被设备使用，无法删除: {}", template_id),
        }
    }
}

impl std::error::Error for TemplateError {}

/// API错误类型
#[derive(Debug)]
pub enum ApiError {
    BadRequest { message: String },
    NotFound { resource: String },
    InternalServerError { message: String },
    TemplateError { source: TemplateError },
    Forbidden,
    Conflict { message: String },
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::BadRequest { message } => write!(f, "请求参数无效: {}", message),
            ApiError::NotFound { resource } => write!(f, "资源未找到: {}", resource),
            ApiError::InternalServerError { message } => write!(f, "内部服务器错误: {}", message),
            ApiError::TemplateError { source } => write!(f, "模板错误: {}", source),
            ApiError::Forbidden => write!(f, "权限不足"),
            ApiError::Conflict { message } => write!(f, "请求冲突: {}", message),
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::TemplateError { source } => Some(source),
            _ => None,
        }
    }
}

/// 验证错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub error_code: String,
}

/// 验证警告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub field: String,
    pub message: String,
    pub warning_code: String,
}

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    pub fn success() -> Self {
        Self { is_valid: true, errors: Vec::new(), warnings: Vec::new() }
    }

    pub fn failure(errors: Vec<ValidationError>) -> Self {
        Self { is_valid: false, errors, warnings: Vec::new() }
    }

    pub fn success_with_warnings(warnings: Vec<ValidationWarning>) -> Self {
        Self { is_valid: true, errors: Vec::new(), warnings }
    }

    pub fn add_error(&mut self, field: &str, message: &str, error_code: &str) {
        self.errors.push(ValidationError {
            field: field.to_string(),
            message: message.to_string(),
            error_code: error_code.to_string(),
        });
        self.is_valid = false;
    }

    pub fn add_warning(&mut self, field: &str, message: &str, warning_code: &str) {
        self.warnings.push(ValidationWarning {
            field: field.to_string(),
            message: message.to_string(),
            warning_code: warning_code.to_string(),
        });
    }

    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        if !other.is_valid {
            self.is_valid = false;
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

impl ValidationError {
    pub fn new(field: &str, message: &str, error_code: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            error_code: error_code.to_string(),
        }
    }

    pub fn required_field(field: &str) -> Self {
        Self::new(field, &format!("字段 {} 是必填的", field), "REQUIRED_FIELD")
    }

    pub fn field_length(field: &str, max_length: usize) -> Self {
        Self::new(
            field,
            &format!("字段 {} 长度不能超过 {} 个字符", field, max_length),
            "FIELD_TOO_LONG",
        )
    }

    pub fn invalid_value(field: &str, value: &str) -> Self {
        Self::new(field, &format!("字段 {} 的值 '{}' 无效", field, value), "INVALID_VALUE")
    }

    pub fn json_format(field: &str) -> Self {
        Self::new(field, &format!("字段 {} 不是有效的JSON格式", field), "INVALID_JSON")
    }

    pub fn value_range(field: &str, min: f64, max: f64) -> Self {
        Self::new(
            field,
            &format!("字段 {} 的值必须在 {} 到 {} 之间", field, min, max),
            "VALUE_OUT_OF_RANGE",
        )
    }
}

impl ValidationWarning {
    pub fn new(field: &str, message: &str, warning_code: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            warning_code: warning_code.to_string(),
        }
    }

    pub fn empty_field(field: &str) -> Self {
        Self::new(field, &format!("字段 {} 为空，建议填写", field), "EMPTY_FIELD")
    }

    pub fn default_value(field: &str, default_value: &str) -> Self {
        Self::new(
            field,
            &format!("字段 {} 将使用默认值: {}", field, default_value),
            "DEFAULT_VALUE",
        )
    }

    pub fn compatibility(field: &str, message: &str) -> Self {
        Self::new(field, message, "COMPATIBILITY_WARNING")
    }
}

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 设备模板系统错误类型
#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("模板不存在: {id}")]
    TemplateNotFound { id: String },

    #[error("模板验证失败: {errors:?}")]
    ValidationFailed { errors: Vec<ValidationError> },

    #[error("JSON格式错误: {message}")]
    JsonFormatError { message: String },

    #[error("模板依赖冲突: {message}")]
    DependencyConflict { message: String },

    #[error("数据库操作失败: {message}")]
    DatabaseError { message: String },

    #[error("文件系统操作失败: {source}")]
    FileSystemError {
        #[from]
        source: std::io::Error,
    },

    #[error("模板引擎错误: {message}")]
    EngineError { message: String },

    #[error("用户输入无效: {field} - {message}")]
    InvalidUserInput { field: String, message: String },

    #[error("序列化错误: {source}")]
    SerializationError {
        #[from]
        source: serde_json::Error,
    },

    #[error("模板名称已存在: {name}")]
    TemplateNameExists { name: String },

    #[error("模板分类不存在: {category}")]
    CategoryNotFound { category: String },

    #[error("模板正在被设备使用，无法删除: {template_id}")]
    TemplateInUse { template_id: String },
}

#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for TemplateError {
    fn from(err: sqlx::Error) -> Self {
        TemplateError::DatabaseError { message: err.to_string() }
    }
}

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
    /// 创建成功的验证结果
    pub fn success() -> Self {
        Self { is_valid: true, errors: Vec::new(), warnings: Vec::new() }
    }

    /// 创建失败的验证结果
    pub fn failure(errors: Vec<ValidationError>) -> Self {
        Self { is_valid: false, errors, warnings: Vec::new() }
    }

    /// 创建带警告的成功验证结果
    pub fn success_with_warnings(warnings: Vec<ValidationWarning>) -> Self {
        Self { is_valid: true, errors: Vec::new(), warnings }
    }

    /// 添加错误
    pub fn add_error(&mut self, field: &str, message: &str, error_code: &str) {
        self.errors.push(ValidationError {
            field: field.to_string(),
            message: message.to_string(),
            error_code: error_code.to_string(),
        });
        self.is_valid = false;
    }

    /// 添加警告
    pub fn add_warning(&mut self, field: &str, message: &str, warning_code: &str) {
        self.warnings.push(ValidationWarning {
            field: field.to_string(),
            message: message.to_string(),
            warning_code: warning_code.to_string(),
        });
    }

    /// 合并验证结果
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        if !other.is_valid {
            self.is_valid = false;
        }
    }

    /// 检查是否有错误
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// 检查是否有警告
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

impl ValidationError {
    /// 创建新的验证错误
    pub fn new(field: &str, message: &str, error_code: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            error_code: error_code.to_string(),
        }
    }

    /// 必填字段错误
    pub fn required_field(field: &str) -> Self {
        Self::new(field, &format!("字段 {} 是必填的", field), "REQUIRED_FIELD")
    }

    /// 字段长度错误
    pub fn field_length(field: &str, max_length: usize) -> Self {
        Self::new(
            field,
            &format!("字段 {} 长度不能超过 {} 个字符", field, max_length),
            "FIELD_TOO_LONG",
        )
    }

    /// 无效值错误
    pub fn invalid_value(field: &str, value: &str) -> Self {
        Self::new(field, &format!("字段 {} 的值 '{}' 无效", field, value), "INVALID_VALUE")
    }

    /// JSON格式错误
    pub fn json_format(field: &str) -> Self {
        Self::new(field, &format!("字段 {} 不是有效的JSON格式", field), "INVALID_JSON")
    }

    /// 数值范围错误
    pub fn value_range(field: &str, min: f64, max: f64) -> Self {
        Self::new(
            field,
            &format!("字段 {} 的值必须在 {} 到 {} 之间", field, min, max),
            "VALUE_OUT_OF_RANGE",
        )
    }
}

impl ValidationWarning {
    /// 创建新的验证警告
    pub fn new(field: &str, message: &str, warning_code: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            warning_code: warning_code.to_string(),
        }
    }

    /// 字段为空警告
    pub fn empty_field(field: &str) -> Self {
        Self::new(field, &format!("字段 {} 为空，建议填写", field), "EMPTY_FIELD")
    }

    /// 默认值警告
    pub fn default_value(field: &str, default_value: &str) -> Self {
        Self::new(
            field,
            &format!("字段 {} 将使用默认值: {}", field, default_value),
            "DEFAULT_VALUE",
        )
    }

    /// 兼容性警告
    pub fn compatibility(field: &str, message: &str) -> Self {
        Self::new(field, message, "COMPATIBILITY_WARNING")
    }
}

/// 从 TemplateError 转换为 ApiError
impl From<TemplateError> for ApiError {
    fn from(err: TemplateError) -> Self {
        match err {
            TemplateError::TemplateNotFound { .. } => {
                ApiError::NotFound { resource: "设备模板".to_string() }
            }
            TemplateError::ValidationFailed { .. } => {
                ApiError::BadRequest { message: err.to_string() }
            }
            TemplateError::JsonFormatError { .. } => {
                ApiError::BadRequest { message: err.to_string() }
            }
            TemplateError::InvalidUserInput { .. } => {
                ApiError::BadRequest { message: err.to_string() }
            }
            TemplateError::TemplateNameExists { .. } => {
                ApiError::Conflict { message: err.to_string() }
            }
            TemplateError::CategoryNotFound { .. } => {
                ApiError::BadRequest { message: err.to_string() }
            }
            TemplateError::TemplateInUse { .. } => ApiError::Conflict { message: err.to_string() },
            _ => ApiError::InternalServerError { message: err.to_string() },
        }
    }
}

/// HTTP状态码映射
impl ApiError {
    pub fn status_code(&self) -> u16 {
        match self {
            ApiError::BadRequest { .. } => 400,
            ApiError::Forbidden => 403,
            ApiError::NotFound { .. } => 404,
            ApiError::Conflict { .. } => 409,
            ApiError::InternalServerError { .. } => 500,
            ApiError::TemplateError { source } => match source {
                TemplateError::TemplateNotFound { .. } => 404,
                TemplateError::ValidationFailed { .. } => 400,
                TemplateError::JsonFormatError { .. } => 400,
                TemplateError::InvalidUserInput { .. } => 400,
                TemplateError::TemplateNameExists { .. } => 409,
                TemplateError::CategoryNotFound { .. } => 400,
                TemplateError::TemplateInUse { .. } => 409,
                _ => 500,
            },
        }
    }
}

/// 错误响应结构
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(error: &str, message: &str) -> Self {
        Self { error: error.to_string(), message: message.to_string(), details: None }
    }

    pub fn with_details(error: &str, message: &str, details: serde_json::Value) -> Self {
        Self { error: error.to_string(), message: message.to_string(), details: Some(details) }
    }

    pub fn from_template_error(err: &TemplateError) -> Self {
        match err {
            TemplateError::ValidationFailed { errors } => Self::with_details(
                "VALIDATION_FAILED",
                "模板验证失败",
                serde_json::to_value(errors).unwrap_or_default(),
            ),
            _ => Self::new("TEMPLATE_ERROR", &err.to_string()),
        }
    }

    pub fn from_api_error(err: &ApiError) -> Self {
        match err {
            ApiError::BadRequest { message } => Self::new("BAD_REQUEST", message),
            ApiError::NotFound { resource } => {
                Self::new("NOT_FOUND", &format!("{} 未找到", resource))
            }
            ApiError::Forbidden => Self::new("FORBIDDEN", "权限不足"),
            ApiError::Conflict { message } => Self::new("CONFLICT", message),
            ApiError::InternalServerError { message } => {
                Self::new("INTERNAL_SERVER_ERROR", message)
            }
            ApiError::TemplateError { source } => Self::from_template_error(source),
        }
    }
}

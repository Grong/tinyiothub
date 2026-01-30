use thiserror::Error;

/// 报警模块错误类型
#[derive(Error, Debug)]
pub enum AlarmError {
    #[error("报警未找到: {0}")]
    NotFound(String),

    #[error("报警规则未找到: {0}")]
    RuleNotFound(String),

    #[error("无效的报警状态转换: 从 {from} 到 {to}")]
    InvalidStatusTransition { from: String, to: String },

    #[error("报警已被确认")]
    AlreadyAcknowledged,

    #[error("报警已被解决")]
    AlreadyResolved,

    #[error("无效的报警条件: {0}")]
    InvalidCondition(String),

    #[error("无效的规则配置: {0}")]
    InvalidRuleConfig(String),

    #[error("数据库错误: {0}")]
    DatabaseError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("规则评估错误: {0}")]
    EvaluationError(String),

    #[error("权限不足")]
    PermissionDenied,

    #[error("内部错误: {0}")]
    InternalError(String),
}

impl From<sqlx::Error> for AlarmError {
    fn from(err: sqlx::Error) -> Self {
        AlarmError::DatabaseError(err.to_string())
    }
}

impl From<serde_json::Error> for AlarmError {
    fn from(err: serde_json::Error) -> Self {
        AlarmError::SerializationError(err.to_string())
    }
}

pub type AlarmResult<T> = Result<T, AlarmError>;

use crate::dto::response::ApiResponse;
use axum::response::Json;
use serde::Serialize;

/// 统一的API响应构建器
/// 确保所有API端点使用一致的响应格式
pub struct ApiResponseBuilder;

impl ApiResponseBuilder {
    /// 创建成功响应
    pub fn success<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
        Json(ApiResponse {
            code: 0,
            msg: String::new(),
            result: Some(data),
        })
    }

    /// 创建成功响应（带消息）
    pub fn success_with_message<T: Serialize>(
        data: T,
        message: impl Into<String>,
    ) -> Json<ApiResponse<T>> {
        Json(ApiResponse {
            code: 0,
            msg: message.into(),
            result: Some(data),
        })
    }

    /// 创建错误响应
    pub fn error<T>(message: impl Into<String>) -> Json<ApiResponse<T>> {
        Json(ApiResponse {
            code: -1,
            msg: message.into(),
            result: None,
        })
    }

    /// 创建带错误码的错误响应
    pub fn error_with_code<T>(code: i32, message: impl Into<String>) -> Json<ApiResponse<T>> {
        Json(ApiResponse {
            code,
            msg: message.into(),
            result: None,
        })
    }

    /// 从Result创建响应
    pub fn from_result<T: Serialize, E: std::fmt::Display>(
        result: Result<T, E>,
    ) -> Json<ApiResponse<T>> {
        match result {
            Ok(data) => Self::success(data),
            Err(error) => Self::error(error.to_string()),
        }
    }
}

/// 便捷宏，用于快速创建API响应
#[macro_export]
macro_rules! api_success {
    ($data:expr) => {
        $crate::dto::response::builder::ApiResponseBuilder::success($data)
    };
    ($data:expr, $msg:expr) => {
        $crate::dto::response::builder::ApiResponseBuilder::success_with_message($data, $msg)
    };
}

#[macro_export]
macro_rules! api_error {
    ($msg:expr) => {
        $crate::dto::response::builder::ApiResponseBuilder::error($msg)
    };
    ($code:expr, $msg:expr) => {
        $crate::dto::response::builder::ApiResponseBuilder::error_with_code($code, $msg)
    };
}

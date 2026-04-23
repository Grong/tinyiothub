use axum::response::Json;
use serde::{Deserialize, Serialize};

/// Unified API response structure
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiResponse<T> {
    pub msg: String,
    pub code: i32,
    pub result: Option<T>,
}

/// Paginated response wrapper
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

/// Pagination metadata
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaginationInfo {
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub total_count: u64,
}

/// Request context for logging/tracing
#[derive(Clone, Debug, Default)]
pub struct ReqCtx {
    pub ori_uri: String,
    pub path: String,
    pub path_params: String,
    pub method: String,
    pub user: UserInfo,
    pub data: String,
}

/// User info extracted from JWT claims
#[derive(Debug, Clone, Default)]
pub struct UserInfo {
    pub id: String,
    pub token_id: String,
    pub name: String,
}

/// 统一的API响应构建器
/// 确保所有API端点使用一致的响应格式
pub struct ApiResponseBuilder;

impl ApiResponseBuilder {
    /// 创建成功响应
    pub fn success<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code: 0, msg: String::new(), result: Some(data) })
    }

    /// 创建成功响应（带消息）
    pub fn success_with_message<T: Serialize>(
        data: T,
        message: impl Into<String>,
    ) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code: 0, msg: message.into(), result: Some(data) })
    }

    /// 创建错误响应
    pub fn error<T>(message: impl Into<String>) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code: -1, msg: message.into(), result: None })
    }

    /// 创建带错误码的错误响应
    pub fn error_with_code<T>(code: i32, message: impl Into<String>) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code, msg: message.into(), result: None })
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

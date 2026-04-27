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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_response() {
        let resp = ApiResponseBuilder::success("hello");
        assert_eq!(resp.0.code, 0);
        assert_eq!(resp.0.msg, "");
        assert_eq!(resp.0.result, Some("hello"));
    }

    #[test]
    fn test_success_with_message() {
        let resp = ApiResponseBuilder::success_with_message(42, "created");
        assert_eq!(resp.0.code, 0);
        assert_eq!(resp.0.msg, "created");
        assert_eq!(resp.0.result, Some(42));
    }

    #[test]
    fn test_error_response() {
        let resp: Json<ApiResponse<String>> = ApiResponseBuilder::error("something broke");
        assert_eq!(resp.0.code, -1);
        assert_eq!(resp.0.msg, "something broke");
        assert_eq!(resp.0.result, None);
    }

    #[test]
    fn test_error_with_code() {
        let resp: Json<ApiResponse<String>> = ApiResponseBuilder::error_with_code(404, "not found");
        assert_eq!(resp.0.code, 404);
        assert_eq!(resp.0.msg, "not found");
        assert_eq!(resp.0.result, None);
    }

    #[test]
    fn test_from_result_ok() {
        let resp = ApiResponseBuilder::from_result(Ok::<_, String>("data"));
        assert_eq!(resp.0.code, 0);
        assert_eq!(resp.0.result, Some("data"));
    }

    #[test]
    fn test_from_result_err() {
        let resp: Json<ApiResponse<String>> =
            ApiResponseBuilder::from_result(Err::<String, _>("fail"));
        assert_eq!(resp.0.code, -1);
        assert_eq!(resp.0.msg, "fail");
        assert_eq!(resp.0.result, None);
    }

    #[test]
    fn test_response_serialization() {
        let resp = ApiResponseBuilder::success(vec![1, 2, 3]);
        let json = serde_json::to_value(&resp.0).unwrap();
        assert_eq!(json["code"], 0);
        assert_eq!(json["msg"], "");
        assert_eq!(json["result"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_paginated_response_serialization() {
        let resp = PaginatedResponse {
            data: vec!["a", "b"],
            pagination: PaginationInfo {
                page: 1,
                page_size: 10,
                total_pages: 5,
                total_count: 50,
            },
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["data"].as_array().unwrap().len(), 2);
        assert_eq!(json["pagination"]["page"], 1);
        assert_eq!(json["pagination"]["total_count"], 50);
    }
}

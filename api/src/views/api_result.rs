use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiResult<T> {
    pub result: String,
    pub data: Option<T>,
    pub code: String,
    pub message: String,
}

impl<T> ApiResult<T> {
    pub fn new(result: String, data: Option<T>, code: String, message: String) -> Self {
        Self {
            result,
            data,
            code,
            message,
        }
    }
}

impl<T> ApiResult<T> {
    pub fn success(data: Option<T>) -> Self {
        Self::new(
            "success".to_string(),
            data,
            "200".to_string(),
            "success".to_string(),
        )
    }

    pub fn error(message: String) -> Self {
        Self::new("fail".to_string(), None, "500".to_string(), message)
    }
}

impl<T> ApiResult<T> {
    pub fn from_result(result: Result<T, String>) -> Self {
        match result {
            Ok(data) => Self::success(Some(data)),
            Err(message) => Self::error(message),
        }
    }
}

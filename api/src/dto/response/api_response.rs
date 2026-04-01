use axum::response::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiResponse<T> {
    pub msg: String,
    pub code: i32,
    pub result: Option<T>,
}

impl<T> ApiResponse<T>
where
    T: Serialize,
{
    pub fn success(data: T) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code: 0, msg: "".to_string(), result: Some(data) })
    }

    pub fn error(msg: String) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code: -1, msg, result: None })
    }

    pub fn error_with_code(code: i32, msg: String) -> Json<ApiResponse<T>> {
        Json(ApiResponse { code, msg, result: None })
    }

    pub fn from_result(rst: Result<T, sqlx::Error>) -> Json<ApiResponse<T>> {
        match rst {
            Ok(data) => Self::success(data),
            Err(e) => Self::error(e.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReqCtx {
    pub ori_uri: String,
    pub path: String,
    pub path_params: String,
    pub method: String,
    pub user: UserInfo,
    pub data: String,
}

#[derive(Debug, Clone, Default)]
pub struct UserInfo {
    pub id: String,
    pub token_id: String,
    pub name: String,
    pub tenant_id: String,
}

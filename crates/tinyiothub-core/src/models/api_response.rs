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
    pub tenant_id: String,
}

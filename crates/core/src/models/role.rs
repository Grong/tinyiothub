//! role 写入/请求契约类型（自 db 归位 core — handler 与 repo 共享的值类型）。

use serde::{Deserialize, Serialize};

/// Create role request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub workspace_id: Option<String>,
}

/// Update role request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub workspace_id: Option<String>,
}

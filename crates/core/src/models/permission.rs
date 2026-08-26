//! permission 写入/请求契约类型（自 db 归位 core — handler 与 repo 共享的值类型）。

use serde::{Deserialize, Serialize};

/// Request for creating a new permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePermissionRequest {
    pub name: String,
    pub code: String,
    pub description: Option<String>,
    pub resource_type: String,
    pub action_type: String,
    pub is_system: Option<bool>,
    pub parent_id: Option<String>,
}

/// Request for updating a permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePermissionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub resource_type: Option<String>,
    pub action_type: Option<String>,
    pub parent_id: Option<String>,
}

/// Request for creating a permission group
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePermissionGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub permission_ids: Vec<String>,
}

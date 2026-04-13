use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 角色实体 - 使用现代化 SQLx 实现
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: i32,
    // created_at column doesn't exist in Roles table
    // pub created_at: Option<String>,
}

/// 角色查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RoleQueryParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建角色请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
}

/// 更新角色请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
}

/// 角色统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub struct RoleStats {
    pub total_roles: i64,
    pub admin_roles: i64,
    pub user_roles: i64,
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            description: None,
            is_administrator: 0,
        }
    }
}

// 为了向后兼容，保留旧的 DTO 别名
pub type RoleDto = Role;

use serde::{Deserialize, Serialize};

/// Role entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: i32,
}

/// Role query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RoleQueryParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Create role request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
}

/// Update role request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
}

/// Role statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
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

/// Backward compatibility alias
pub type RoleDto = Role;

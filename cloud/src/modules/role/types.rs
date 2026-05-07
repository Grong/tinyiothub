use serde::{Deserialize, Serialize};

/// Role entity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_administrator: i32,
    pub workspace_id: Option<String>,
}

/// Role query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RoleQueryParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_administrator: Option<i32>,
    pub workspace_id: Option<String>,
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
            workspace_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_default() {
        let role = Role::default();
        assert!(!role.id.is_empty());
        assert!(role.name.is_empty());
        assert_eq!(role.description, None);
        assert_eq!(role.is_administrator, 0);
    }
}

/// Backward compatibility alias
pub type RoleDto = Role;

use serde::{Deserialize, Serialize};

/// Permission entity - 权限实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Permission {
    pub id: String,
    pub name: String,
    pub code: String,
    pub description: Option<String>,
    pub resource_type: String,
    pub action_type: String,
    pub is_system: bool,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Permission group entity - 权限组实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PermissionGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub permissions: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Query parameters for permission search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct PermissionQuery {
    pub name: Option<String>,
    pub code: Option<String>,
    pub resource_type: Option<String>,
    pub action_type: Option<String>,
    pub is_system: Option<bool>,
    pub parent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

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

impl Permission {
    /// Create a new permission
    pub fn new(request: CreatePermissionRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            code: request.code,
            description: request.description,
            resource_type: request.resource_type,
            action_type: request.action_type,
            is_system: request.is_system.unwrap_or(false),
            parent_id: request.parent_id,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Check if this is a system permission
    pub fn is_system_permission(&self) -> bool {
        self.is_system
    }

    /// Check if this is a root permission (no parent)
    pub fn is_root_permission(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Get permission full code
    pub fn get_full_code(&self) -> String {
        format!("{}:{}", self.resource_type, self.action_type)
    }

    /// Check if permission allows action on resource
    pub fn allows_action(&self, resource_type: &str, action_type: &str) -> bool {
        (self.resource_type == resource_type || self.resource_type == "*")
            && (self.action_type == action_type
                || self.action_type == "*"
                || self.action_type == "admin")
    }

    /// Get permission priority
    pub fn get_priority(&self) -> u8 {
        match self.action_type.as_str() {
            "admin" => 10,
            "write" => 8,
            "delete" => 7,
            "execute" => 6,
            "read" => 5,
            _ => 1,
        }
    }
}

impl PermissionGroup {
    /// Create a new permission group
    pub fn new(request: CreatePermissionGroupRequest) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let permissions_json =
            serde_json::to_string(&request.permission_ids).unwrap_or_else(|_| "[]".to_string());

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            description: request.description,
            permissions: permissions_json,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Get permission IDs as vector
    pub fn get_permission_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.permissions).unwrap_or_else(|_| Vec::new())
    }

    /// Add permission to group
    pub fn add_permission(&mut self, permission_id: String) {
        let mut permission_ids = self.get_permission_ids();
        if !permission_ids.contains(&permission_id) {
            permission_ids.push(permission_id);
            self.permissions =
                serde_json::to_string(&permission_ids).unwrap_or_else(|_| "[]".to_string());
            self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Remove permission from group
    pub fn remove_permission(&mut self, permission_id: &str) {
        let mut permission_ids = self.get_permission_ids();
        if let Some(pos) = permission_ids.iter().position(|x| x == permission_id) {
            permission_ids.remove(pos);
            self.permissions =
                serde_json::to_string(&permission_ids).unwrap_or_else(|_| "[]".to_string());
            self.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Check if group contains permission
    pub fn contains_permission(&self, permission_id: &str) -> bool {
        self.get_permission_ids().contains(&permission_id.to_string())
    }
}

// Backward compatibility
pub type PermissionDto = Permission;
pub type PermissionQueryParams = PermissionQuery;

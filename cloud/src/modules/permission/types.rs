use serde::{Deserialize, Serialize};

/// Permission entity
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

/// Permission group entity
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_create_request() -> CreatePermissionRequest {
        CreatePermissionRequest {
            name: "Read Devices".to_string(),
            code: "device:read".to_string(),
            description: Some("Can read device data".to_string()),
            resource_type: "device".to_string(),
            action_type: "read".to_string(),
            is_system: Some(true),
            parent_id: Some("parent-1".to_string()),
        }
    }

    #[test]
    fn test_permission_new() {
        let perm = Permission::new(test_create_request());
        assert_eq!(perm.name, "Read Devices");
        assert_eq!(perm.code, "device:read");
        assert_eq!(perm.resource_type, "device");
        assert_eq!(perm.action_type, "read");
        assert!(perm.is_system);
        assert_eq!(perm.parent_id, Some("parent-1".to_string()));
    }

    #[test]
    fn test_permission_defaults() {
        let req = CreatePermissionRequest { is_system: None, ..test_create_request() };
        let perm = Permission::new(req);
        assert!(!perm.is_system);
    }

    #[test]
    fn test_is_system_permission() {
        let mut perm = Permission::new(test_create_request());
        perm.is_system = true;
        assert!(perm.is_system_permission());
        perm.is_system = false;
        assert!(!perm.is_system_permission());
    }

    #[test]
    fn test_is_root_permission() {
        let mut perm = Permission::new(test_create_request());
        assert!(!perm.is_root_permission());
        perm.parent_id = None;
        assert!(perm.is_root_permission());
    }

    #[test]
    fn test_get_full_code() {
        let perm = Permission::new(test_create_request());
        assert_eq!(perm.get_full_code(), "device:read");
    }

    #[test]
    fn test_allows_action() {
        let perm = Permission::new(test_create_request());
        assert!(perm.allows_action("device", "read"));
        assert!(!perm.allows_action("alarm", "read"));
        assert!(!perm.allows_action("device", "write"));
    }

    #[test]
    fn test_allows_action_wildcard_resource() {
        let req =
            CreatePermissionRequest { resource_type: "*".to_string(), ..test_create_request() };
        let perm = Permission::new(req);
        assert!(perm.allows_action("device", "read"));
        assert!(perm.allows_action("alarm", "read"));
    }

    #[test]
    fn test_allows_action_wildcard_action() {
        let req = CreatePermissionRequest { action_type: "*".to_string(), ..test_create_request() };
        let perm = Permission::new(req);
        assert!(perm.allows_action("device", "read"));
        assert!(perm.allows_action("device", "write"));
    }

    #[test]
    fn test_allows_action_admin() {
        let req =
            CreatePermissionRequest { action_type: "admin".to_string(), ..test_create_request() };
        let perm = Permission::new(req);
        assert!(perm.allows_action("device", "delete"));
    }

    #[test]
    fn test_get_priority() {
        let mut perm = Permission::new(test_create_request());

        perm.action_type = "admin".to_string();
        assert_eq!(perm.get_priority(), 10);

        perm.action_type = "write".to_string();
        assert_eq!(perm.get_priority(), 8);

        perm.action_type = "delete".to_string();
        assert_eq!(perm.get_priority(), 7);

        perm.action_type = "execute".to_string();
        assert_eq!(perm.get_priority(), 6);

        perm.action_type = "read".to_string();
        assert_eq!(perm.get_priority(), 5);

        perm.action_type = "other".to_string();
        assert_eq!(perm.get_priority(), 1);
    }

    #[test]
    fn test_permission_group_new() {
        let req = CreatePermissionGroupRequest {
            name: "Admins".to_string(),
            description: Some("Admin group".to_string()),
            permission_ids: vec!["perm-1".to_string(), "perm-2".to_string()],
        };
        let group = PermissionGroup::new(req);
        assert_eq!(group.name, "Admins");
        assert_eq!(group.get_permission_ids(), vec!["perm-1", "perm-2"]);
    }

    #[test]
    fn test_permission_group_add_remove() {
        let req = CreatePermissionGroupRequest {
            name: "Test".to_string(),
            description: None,
            permission_ids: vec!["perm-1".to_string()],
        };
        let mut group = PermissionGroup::new(req);

        assert!(group.contains_permission("perm-1"));
        assert!(!group.contains_permission("perm-2"));

        group.add_permission("perm-2".to_string());
        assert!(group.contains_permission("perm-2"));

        group.remove_permission("perm-1");
        assert!(!group.contains_permission("perm-1"));
        assert_eq!(group.get_permission_ids(), vec!["perm-2"]);
    }

    #[test]
    fn test_permission_group_add_duplicate() {
        let req = CreatePermissionGroupRequest {
            name: "Test".to_string(),
            description: None,
            permission_ids: vec!["perm-1".to_string()],
        };
        let mut group = PermissionGroup::new(req);
        group.add_permission("perm-1".to_string());
        assert_eq!(group.get_permission_ids().len(), 1);
    }

    #[test]
    fn test_permission_group_get_permission_ids_invalid_json() {
        let mut group = PermissionGroup::new(CreatePermissionGroupRequest {
            name: "Test".to_string(),
            description: None,
            permission_ids: vec![],
        });
        group.permissions = "not json".to_string();
        assert!(group.get_permission_ids().is_empty());
    }
}

/// Backward compatibility aliases
pub type PermissionDto = Permission;
pub type PermissionQueryParams = PermissionQuery;

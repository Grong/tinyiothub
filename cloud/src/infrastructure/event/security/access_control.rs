// Event access control implementations
use std::sync::Arc;

use crate::domain::event::{entities::Event, value_objects::EventType, Result};

/// Access control result
#[derive(Debug, Clone, PartialEq)]
pub enum AccessResult {
    Allow,
    Deny(String),
    Allowed, // For audit log compatibility
    Denied,  // For audit log compatibility
    Error,   // For audit log compatibility
}

/// Access type enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum AccessType {
    Read,
    Create,
    Update,
    Delete,
    Query,     // For audit log compatibility
    Export,    // For audit log compatibility
    Subscribe, // For audit log compatibility
}

/// Event access control trait
#[async_trait::async_trait]
pub trait EventAccessControl: Send + Sync {
    /// Check if user can read an event
    async fn can_read_event(&self, user_id: &str, event: &Event) -> Result<bool>;

    /// Check if user can create an event of given type
    async fn can_create_event(&self, user_id: &str, event_type: &EventType) -> Result<bool>;

    /// Check if user can update an event
    async fn can_update_event(&self, user_id: &str, event: &Event) -> Result<bool>;

    /// Check if user can delete an event
    async fn can_delete_event(&self, user_id: &str, event: &Event) -> Result<bool>;

    /// Get user roles
    async fn get_user_roles(&self, user_id: &str) -> Result<Vec<String>>;

    /// Get user permissions for a resource type
    async fn get_user_permissions(&self, user_id: &str, resource_type: &str)
        -> Result<Vec<String>>;
}

/// Role-based access control implementation
pub struct RoleBasedAccessControl {
    db: Arc<crate::infrastructure::persistence::Database>,
}

impl RoleBasedAccessControl {
    pub fn new(db: Arc<crate::infrastructure::persistence::Database>) -> Self {
        Self { db }
    }

    /// Check if user has required role
    async fn has_role(&self, user_id: &str, required_role: &str) -> Result<bool> {
        let roles = self.get_user_roles(user_id).await?;
        Ok(roles.contains(&required_role.to_string()))
    }

    /// Check if user has required permission
    async fn has_permission(
        &self,
        user_id: &str,
        resource_type: &str,
        permission: &str,
    ) -> Result<bool> {
        let permissions = self.get_user_permissions(user_id, resource_type).await?;
        Ok(permissions.contains(&permission.to_string()))
    }
}

#[async_trait::async_trait]
impl EventAccessControl for RoleBasedAccessControl {
    async fn can_read_event(&self, user_id: &str, event: &Event) -> Result<bool> {
        // Admin can read all events
        if self.has_role(user_id, "admin").await? {
            return Ok(true);
        }

        // Users can read their own events
        if event.source().user_id() == Some(user_id) {
            return Ok(true);
        }

        // Check if user has read permission for this event type
        let event_type_str = match event.event_type() {
            crate::domain::event::value_objects::EventType::Device(device_type) => {
                match device_type {
                    crate::domain::event::value_objects::DeviceEventType::Connection => {
                        "device_connection"
                    }
                    crate::domain::event::value_objects::DeviceEventType::PropertyChange
                    | crate::domain::event::value_objects::DeviceEventType::PropertyAlarm
                    | crate::domain::event::value_objects::DeviceEventType::PropertyNormal => {
                        "device_property"
                    }
                    crate::domain::event::value_objects::DeviceEventType::CommandStarted
                    | crate::domain::event::value_objects::DeviceEventType::CommandCompleted
                    | crate::domain::event::value_objects::DeviceEventType::CommandFailed => {
                        "device_command"
                    }
                    crate::domain::event::value_objects::DeviceEventType::DeviceAlarm
                    | crate::domain::event::value_objects::DeviceEventType::DeviceNormal => {
                        "device_alarm"
                    }
                    crate::domain::event::value_objects::DeviceEventType::DeviceCreated
                    | crate::domain::event::value_objects::DeviceEventType::DeviceUpdated
                    | crate::domain::event::value_objects::DeviceEventType::DeviceDeleted => {
                        "device_lifecycle"
                    }
                }
            }
            crate::domain::event::value_objects::EventType::System(system_type) => {
                match system_type {
                    crate::domain::event::value_objects::SystemEventType::UserAuth => "user_auth",
                    crate::domain::event::value_objects::SystemEventType::UserOperation => {
                        "user_operation"
                    }
                    crate::domain::event::value_objects::SystemEventType::SystemConfig => {
                        "system_config"
                    }
                    crate::domain::event::value_objects::SystemEventType::SystemError => {
                        "system_error"
                    }
                }
            }
        };

        self.has_permission(user_id, event_type_str, "read").await
    }

    async fn can_create_event(&self, user_id: &str, event_type: &EventType) -> Result<bool> {
        // Admin can create all events
        if self.has_role(user_id, "admin").await? {
            return Ok(true);
        }

        // Check specific permissions based on event type
        let event_type_str = match event_type {
            crate::domain::event::value_objects::EventType::Device(device_type) => {
                match device_type {
                    crate::domain::event::value_objects::DeviceEventType::Connection => {
                        "device_connection"
                    }
                    crate::domain::event::value_objects::DeviceEventType::PropertyChange
                    | crate::domain::event::value_objects::DeviceEventType::PropertyAlarm
                    | crate::domain::event::value_objects::DeviceEventType::PropertyNormal => {
                        "device_property"
                    }
                    crate::domain::event::value_objects::DeviceEventType::CommandStarted
                    | crate::domain::event::value_objects::DeviceEventType::CommandCompleted
                    | crate::domain::event::value_objects::DeviceEventType::CommandFailed => {
                        "device_command"
                    }
                    crate::domain::event::value_objects::DeviceEventType::DeviceAlarm
                    | crate::domain::event::value_objects::DeviceEventType::DeviceNormal => {
                        "device_alarm"
                    }
                    crate::domain::event::value_objects::DeviceEventType::DeviceCreated
                    | crate::domain::event::value_objects::DeviceEventType::DeviceUpdated
                    | crate::domain::event::value_objects::DeviceEventType::DeviceDeleted => {
                        "device_lifecycle"
                    }
                }
            }
            crate::domain::event::value_objects::EventType::System(system_type) => {
                match system_type {
                    crate::domain::event::value_objects::SystemEventType::UserAuth => "user_auth",
                    crate::domain::event::value_objects::SystemEventType::UserOperation => {
                        "user_operation"
                    }
                    crate::domain::event::value_objects::SystemEventType::SystemConfig => {
                        "system_config"
                    }
                    crate::domain::event::value_objects::SystemEventType::SystemError => {
                        "system_error"
                    }
                }
            }
        };

        self.has_permission(user_id, event_type_str, "create").await
    }

    async fn can_update_event(&self, user_id: &str, _event: &Event) -> Result<bool> {
        // Admin can update all events
        if self.has_role(user_id, "admin").await? {
            return Ok(true);
        }

        // Generally, events should not be updated after creation
        // Only allow in special cases with proper permissions
        Ok(false)
    }

    async fn can_delete_event(&self, user_id: &str, _event: &Event) -> Result<bool> {
        // Only admin can delete events, and only in special cases
        if self.has_role(user_id, "admin").await? {
            // Additional checks could be added here
            return Ok(true);
        }

        Ok(false)
    }

    async fn get_user_roles(&self, user_id: &str) -> Result<Vec<String>> {
        // Query user roles from database
        let query = "SELECT role_name FROM user_roles WHERE user_id = ? AND is_active = 1";

        match sqlx::query_scalar::<_, String>(query).bind(user_id).fetch_all(self.db.pool()).await {
            Ok(roles) => {
                if roles.is_empty() {
                    // Default role for authenticated users
                    Ok(vec!["user".to_string()])
                } else {
                    Ok(roles)
                }
            }
            Err(sqlx::Error::RowNotFound) => {
                // User not found, return default role
                Ok(vec!["user".to_string()])
            }
            Err(e) => {
                tracing::error!("Failed to get user roles for {}: {}", user_id, e);
                // Fallback to default role on database error
                Ok(vec!["user".to_string()])
            }
        }
    }

    async fn get_user_permissions(
        &self,
        user_id: &str,
        resource_type: &str,
    ) -> Result<Vec<String>> {
        let roles = self.get_user_roles(user_id).await?;

        let mut permissions = Vec::new();

        // Query user-specific permissions from database
        let user_permissions_query = "SELECT permission FROM user_permissions WHERE user_id = ? AND resource_type = ? AND is_active = 1";

        match sqlx::query_scalar::<_, String>(user_permissions_query)
            .bind(user_id)
            .bind(resource_type)
            .fetch_all(self.db.pool())
            .await
        {
            Ok(user_permissions) => {
                permissions.extend(user_permissions);
            }
            Err(sqlx::Error::RowNotFound) => {
                // No user-specific permissions, continue with role-based permissions
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to get user permissions for {} on {}: {}",
                    user_id,
                    resource_type,
                    e
                );
                // Continue with role-based permissions as fallback
            }
        }

        // Add role-based permissions from the role_permissions table
        for role in roles {
            let role_permissions_query = "SELECT permission FROM role_permissions WHERE role_name = ? AND resource_type = ? AND is_active = 1";

            match sqlx::query_scalar::<_, String>(role_permissions_query)
                .bind(&role)
                .bind(resource_type)
                .fetch_all(self.db.pool())
                .await
            {
                Ok(role_permissions) => {
                    permissions.extend(role_permissions);
                }
                Err(sqlx::Error::RowNotFound) => {
                    // No permissions for this role and resource type
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get role permissions for role {} on {}: {}",
                        role,
                        resource_type,
                        e
                    );
                }
            }
        }

        // Remove duplicates and sort
        permissions.sort();
        permissions.dedup();

        Ok(permissions)
    }
}

impl AccessType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessType::Read => "read",
            AccessType::Create => "create",
            AccessType::Update => "update",
            AccessType::Delete => "delete",
            AccessType::Query => "query",
            AccessType::Export => "export",
            AccessType::Subscribe => "subscribe",
        }
    }
}

impl AccessResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessResult::Allow => "allowed",
            AccessResult::Deny(_) => "denied",
            AccessResult::Allowed => "allowed",
            AccessResult::Denied => "denied",
            AccessResult::Error => "error",
        }
    }
}

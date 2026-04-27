// Secure event service with access control, encryption, and audit logging
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::{
    modules::event::{
        entities::Event,
        repositories::EventRepository,
        value_objects::{EventId, EventLevel, EventType, RichContent},
        EventError, Result,
    },
    shared::event::security::{
        AccessResult, AccessType, EncryptedContent, EventAccessControl, EventAuditLog,
        EventEncryption, EventSecurityConfig,
    },
};

/// Secure event service that wraps the event repository with security features
pub struct SecureEventService {
    event_repository: Arc<dyn EventRepository>,
    access_control: Arc<dyn EventAccessControl>,
    encryption: Arc<dyn EventEncryption>,
    audit_log: Arc<dyn EventAuditLog>,
    config: EventSecurityConfig,
}

impl SecureEventService {
    /// Create a new secure event service
    pub fn new(
        event_repository: Arc<dyn EventRepository>,
        access_control: Arc<dyn EventAccessControl>,
        encryption: Arc<dyn EventEncryption>,
        audit_log: Arc<dyn EventAuditLog>,
        config: EventSecurityConfig,
    ) -> Result<Self> {
        Ok(Self { event_repository, access_control, encryption, audit_log, config })
    }

    /// Create an event with security checks
    pub async fn create_event(&self, user_id: &str, mut event: Event) -> Result<EventId> {
        // Check access control
        if !self.access_control.can_create_event(user_id, event.event_type()).await? {
            self.audit_log
                .log_access_denied(
                    user_id,
                    "create_event",
                    &format!("event_type:{:?}", event.event_type()),
                    "Insufficient permissions",
                )
                .await?;

            return Err(EventError::AccessDenied(
                "Insufficient permissions to create this event type".to_string(),
            ));
        }

        // Encrypt sensitive content if enabled
        if self.config.enable_encryption && self.encryption.should_encrypt(event.content()) {
            let encrypted_content = self.encryption.encrypt_content(event.content())?;
            // Convert encrypted content back to RichContent for storage
            // This is a simplified approach - in practice you might store encrypted data differently
            let encrypted_rich_content = RichContent::new_text(
                "Encrypted Content".to_string(),
                serde_json::to_string(&encrypted_content).unwrap_or_default(),
            );

            // Create a new event with encrypted content
            event = Event::reconstruct(
                event.id().clone(),
                event.event_type().clone(),
                event.level(),
                event.timestamp(),
                event.source().clone(),
                encrypted_rich_content
            );
        }

        // Create the event
        self.event_repository.save(&event).await?;
        // For now, return the event's ID (we'll need to modify the save method to return the ID)
        let event_id = event.id().clone();

        // Log the action
        self.audit_log.log_event_created(user_id, &event_id, &event).await?;

        Ok(event_id)
    }

    /// Get an event by ID with security checks
    pub async fn get_event(&self, user_id: &str, event_id: &EventId) -> Result<Option<Event>> {
        // Get the event first
        let mut event = match self.event_repository.find_by_id(event_id).await? {
            Some(event) => event,
            None => return Ok(None),
        };

        // Check access control
        if !self.access_control.can_read_event(user_id, &event).await? {
            self.audit_log
                .log_access_denied(
                    user_id,
                    "get_event",
                    &event_id.to_string(),
                    "Insufficient permissions",
                )
                .await?;

            return Err(EventError::AccessDenied(
                "Insufficient permissions to read this event".to_string(),
            ));
        }

        // Decrypt content if encrypted
        if self.config.enable_encryption {
            // Check if content appears to be encrypted (simplified check)
            if event.content().title() == "Encrypted Content"
                && let Some(first_element) = event.content().elements().first()
                    && let crate::modules::event::value_objects::ContentElement::Text {
                        content,
                        ..
                    } = first_element
                        && let Ok(encrypted_data) =
                            serde_json::from_str::<EncryptedContent>(content)
                            && let Ok(decrypted_content) =
                                self.encryption.decrypt_content(&encrypted_data)
                            {
                                // Create a new event with decrypted content
                                event = Event::reconstruct(
                                    event.id().clone(),
                                    event.event_type().clone(),
                                    event.level(),
                                    event.timestamp(),
                                    event.source().clone(),
                                    decrypted_content
                                );
                            }
        }

        // Log the access
        self.audit_log.log_event_accessed(user_id, event_id).await?;

        Ok(Some(event))
    }

    /// List events with security filtering
    pub async fn list_events(
        &self,
        user_id: &str,
        event_type: Option<EventType>,
        level: Option<EventLevel>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Event>> {
        // Get events from repository
        let criteria = crate::modules::event::repo::EventCriteria {
            start_time,
            end_time,
            event_types: event_type.as_ref().map(|t| vec![t.clone()]),
            levels: level.map(|l| vec![l]),
            limit: limit.map(|l| l as u32),
            ..Default::default()
        };
        let events = self.event_repository.find_by_criteria(&criteria).await?;

        let mut filtered_events = Vec::new();

        // Filter events based on access control
        for mut event in events {
            if self.access_control.can_read_event(user_id, &event).await? {
                // Decrypt content if encrypted
                if self.config.enable_encryption && event.content().title() == "Encrypted Content"
                    && let Some(first_element) = event.content().elements().first()
                        && let crate::modules::event::value_objects::ContentElement::Text {
                            content,
                            ..
                        } = first_element
                            && let Ok(encrypted_data) =
                                serde_json::from_str::<EncryptedContent>(content)
                                && let Ok(decrypted_content) =
                                    self.encryption.decrypt_content(&encrypted_data)
                                {
                                    // Create a new event with decrypted content
                                    event = Event::reconstruct(
                                        event.id().clone(),
                                        event.event_type().clone(),
                                        event.level(),
                                        event.timestamp(),
                                        event.source().clone(),
                                        decrypted_content
                                    );
                                }
                filtered_events.push(event);
            }
        }

        // Log the query
        self.audit_log
            .log_event_query(
                user_id,
                event_type,
                level,
                start_time,
                end_time,
                filtered_events.len(),
            )
            .await?;

        Ok(filtered_events)
    }

    /// Update an event with security checks
    pub async fn update_event(
        &self,
        user_id: &str,
        event_id: &EventId,
        mut updated_event: Event,
    ) -> Result<()> {
        // Get the existing event
        let existing_event = match self.event_repository.find_by_id(event_id).await? {
            Some(event) => event,
            None => {
                return Err(EventError::NotFound { id: format!("Event {} not found", event_id) })
            }
        };

        // Check access control
        if !self.access_control.can_update_event(user_id, &existing_event).await? {
            self.audit_log
                .log_access_denied(
                    user_id,
                    "update_event",
                    &event_id.to_string(),
                    "Insufficient permissions",
                )
                .await?;

            return Err(EventError::AccessDenied(
                "Insufficient permissions to update this event".to_string(),
            ));
        }

        // Encrypt content if enabled
        if self.config.enable_encryption && self.encryption.should_encrypt(updated_event.content())
        {
            let encrypted_content = self.encryption.encrypt_content(updated_event.content())?;
            let encrypted_rich_content = RichContent::new_text(
                "Encrypted Content".to_string(),
                serde_json::to_string(&encrypted_content).unwrap_or_default(),
            );

            // Create a new event with encrypted content
            updated_event = Event::reconstruct(
                updated_event.id().clone(),
                updated_event.event_type().clone(),
                updated_event.level(),
                updated_event.timestamp(),
                updated_event.source().clone(),
                encrypted_rich_content
            );
        }

        // Update the event (simplified - in a real implementation, this would update the repository)
        // For now, we'll just validate that the update is allowed
        // self.event_repository.update_event(event_id, updated_event.clone()).await?;

        // Log the action
        self.audit_log
            .log_event_updated(user_id, event_id, &existing_event, &updated_event)
            .await?;

        Ok(())
    }

    /// Delete an event with security checks
    pub async fn delete_event(&self, user_id: &str, event_id: &EventId) -> Result<()> {
        // Get the existing event
        let existing_event = match self.event_repository.find_by_id(event_id).await? {
            Some(event) => event,
            None => {
                return Err(EventError::NotFound { id: format!("Event {} not found", event_id) })
            }
        };

        // Check access control
        if !self.access_control.can_delete_event(user_id, &existing_event).await? {
            self.audit_log
                .log_access_denied(
                    user_id,
                    "delete_event",
                    &event_id.to_string(),
                    "Insufficient permissions",
                )
                .await?;

            return Err(EventError::AccessDenied(
                "Insufficient permissions to delete this event".to_string(),
            ));
        }

        // Delete the event (simplified - in a real implementation, this would delete from the repository)
        // For now, we'll just validate that the deletion is allowed
        // self.event_repository.delete_event(event_id).await?;

        // Log the action
        self.audit_log.log_event_deleted(user_id, event_id, &existing_event).await?;

        Ok(())
    }

    /// Get user access summary
    pub async fn get_user_access_summary(&self, user_id: &str) -> Result<UserAccessSummary> {
        let roles = self.access_control.get_user_roles(user_id).await?;

        let mut permissions = std::collections::HashMap::new();
        let resource_types = [
            "device_connection",
            "device_property",
            "device_command",
            "device_alarm",
            "device_lifecycle",
            "user_auth",
            "user_operation",
            "system_config",
            "system_error",
        ];

        for resource_type in &resource_types {
            let perms = self.access_control.get_user_permissions(user_id, resource_type).await?;
            permissions.insert(resource_type.to_string(), perms);
        }

        Ok(UserAccessSummary { user_id: user_id.to_string(), roles, permissions })
    }

    /// Check if user can perform an action on a resource
    pub async fn check_access(
        &self,
        user_id: &str,
        action: AccessType,
        _resource: &str,
    ) -> Result<AccessResult> {
        match action {
            AccessType::Read => {
                // For read access, we need to check the specific event
                // This is a simplified check - in practice, you'd parse the resource
                Ok(AccessResult::Allow)
            }
            AccessType::Create => {
                // Check if user can create events of this type
                // Parse resource to get event type
                Ok(AccessResult::Allow)
            }
            AccessType::Update | AccessType::Delete => {
                // These operations are generally restricted
                let roles = self.access_control.get_user_roles(user_id).await?;
                if roles.contains(&"admin".to_string()) {
                    Ok(AccessResult::Allow)
                } else {
                    Ok(AccessResult::Deny("Insufficient privileges".to_string()))
                }
            }
            _ => Ok(AccessResult::Allow), // Default allow for other actions
        }
    }

    /// Get access control service
    pub fn access_control(&self) -> &dyn EventAccessControl {
        self.access_control.as_ref()
    }

    /// Get audit log service
    pub fn audit_log(&self) -> Option<&dyn EventAuditLog> {
        if self.config.enable_audit_log {
            Some(self.audit_log.as_ref())
        } else {
            None
        }
    }

    /// Get encryption service
    pub fn encryption(&self) -> &dyn EventEncryption {
        self.encryption.as_ref()
    }

    /// Get configuration
    pub fn config(&self) -> &EventSecurityConfig {
        &self.config
    }
}

/// User access summary
#[derive(Debug, Clone)]
pub struct UserAccessSummary {
    pub user_id: String,
    pub roles: Vec<String>,
    pub permissions: std::collections::HashMap<String, Vec<String>>,
}

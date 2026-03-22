// Event security configuration and factory
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    domain::event::{repositories::EventRepository, EventError, Result},
    infrastructure::event::security::{
        AesEventEncryption, DatabaseAuditLog, EventAccessControl, EventAuditLog, EventEncryption,
        InMemoryAuditLog, NoOpEncryption, RoleBasedAccessControl, SecureEventService,
    },
};

/// Event security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSecurityConfig {
    pub enable_rbac: bool,
    pub enable_encryption: bool,
    pub enable_audit_log: bool,
    pub encryption_key: Option<String>,
    pub audit_retention_days: u32,
}

impl Default for EventSecurityConfig {
    fn default() -> Self {
        Self {
            enable_rbac: true,
            enable_encryption: true,
            enable_audit_log: true,
            encryption_key: None,
            audit_retention_days: 90,
        }
    }
}

impl EventSecurityConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.enable_encryption && self.encryption_key.is_none() {
            return Err(EventError::Configuration(
                "Encryption enabled but no encryption key provided".to_string(),
            ));
        }

        if self.audit_retention_days == 0 {
            return Err(EventError::Configuration(
                "Audit retention days must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Event security factory for creating and configuring security components
pub struct EventSecurityFactory {
    db: Arc<crate::infrastructure::persistence::Database>,
    config: EventSecurityConfig,
}

/// Security component bundle
pub struct SecurityComponents {
    pub access_control: Arc<dyn EventAccessControl>,
    pub encryption: Arc<dyn EventEncryption>,
    pub audit_log: Arc<dyn EventAuditLog>,
}

impl EventSecurityFactory {
    /// Create a new security factory
    pub fn new(
        db: Arc<crate::infrastructure::persistence::Database>,
        config: EventSecurityConfig,
    ) -> Result<Self> {
        config.validate()?;

        Ok(Self { db, config })
    }

    /// Create security components based on configuration
    pub async fn create_security_components(&self) -> Result<SecurityComponents> {
        // Create access control service
        let access_control: Arc<dyn EventAccessControl> = if self.config.enable_rbac {
            Arc::new(RoleBasedAccessControl::new(self.db.clone()))
        } else {
            Arc::new(NoOpAccessControl)
        };

        // Create encryption service
        let encryption: Arc<dyn EventEncryption> = if self.config.enable_encryption {
            if let Some(key) = &self.config.encryption_key {
                Arc::new(AesEventEncryption::from_base64_key(key)?)
            } else {
                return Err(EventError::Configuration(
                    "Encryption enabled but no key provided".to_string(),
                ));
            }
        } else {
            Arc::new(NoOpEncryption)
        };

        // Create audit log service
        let audit_log: Arc<dyn EventAuditLog> = if self.config.enable_audit_log {
            let db_audit_log = DatabaseAuditLog::new(self.db.clone());
            db_audit_log.initialize().await?;
            Arc::new(db_audit_log)
        } else {
            Arc::new(InMemoryAuditLog::new())
        };

        Ok(SecurityComponents { access_control, encryption, audit_log })
    }

    /// Create a secure event service with all security components
    pub async fn create_secure_event_service(
        &self,
        event_repository: Arc<dyn EventRepository>,
    ) -> Result<SecureEventService> {
        let components = self.create_security_components().await?;

        SecureEventService::new(
            event_repository,
            components.access_control,
            components.encryption,
            components.audit_log,
            self.config.clone(),
        )
    }

    /// Create security configuration from environment variables
    pub fn from_env() -> Result<EventSecurityConfig> {
        let mut config = EventSecurityConfig::default();

        // Read configuration from environment variables
        if let Ok(rbac_enabled) = std::env::var("EVENT_RBAC_ENABLED") {
            config.enable_rbac = rbac_enabled.parse().unwrap_or(true);
        }

        if let Ok(encryption_enabled) = std::env::var("EVENT_ENCRYPTION_ENABLED") {
            config.enable_encryption = encryption_enabled.parse().unwrap_or(true);
        }

        if let Ok(audit_enabled) = std::env::var("EVENT_AUDIT_ENABLED") {
            config.enable_audit_log = audit_enabled.parse().unwrap_or(true);
        }

        if let Ok(encryption_key) = std::env::var("EVENT_ENCRYPTION_KEY") {
            config.encryption_key = Some(encryption_key);
        }

        if let Ok(retention_days) = std::env::var("EVENT_AUDIT_RETENTION_DAYS") {
            config.audit_retention_days = retention_days.parse().unwrap_or(90);
        }

        // Generate encryption key if needed and not provided
        if config.enable_encryption && config.encryption_key.is_none() {
            let key = AesEventEncryption::generate_key();
            let key_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key);
            config.encryption_key = Some(key_base64);

            tracing::warn!(
                "Generated new encryption key. Save this key: {}",
                config.encryption_key.as_ref().unwrap()
            );
        }

        config.validate()?;
        Ok(config)
    }
}

/// No-op access control implementation for when RBAC is disabled
struct NoOpAccessControl;

#[async_trait::async_trait]
impl EventAccessControl for NoOpAccessControl {
    async fn can_read_event(
        &self,
        _user_id: &str,
        _event: &crate::domain::event::entities::Event,
    ) -> Result<bool> {
        Ok(true) // Allow all access when RBAC is disabled
    }

    async fn can_create_event(
        &self,
        _user_id: &str,
        _event_type: &crate::domain::event::value_objects::EventType,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn can_update_event(
        &self,
        _user_id: &str,
        _event: &crate::domain::event::entities::Event,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn can_delete_event(
        &self,
        _user_id: &str,
        _event: &crate::domain::event::entities::Event,
    ) -> Result<bool> {
        Ok(false) // Generally don't allow deletion even without RBAC
    }

    async fn get_user_roles(&self, _user_id: &str) -> Result<Vec<String>> {
        Ok(vec!["user".to_string()])
    }

    async fn get_user_permissions(
        &self,
        _user_id: &str,
        _resource_type: &str,
    ) -> Result<Vec<String>> {
        Ok(vec!["read".to_string(), "create".to_string(), "update".to_string()])
    }
}

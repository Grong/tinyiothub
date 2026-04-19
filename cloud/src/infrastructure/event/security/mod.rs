// Event security module with access control, encryption, and audit logging

// Module declarations
pub mod access_control;
pub mod audit_log;
pub mod config;
pub mod encryption;
pub mod secure_event_service;

// Re-export security types
pub use access_control::{AccessResult, AccessType, EventAccessControl, RoleBasedAccessControl};
pub use audit_log::{AuditLogEntry, DatabaseAuditLog, EventAuditLog, InMemoryAuditLog};
pub use config::{EventSecurityConfig, EventSecurityFactory};
pub use encryption::{AesEventEncryption, EncryptedContent, EventEncryption, NoOpEncryption};
pub use secure_event_service::SecureEventService;

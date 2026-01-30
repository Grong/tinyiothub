# Event Security Module

This module provides comprehensive security features for the event system, including access control, encryption, and audit logging.

## Components

### 1. Access Control (`access_control.rs`)

Provides role-based access control (RBAC) for event operations:

- **EventAccessControl trait**: Defines the interface for access control
- **RoleBasedAccessControl**: Implementation using user roles and permissions
- **AccessType**: Enumeration of access types (Read, Create, Update, Delete, etc.)
- **AccessResult**: Result of access control checks

#### Features:
- User role management
- Permission-based access control
- Event-type specific permissions
- User ownership checks

### 2. Audit Logging (`audit_log.rs`)

Comprehensive audit logging for security and compliance:

- **EventAuditLog trait**: Interface for audit logging
- **DatabaseAuditLog**: Database-backed audit log implementation
- **InMemoryAuditLog**: In-memory audit log for testing
- **AuditLogEntry**: Structured audit log entries

#### Features:
- Event creation/access/modification/deletion logging
- Access denial logging
- User activity tracking
- Configurable retention policies

### 3. Encryption (`encryption.rs`)

Content encryption for sensitive event data:

- **EventEncryption trait**: Interface for content encryption
- **AesEventEncryption**: AES-256-GCM encryption implementation
- **NoOpEncryption**: No-op implementation for testing/disabled encryption
- **EncryptedContent**: Container for encrypted data with metadata

#### Features:
- AES-256-GCM encryption
- Automatic sensitive content detection
- Content integrity verification
- Configurable encryption rules

### 4. Secure Event Service (`secure_event_service.rs`)

Main service that orchestrates all security features:

- **SecureEventService**: Wrapper around EventRepository with security
- **UserAccessSummary**: User access rights summary

#### Features:
- Transparent security layer over event operations
- Automatic encryption/decryption
- Access control enforcement
- Comprehensive audit logging

### 5. Configuration (`config.rs`)

Security configuration and factory:

- **EventSecurityConfig**: Configuration for all security features
- **EventSecurityFactory**: Factory for creating security components
- **SecurityComponents**: Bundle of security services

#### Features:
- Environment-based configuration
- Automatic key generation
- Component lifecycle management
- Configuration validation

## Usage

### Basic Setup

```rust
use crate::infrastructure::event::security::{
    EventSecurityFactory, EventSecurityConfig
};

// Create configuration
let config = EventSecurityConfig::from_env()?;

// Create security factory
let factory = EventSecurityFactory::new(db.clone(), config)?;

// Create secure event service
let secure_service = factory.create_secure_event_service(event_repository).await?;
```

### Creating Events with Security

```rust
// Create an event with automatic security checks
let event = Event::new(
    EventType::System(SystemEventType::UserAuth),
    EventLevel::Info,
    EventSource::user("user123".to_string()),
    RichContent::new_text("User login".to_string(), "User logged in successfully".to_string()),
)?;

let event_id = secure_service.create_event("user123", event).await?;
```

### Querying Events with Access Control

```rust
// List events with automatic access filtering
let events = secure_service.list_events(
    "user123",
    Some(EventType::System(SystemEventType::UserAuth)),
    Some(EventLevel::Info),
    None, // start_time
    None, // end_time
    Some(10), // limit
).await?;
```

### Access Control Checks

```rust
// Check user access
let access_summary = secure_service.get_user_access_summary("user123").await?;
println!("User roles: {:?}", access_summary.roles);
println!("User permissions: {:?}", access_summary.permissions);
```

## Configuration

### Environment Variables

- `EVENT_RBAC_ENABLED`: Enable/disable role-based access control
- `EVENT_ENCRYPTION_ENABLED`: Enable/disable content encryption
- `EVENT_AUDIT_ENABLED`: Enable/disable audit logging
- `EVENT_ENCRYPTION_KEY`: Base64-encoded encryption key
- `EVENT_AUDIT_RETENTION_DAYS`: Audit log retention period

### Configuration File

```toml
[event_security]
enable_rbac = true
enable_encryption = true
enable_audit_log = true
encryption_key = "base64-encoded-key"
audit_retention_days = 90
```

## Security Features

### Access Control
- Role-based permissions
- Event type restrictions
- User ownership validation
- Administrative overrides

### Encryption
- AES-256-GCM encryption
- Automatic sensitive content detection
- Content integrity verification
- Key rotation support

### Audit Logging
- Comprehensive activity logging
- Access denial tracking
- User session correlation
- Configurable retention

## Testing

The module includes comprehensive tests covering:

- Configuration validation
- Access control logic
- Encryption/decryption roundtrips
- Audit log functionality
- Integration scenarios

Run tests with:
```bash
cargo test infrastructure::event::security
```

## Performance Considerations

- Encryption adds computational overhead
- Audit logging requires database writes
- Access control checks add latency
- Consider caching for frequently accessed permissions

## Security Best Practices

1. **Key Management**: Store encryption keys securely
2. **Access Control**: Follow principle of least privilege
3. **Audit Logs**: Monitor for suspicious activity
4. **Configuration**: Validate all security settings
5. **Updates**: Keep encryption algorithms current

## Compliance

This module supports compliance with:
- GDPR (data protection and audit trails)
- SOX (audit logging and access control)
- HIPAA (encryption and access control)
- ISO 27001 (comprehensive security controls)
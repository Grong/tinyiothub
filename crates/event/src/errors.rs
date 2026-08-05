/// Domain-specific errors for the event system
///
/// These errors represent business rule violations and domain-specific failures.
/// They are separate from infrastructure errors and provide clear business context.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventDomainError {
    #[error("Event validation failed: {message}")]
    ValidationFailed { message: String },

    #[error("Event not found: {id}")]
    EventNotFound { id: String },

    #[error("Event cannot be modified: {reason}")]
    EventImmutable { reason: String },

    #[error("Event content is invalid: {details}")]
    InvalidContent { details: String },

    #[error("Event source is invalid: {0}")]
    InvalidSource(String),

    #[error("Event type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Event level is inappropriate for type: {event_type} cannot have level {level}")]
    InappropriateLevel { event_type: String, level: String },

    #[error("Event timestamp is invalid: {reason}")]
    InvalidTimestamp { reason: String },
}

#[derive(Debug, Error)]
pub enum NotificationDomainError {
    #[error("Notification rule validation failed: {message}")]
    RuleValidationFailed { message: String },

    #[error("Notification rule not found: {id}")]
    RuleNotFound { id: String },

    #[error("Notification record not found: {id}")]
    RecordNotFound { id: String },

    #[error("Invalid notification channel: {channel}")]
    InvalidChannel { channel: String },

    #[error("Invalid recipient format: {recipient} for channel {channel}")]
    InvalidRecipient { recipient: String, channel: String },

    #[error("Notification delivery failed: {reason}")]
    DeliveryFailed { reason: String },

    #[error("Notification already processed: {id}")]
    AlreadyProcessed { id: String },

    #[error("Notification retry limit exceeded: {id}")]
    RetryLimitExceeded { id: String },

    #[error("Notification channel unavailable: {channel}")]
    ChannelUnavailable { channel: String },

    #[error("Notification suppressed by rate limiting: rule {rule_id}")]
    RateLimited { rule_id: String },
}

#[derive(Debug, Error)]
pub enum EventServiceDomainError {
    #[error("Event service not initialized")]
    NotInitialized,

    #[error("Event processing failed: {reason}")]
    ProcessingFailed { reason: String },

    #[error("Event storage failed: {reason}")]
    StorageFailed { reason: String },

    #[error("Event query failed: {reason}")]
    QueryFailed { reason: String },

    #[error("Event subscription failed: {reason}")]
    SubscriptionFailed { reason: String },

    #[error("Event bus error: {message}")]
    EventBusError { message: String },

    #[error("Concurrent modification detected: version {expected} vs {actual}")]
    ConcurrentModification { expected: u64, actual: u64 },

    #[error("Service capacity exceeded: {current}/{max}")]
    CapacityExceeded { current: usize, max: usize },
}

#[derive(Debug, Error)]
pub enum PerformanceDomainError {
    #[error("Performance threshold exceeded: {metric} = {value} > {threshold}")]
    ThresholdExceeded { metric: String, value: f64, threshold: f64 },

    #[error("Performance monitoring failed: {reason}")]
    MonitoringFailed { reason: String },

    #[error("Load balancer error: {message}")]
    LoadBalancerError { message: String },

    #[error("Performance optimization failed: {reason}")]
    OptimizationFailed { reason: String },

    #[error("Metrics collection failed: {reason}")]
    MetricsCollectionFailed { reason: String },
}

#[derive(Debug, Error)]
pub enum SecurityDomainError {
    #[error("Access denied: {operation} on {resource}")]
    AccessDenied { operation: String, resource: String },

    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    #[error("Authorization failed: {reason}")]
    AuthorizationFailed { reason: String },

    #[error("Encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    #[error("Audit log error: {message}")]
    AuditLogError { message: String },

    #[error("Security policy violation: {policy}")]
    PolicyViolation { policy: String },

    #[error("Invalid security configuration: {details}")]
    InvalidConfiguration { details: String },
}

/// Unified domain error type for the event system
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Event error: {0}")]
    Event(#[from] EventDomainError),

    #[error("Notification error: {0}")]
    Notification(#[from] NotificationDomainError),

    #[error("Event service error: {0}")]
    EventService(#[from] EventServiceDomainError),

    #[error("Performance error: {0}")]
    Performance(#[from] PerformanceDomainError),

    #[error("Security error: {0}")]
    Security(#[from] SecurityDomainError),
}

/// Domain result type
pub type DomainResult<T> = std::result::Result<T, DomainError>;

/// Helper functions for creating domain errors
impl EventDomainError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::ValidationFailed { message: message.into() }
    }

    pub fn not_found(id: impl Into<String>) -> Self {
        Self::EventNotFound { id: id.into() }
    }

    pub fn immutable(reason: impl Into<String>) -> Self {
        Self::EventImmutable { reason: reason.into() }
    }

    pub fn invalid_content(details: impl Into<String>) -> Self {
        Self::InvalidContent { details: details.into() }
    }

    pub fn invalid_source(source: impl Into<String>) -> Self {
        Self::InvalidSource(source.into())
    }

    pub fn type_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::TypeMismatch { expected: expected.into(), actual: actual.into() }
    }
}

impl NotificationDomainError {
    pub fn rule_validation(message: impl Into<String>) -> Self {
        Self::RuleValidationFailed { message: message.into() }
    }

    pub fn rule_not_found(id: impl Into<String>) -> Self {
        Self::RuleNotFound { id: id.into() }
    }

    pub fn record_not_found(id: impl Into<String>) -> Self {
        Self::RecordNotFound { id: id.into() }
    }

    pub fn delivery_failed(reason: impl Into<String>) -> Self {
        Self::DeliveryFailed { reason: reason.into() }
    }

    pub fn invalid_recipient(recipient: impl Into<String>, channel: impl Into<String>) -> Self {
        Self::InvalidRecipient { recipient: recipient.into(), channel: channel.into() }
    }
}

impl EventServiceDomainError {
    pub fn processing_failed(reason: impl Into<String>) -> Self {
        Self::ProcessingFailed { reason: reason.into() }
    }

    pub fn storage_failed(reason: impl Into<String>) -> Self {
        Self::StorageFailed { reason: reason.into() }
    }

    pub fn query_failed(reason: impl Into<String>) -> Self {
        Self::QueryFailed { reason: reason.into() }
    }

    pub fn concurrent_modification(expected: u64, actual: u64) -> Self {
        Self::ConcurrentModification { expected, actual }
    }
}

/// Conversion from domain errors to the main event system error type
impl From<DomainError> for super::EventError {
    fn from(domain_error: DomainError) -> Self {
        match domain_error {
            DomainError::Event(e) => Self::Validation { message: e.to_string() },
            DomainError::Notification(e) => Self::Notification(e.to_string()),
            DomainError::EventService(e) => Self::Validation { message: e.to_string() },
            DomainError::Performance(e) => Self::Configuration(e.to_string()),
            DomainError::Security(e) => Self::PermissionDenied { operation: e.to_string() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_domain_errors() {
        let error = EventDomainError::validation("Test validation error");
        assert!(error.to_string().contains("Test validation error"));

        let error = EventDomainError::not_found("event-123");
        assert!(error.to_string().contains("event-123"));

        let error = EventDomainError::type_mismatch("System", "Device");
        assert!(error.to_string().contains("System"));
        assert!(error.to_string().contains("Device"));
    }

    #[test]
    fn test_notification_domain_errors() {
        let error = NotificationDomainError::rule_validation("Invalid rule");
        assert!(error.to_string().contains("Invalid rule"));

        let error = NotificationDomainError::delivery_failed("Network timeout");
        assert!(error.to_string().contains("Network timeout"));

        let error = NotificationDomainError::invalid_recipient("invalid-email", "email");
        assert!(error.to_string().contains("invalid-email"));
        assert!(error.to_string().contains("email"));
    }

    #[test]
    fn test_unified_domain_error() {
        let event_error = EventDomainError::validation("Test error");
        let domain_error = DomainError::Event(event_error);
        assert!(domain_error.to_string().contains("Event error"));

        let notification_error = NotificationDomainError::delivery_failed("Test failure");
        let domain_error = DomainError::Notification(notification_error);
        assert!(domain_error.to_string().contains("Notification error"));
    }

    #[test]
    fn test_domain_error_conversion() {
        let domain_error = DomainError::Event(EventDomainError::validation("Test"));
        let event_error: super::super::EventError = domain_error.into();

        match event_error {
            super::super::EventError::Validation { message } => {
                assert!(message.contains("Test"));
            }
            _ => panic!("Expected validation error"),
        }
    }
}

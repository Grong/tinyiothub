// Unified error handling system for consistent API responses
use axum::response::Json;
use tinyiothub_web::response::ApiResponseBuilder;
use tracing::{error, info, warn};

use crate::shared::api_response::ApiResponse;

/// Standard error codes for consistent API responses
#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    // Client errors (4xx)
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    Conflict = 409,
    ValidationFailed = 422,
    TooManyRequests = 429,

    // Server errors (5xx)
    InternalError = 500,
    ServiceUnavailable = 503,
    DatabaseError = 504,
    ExternalServiceError = 502,
}

impl ErrorCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Error categories for better error handling and monitoring
#[derive(Debug, Clone)]
pub enum ErrorCategory {
    Authentication,
    Authorization,
    Validation,
    NotFound,
    Database,
    ExternalService,
    Configuration,
    Performance,
    Security,
    Business,
}

/// Unified error context for better debugging and monitoring
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub category: ErrorCategory,
    pub operation: String,
    pub user_id: Option<String>,
    pub resource_id: Option<String>,
    pub details: Option<String>,
}

impl ErrorContext {
    pub fn new(category: ErrorCategory, operation: impl Into<String>) -> Self {
        Self {
            category,
            operation: operation.into(),
            user_id: None,
            resource_id: None,
            details: None,
        }
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_resource(mut self, resource_id: impl Into<String>) -> Self {
        self.resource_id = Some(resource_id.into());
        self
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Unified error handler that provides consistent error responses and logging
pub struct ErrorHandler;

impl ErrorHandler {
    /// Handle service errors with consistent logging and response format
    pub fn handle_service_error<T: serde::Serialize>(
        result: Result<T, impl std::fmt::Display>,
        context: ErrorContext,
        user_message: &str,
    ) -> Json<ApiResponse<T>> {
        match result {
            Ok(data) => ApiResponseBuilder::success(data),
            Err(e) => {
                Self::log_error(&context, &e);
                let error_code = match context.category {
                    ErrorCategory::Authentication => ErrorCode::Unauthorized,
                    ErrorCategory::Authorization => ErrorCode::Forbidden,
                    ErrorCategory::Validation => ErrorCode::ValidationFailed,
                    ErrorCategory::NotFound => ErrorCode::NotFound,
                    ErrorCategory::Database => ErrorCode::DatabaseError,
                    ErrorCategory::ExternalService => ErrorCode::ExternalServiceError,
                    ErrorCategory::Configuration => ErrorCode::InternalError,
                    ErrorCategory::Performance => ErrorCode::ServiceUnavailable,
                    ErrorCategory::Security => ErrorCode::Forbidden,
                    ErrorCategory::Business => ErrorCode::BadRequest,
                };
                ApiResponseBuilder::error_with_code(error_code.as_i32(), user_message)
            }
        }
    }

    /// Log error with structured information
    fn log_error(context: &ErrorContext, error: &impl std::fmt::Display) {
        let log_level = match context.category {
            ErrorCategory::Authentication
            | ErrorCategory::Authorization
            | ErrorCategory::Security => "WARN",
            ErrorCategory::Validation | ErrorCategory::NotFound => "INFO",
            _ => "ERROR",
        };

        let log_message = format!(
            "[{}] Operation '{}' failed: {} | User: {} | Resource: {} | Details: {}",
            log_level,
            context.operation,
            error,
            context.user_id.as_deref().unwrap_or("unknown"),
            context.resource_id.as_deref().unwrap_or("unknown"),
            context.details.as_deref().unwrap_or("none")
        );

        match log_level {
            "ERROR" => error!("{}", log_message),
            "WARN" => warn!("{}", log_message),
            _ => info!("{}", log_message),
        }
    }
}

/// Convenient macro for error handling
#[macro_export]
macro_rules! handle_service_result {
    ($result:expr, $category:expr, $operation:expr, $user_message:expr) => {
        $crate::shared::error_handling::ErrorHandler::handle_service_error(
            $result,
            $crate::shared::error_handling::ErrorContext::new($category, $operation),
            $user_message,
        )
    };

    ($result:expr, $category:expr, $operation:expr, $user_message:expr, $user_id:expr) => {
        $crate::shared::error_handling::ErrorHandler::handle_service_error(
            $result,
            $crate::shared::error_handling::ErrorContext::new($category, $operation)
                .with_user($user_id),
            $user_message,
        )
    };

    ($result:expr, $category:expr, $operation:expr, $user_message:expr, $user_id:expr, $resource_id:expr) => {
        $crate::shared::error_handling::ErrorHandler::handle_service_error(
            $result,
            $crate::shared::error_handling::ErrorContext::new($category, $operation)
                .with_user($user_id)
                .with_resource($resource_id),
            $user_message,
        )
    };
}

/// Authorization helper functions
pub struct AuthHelper;

impl AuthHelper {
    /// Check if user has required role
    pub async fn check_role(
        state: &crate::shared::app_state::AppState,
        user_id: &str,
        required_role: &str,
    ) -> Result<bool, String> {
        let secure_service = state
            .initialize_secure_event_service()
            .await
            .map_err(|e| format!("Failed to initialize security service: {}", e))?;

        let access_control = secure_service.access_control();
        let roles = access_control
            .get_user_roles(user_id)
            .await
            .map_err(|e| format!("Failed to get user roles: {}", e))?;

        Ok(roles.contains(&required_role.to_string()))
    }

    /// Check if user has required permission
    pub async fn check_permission(
        state: &crate::shared::app_state::AppState,
        user_id: &str,
        resource_type: &str,
        permission: &str,
    ) -> Result<bool, String> {
        let secure_service = state
            .initialize_secure_event_service()
            .await
            .map_err(|e| format!("Failed to initialize security service: {}", e))?;

        let access_control = secure_service.access_control();
        let permissions = access_control
            .get_user_permissions(user_id, resource_type)
            .await
            .map_err(|e| format!("Failed to get user permissions: {}", e))?;

        Ok(permissions.contains(&permission.to_string()))
    }

    /// Require admin role or return error response
    pub async fn require_admin_role(
        state: &crate::shared::app_state::AppState,
        user_id: &str,
        _operation: &str,
    ) -> Result<(), Json<ApiResponse<serde_json::Value>>> {
        match Self::check_role(state, user_id, "admin").await {
            Ok(true) => Ok(()),
            Ok(false) => Err(ApiResponseBuilder::error_with_code(
                ErrorCode::Forbidden.as_i32(),
                "Access denied: admin role required",
            )),
            Err(e) => {
                tracing::warn!("Permission check failed for user {}: {}", user_id, e);
                Err(ApiResponseBuilder::error_with_code(
                    ErrorCode::Unauthorized.as_i32(),
                    "Permission check failed",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_builder() {
        let context = ErrorContext::new(ErrorCategory::Database, "test_operation")
            .with_user("user123")
            .with_resource("resource456")
            .with_details("Test error details");

        assert_eq!(context.operation, "test_operation");
        assert_eq!(context.user_id, Some("user123".to_string()));
        assert_eq!(context.resource_id, Some("resource456".to_string()));
        assert_eq!(context.details, Some("Test error details".to_string()));
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(ErrorCode::BadRequest.as_i32(), 400);
        assert_eq!(ErrorCode::Unauthorized.as_i32(), 401);
        assert_eq!(ErrorCode::Forbidden.as_i32(), 403);
        assert_eq!(ErrorCode::NotFound.as_i32(), 404);
        assert_eq!(ErrorCode::InternalError.as_i32(), 500);
    }
}

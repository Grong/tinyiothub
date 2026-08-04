// Cloud-specific authorization helper (depends on cloud AppState).
// Portable error types live in tinyiothub_web::error_handling — re-exported here
// so existing `shared::error_handling::{...}` imports keep working.
use axum::response::Json;
use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::response::ApiResponseBuilder;

pub use tinyiothub_web::error_handling::{ErrorCategory, ErrorCode, ErrorContext, ErrorHandler};

use crate::shared::app_state::AppState;

/// Authorization helper functions
pub struct AuthHelper;

impl AuthHelper {
    /// Check if user has required role
    pub async fn check_role(
        state: &AppState,
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
        state: &AppState,
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
        state: &AppState,
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

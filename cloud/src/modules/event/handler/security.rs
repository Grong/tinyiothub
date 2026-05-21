// Event security API endpoints
// Provides endpoints for managing event security, permissions, and audit logs

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;
use tokio::sync::OnceCell;

use crate::{
    handle_service_result,
    modules::event::value_objects::EventId,
    shared::{
        api_response::ApiResponse,
        app_state::AppState,
        error_handling::{AuthHelper, ErrorCategory},
        event::security::AuditLogEntry,
        pagination::PaginationQuery,
        performance::Cache,
        security::jwt::Claims,
    },
};

/// Cache for user permissions to improve performance
static PERMISSIONS_CACHE: OnceCell<Arc<Cache<String, UserPermissionsResponse>>> =
    OnceCell::const_new();

/// Initialize the permissions cache
async fn get_permissions_cache() -> &'static Arc<Cache<String, UserPermissionsResponse>> {
    PERMISSIONS_CACHE
        .get_or_init(|| async {
            Arc::new(Cache::new(Duration::from_secs(300), 1000)) // 5 minute TTL, max 1000 entries
        })
        .await
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogQueryParams {
    #[serde(flatten)]
    pub pagination: PaginationQuery,

    // Time range filters
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,

    // Log type filter
    pub log_type: Option<String>, // "access", "creation", "modification", "deletion"
    pub action: Option<String>,
    pub result: Option<String>,
}

/// User permissions response
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct UserPermissionsResponse {
    pub user_id: String,
    pub roles: Vec<String>,
    pub event_permissions: Vec<String>,
    pub device_permissions: Vec<String>,
    pub system_permissions: Vec<String>,
}

/// Security configuration response
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityConfigResponse {
    pub enable_rbac: bool,
    pub enable_encryption: bool,
    pub enable_audit_log: bool,
    pub audit_retention_days: u32,
}

/// Security configuration update request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityConfigUpdateRequest {
    pub enable_rbac: Option<bool>,
    pub enable_encryption: Option<bool>,
    pub enable_audit_log: Option<bool>,
    pub audit_retention_days: Option<u32>,
}

/// Audit log response DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogResponse {
    pub id: String,
    pub log_type: String,
    pub user_id: String,
    pub event_id: String,
    pub event_type: Option<String>,
    pub event_level: Option<i32>,
    pub action: String,
    pub result: String,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Get current user's event permissions
///
/// Returns the permissions that the current user has for event operations.
/// This includes roles and specific permissions for events, devices, and system.
#[axum::debug_handler]
pub async fn get_user_permissions(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<UserPermissionsResponse>> {
    let operation = "get_user_permissions";

    // Performance monitoring with caching
    let start = std::time::Instant::now();

    let result = async {
        // Try cache first
        let cache = get_permissions_cache().await;
        let cache_key = format!("permissions_{}", claims.user_id);

        if let Some(cached_response) = cache.get(&cache_key).await {
            tracing::debug!("Returning cached permissions for user: {}", claims.user_id);
            return Ok(cached_response);
        }

        // Cache miss, compute permissions
        get_user_permissions_impl(&state, &claims.user_id).await.inspect(|response| {
            // Cache the result asynchronously
            // HarmonyOS: Skip async cache update
            #[cfg(not(feature = "harmonyos"))]
            {
                let cache_clone = cache.clone();
                let key_clone = cache_key.clone();
                let response_clone = response.clone();
                use std::panic;
                tokio::spawn(async move {
                    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            cache_clone.set(key_clone, response_clone).await;
                        })
                    }));
                    if let Err(e) = result {
                        tracing::error!("Cache update panicked: {:?}", e);
                    }
                });
            }

            #[cfg(feature = "harmonyos")]
            {
                drop((cache, cache_key, response));
            }
        })
    }
    .await;

    let duration = start.elapsed();
    if duration.as_millis() > 100 {
        tracing::warn!("Slow operation '{}': {}ms", operation, duration.as_millis());
    } else {
        tracing::info!("Operation '{}' completed in {}ms", operation, duration.as_millis());
    }

    handle_service_result!(
        result,
        ErrorCategory::Authentication,
        operation,
        "Failed to retrieve user permissions",
        &claims.user_id
    )
}

/// Implementation function for getting user permissions
async fn get_user_permissions_impl(
    state: &AppState,
    user_id: &str,
) -> Result<UserPermissionsResponse, String> {
    // Initialize secure event service if needed
    let secure_service = state
        .initialize_secure_event_service()
        .await
        .map_err(|e| format!("Failed to initialize security service: {}", e))?;

    // Get user roles and permissions from access control
    let access_control = secure_service.access_control();

    let roles = access_control
        .get_user_roles(user_id)
        .await
        .map_err(|e| format!("Failed to get user roles: {}", e))?;

    // Get permissions for different resource types in parallel
    let (event_permissions, device_permissions, system_permissions) = tokio::try_join!(
        access_control.get_user_permissions(user_id, "event"),
        access_control.get_user_permissions(user_id, "device"),
        access_control.get_user_permissions(user_id, "system")
    )
    .map_err(|e| format!("Failed to get user permissions: {}", e))?;

    let response = UserPermissionsResponse {
        user_id: user_id.to_string(),
        roles,
        event_permissions,
        device_permissions,
        system_permissions,
    };

    // Log the permission check
    if let Some(audit_log) = secure_service.audit_log() {
        let _ = audit_log
            .log_access_denied(user_id, "get_permissions", "user_permissions", "success")
            .await;
    }

    Ok(response)
}

/// Get audit logs for a specific event
///
/// Returns all audit log entries for the specified event, including access,
/// creation, modification, and deletion logs.
#[axum::debug_handler]
pub async fn get_event_audit_logs(
    Path(event_id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<AuditLogResponse>>> {
    tracing::info!("Getting audit logs for event: {} by user: {}", event_id, claims.user_id);

    // Initialize secure event service if needed
    let secure_service = match state.initialize_secure_event_service().await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize secure event service: {}", e);
            return ApiResponseBuilder::error("Failed to initialize security service");
        }
    };

    // Parse event ID
    let event_id_obj = EventId::from_string(event_id.clone());

    // Check if user has permission to view audit logs for this event
    let access_control = secure_service.access_control();
    let has_permission =
        match access_control.get_user_permissions(&claims.user_id, "audit_log").await {
            Ok(perms) => perms.contains(&"read".to_string()),
            Err(_) => false,
        };

    if !has_permission {
        // Log access denied
        if let Some(audit_log) = secure_service.audit_log() {
            let _ = audit_log
                .log_access_denied(&claims.user_id, "read", "audit_log", "insufficient permissions")
                .await;
        }
        return ApiResponseBuilder::error(
            "Access denied: insufficient permissions to view audit logs",
        );
    }

    // Get audit logs from the audit service
    let audit_log = match secure_service.audit_log() {
        Some(log) => log,
        None => {
            return ApiResponseBuilder::error("Audit logging is not enabled");
        }
    };

    let entries = match audit_log.get_event_audit_logs(&event_id_obj, Some(100)).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!("Failed to get event audit logs: {}", e);
            return ApiResponseBuilder::error("Failed to retrieve audit logs");
        }
    };

    // Convert to response format
    let logs: Vec<AuditLogResponse> = entries.into_iter().map(convert_audit_log_entry).collect();

    // Log the audit log access
    let _ = audit_log.log_event_accessed(&claims.user_id, &event_id_obj).await;

    ApiResponseBuilder::success(logs)
}

/// Get audit logs for the current user
///
/// Returns audit log entries for the current user's event operations.
/// Supports pagination and filtering by time range, log type, action, and result.
#[axum::debug_handler]
pub async fn get_user_audit_logs(
    Query(params): Query<AuditLogQueryParams>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<AuditLogResponse>>> {
    tracing::info!("Getting audit logs for user: {}", claims.user_id);

    // Initialize secure event service if needed
    let secure_service = match state.initialize_secure_event_service().await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize secure event service: {}", e);
            return ApiResponseBuilder::error("Failed to initialize security service");
        }
    };

    let limit = params.pagination.page_size.unwrap_or(50).min(200);

    // Get audit logs from the audit service
    let audit_log = match secure_service.audit_log() {
        Some(log) => log,
        None => {
            return ApiResponseBuilder::error("Audit logging is not enabled");
        }
    };

    let entries = match audit_log.get_user_audit_logs(&claims.user_id, Some(limit as usize)).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!("Failed to get user audit logs: {}", e);
            return ApiResponseBuilder::error("Failed to retrieve audit logs");
        }
    };

    // Apply additional filters if provided
    let filtered_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| {
            // Filter by log type
            if let Some(ref log_type) = params.log_type
                && !entry.action.contains(log_type)
            {
                return false;
            }

            // Filter by action
            if let Some(ref action) = params.action
                && entry.action != *action
            {
                return false;
            }

            // Filter by result
            if let Some(ref result) = params.result
                && entry.result.as_ref() != Some(result)
            {
                return false;
            }

            // Filter by time range
            if let Some(start_time) = params.start_time {
                let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
                if entry.created_at < start_str {
                    return false;
                }
            }
            if let Some(end_time) = params.end_time {
                let end_str = end_time.format("%Y-%m-%d %H:%M:%S").to_string();
                if entry.created_at > end_str {
                    return false;
                }
            }

            true
        })
        .collect();

    // Convert to response format
    let logs: Vec<AuditLogResponse> =
        filtered_entries.into_iter().map(convert_audit_log_entry).collect();

    ApiResponseBuilder::success(logs)
}

/// Get audit logs for all users (admin only)
///
/// Returns audit log entries for all users. This endpoint requires admin permissions.
/// Supports pagination and filtering by time range, log type, action, and result.
#[axum::debug_handler]
pub async fn get_all_audit_logs(
    Query(params): Query<AuditLogQueryParams>,
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<AuditLogResponse>>> {
    let operation = "get_all_audit_logs";

    // Check admin permissions first
    if AuthHelper::require_admin_role(&state, &claims.user_id, operation).await.is_err() {
        return ApiResponseBuilder::error_with_code(403, "Access denied: admin role required");
    }

    // Performance monitoring
    let start = std::time::Instant::now();
    let result = get_all_audit_logs_impl(&state, params, &claims.user_id).await;
    let duration = start.elapsed();

    if duration.as_millis() > 200 {
        tracing::warn!("Slow operation '{}': {}ms", operation, duration.as_millis());
    } else {
        tracing::info!("Operation '{}' completed in {}ms", operation, duration.as_millis());
    }

    handle_service_result!(
        result,
        ErrorCategory::Database,
        operation,
        "Failed to retrieve audit logs",
        &claims.user_id
    )
}

/// Implementation function for getting all audit logs
async fn get_all_audit_logs_impl(
    state: &AppState,
    params: AuditLogQueryParams,
    user_id: &str,
) -> Result<Vec<AuditLogResponse>, String> {
    let secure_service = state
        .initialize_secure_event_service()
        .await
        .map_err(|e| format!("Failed to initialize security service: {}", e))?;

    let limit = params.pagination.page_size.unwrap_or(50).min(200);
    let offset = params.pagination.page.unwrap_or(1).saturating_sub(1) * limit;

    // Get audit logs from the audit service
    let audit_log = secure_service.audit_log().ok_or("Audit logging is not enabled")?;

    let entries = audit_log
        .get_all_audit_logs(Some(limit as usize), Some(offset as usize))
        .await
        .map_err(|e| format!("Failed to get all audit logs: {}", e))?;

    // Apply additional filters efficiently
    let filtered_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| {
            // Filter by log type
            if let Some(ref log_type) = params.log_type
                && !entry.action.contains(log_type)
            {
                return false;
            }

            // Filter by action
            if let Some(ref action) = params.action
                && entry.action != *action
            {
                return false;
            }

            // Filter by result
            if let Some(ref result) = params.result
                && entry.result.as_ref() != Some(result)
            {
                return false;
            }

            true
        })
        .collect();

    // Convert to response format
    let logs: Vec<AuditLogResponse> =
        filtered_entries.into_iter().map(convert_audit_log_entry).collect();

    // Log the admin audit access
    let _ = audit_log
        .log(
            AuditLogEntry::new("admin_audit_access".to_string(), Some(user_id.to_string()))
                .with_details(format!("Accessed all audit logs, returned {} entries", logs.len())),
        )
        .await;

    Ok(logs)
}

/// Clean up old audit logs (admin only)
///
/// Removes audit log entries older than the configured retention period.
/// This endpoint requires admin permissions.
#[axum::debug_handler]
pub async fn cleanup_audit_logs(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<u64>> {
    let operation = "cleanup_audit_logs";

    // Check admin permissions first
    if AuthHelper::require_admin_role(&state, &claims.user_id, operation).await.is_err() {
        return ApiResponseBuilder::error_with_code(403, "Access denied: admin role required");
    }

    // Performance monitoring for potentially long-running operation
    let start = std::time::Instant::now();
    let result = cleanup_audit_logs_impl(&state, &claims.user_id).await;
    let duration = start.elapsed();

    if duration.as_millis() > 5000 {
        tracing::warn!("Slow operation '{}': {}ms", operation, duration.as_millis());
    } else {
        tracing::info!("Operation '{}' completed in {}ms", operation, duration.as_millis());
    }

    handle_service_result!(
        result,
        ErrorCategory::Database,
        operation,
        "Failed to cleanup audit logs",
        &claims.user_id
    )
}

/// Implementation function for cleaning up audit logs
async fn cleanup_audit_logs_impl(state: &AppState, user_id: &str) -> Result<u64, String> {
    let secure_service = state
        .initialize_secure_event_service()
        .await
        .map_err(|e| format!("Failed to initialize security service: {}", e))?;

    // Get audit log service
    let audit_log = secure_service.audit_log().ok_or("Audit logging is not enabled")?;

    // Use default retention period of 90 days
    let retention_days = 90u32;

    let cleaned_count = audit_log
        .cleanup_old_logs(retention_days)
        .await
        .map_err(|e| format!("Failed to cleanup audit logs: {}", e))?
        as u64;

    // Log the cleanup operation
    let _ = audit_log
        .log(
            AuditLogEntry::new("audit_cleanup".to_string(), Some(user_id.to_string()))
                .with_details(format!(
                    "Cleaned up {} old audit log entries (retention: {} days)",
                    cleaned_count, retention_days
                )),
        )
        .await;

    tracing::info!("Successfully cleaned up {} audit log entries", cleaned_count);

    Ok(cleaned_count)
}

/// Get security configuration
///
/// Returns the current security configuration including RBAC, encryption,
/// and audit logging settings.
#[axum::debug_handler]
pub async fn get_security_config(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<SecurityConfigResponse>> {
    tracing::info!("Getting security config requested by user: {}", claims.user_id);

    // Initialize secure event service if needed
    let secure_service = match state.initialize_secure_event_service().await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize secure event service: {}", e);
            return ApiResponseBuilder::error("Failed to initialize security service");
        }
    };

    // Check if user has permission to view security config
    let access_control = secure_service.access_control();
    let has_permission = match access_control.get_user_permissions(&claims.user_id, "system").await
    {
        Ok(perms) => perms.contains(&"read".to_string()),
        Err(_) => false,
    };

    if !has_permission {
        // Log access denied
        if let Some(audit_log) = secure_service.audit_log() {
            let _ = audit_log
                .log_access_denied(
                    &claims.user_id,
                    "read",
                    "security_config",
                    "insufficient permissions",
                )
                .await;
        }
        return ApiResponseBuilder::error(
            "Access denied: insufficient permissions to view security configuration",
        );
    }

    // Get configuration from secure service
    let config = secure_service.config();

    let response = SecurityConfigResponse {
        enable_rbac: config.enable_rbac,
        enable_encryption: config.enable_encryption,
        enable_audit_log: config.enable_audit_log,
        audit_retention_days: config.audit_retention_days,
    };

    // Log the config access
    if let Some(audit_log) = secure_service.audit_log() {
        let _ = audit_log
            .log(
                crate::shared::event::security::AuditLogEntry::new(
                    "security_config_access".to_string(),
                    Some(claims.user_id.clone()),
                )
                .with_details("Accessed security configuration".to_string()),
            )
            .await;
    }

    ApiResponseBuilder::success(response)
}

/// Update security configuration (admin only)
///
/// Updates the security configuration. This endpoint requires admin permissions.
#[axum::debug_handler]
pub async fn update_security_config(
    State(state): State<AppState>,
    claims: Claims,
    Json(request): Json<SecurityConfigUpdateRequest>,
) -> Json<ApiResponse<SecurityConfigResponse>> {
    tracing::info!("Updating security config requested by user: {}", claims.user_id);

    // Initialize secure event service if needed
    let secure_service = match state.initialize_secure_event_service().await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize secure event service: {}", e);
            return ApiResponseBuilder::error("Failed to initialize security service");
        }
    };

    // Check if user has admin permissions
    let access_control = secure_service.access_control();
    let is_admin = match access_control.get_user_roles(&claims.user_id).await {
        Ok(roles) => roles.contains(&"admin".to_string()),
        Err(e) => {
            tracing::error!("Failed to check user roles: {}", e);
            false
        }
    };

    if !is_admin {
        // Log access denied
        if let Some(audit_log) = secure_service.audit_log() {
            let _ = audit_log
                .log_access_denied(
                    &claims.user_id,
                    "update",
                    "security_config",
                    "admin role required",
                )
                .await;
        }
        return ApiResponseBuilder::error("Access denied: admin role required");
    }

    // Validate configuration changes
    if let Some(retention_days) = request.audit_retention_days
        && !(1..=3650).contains(&retention_days)
    {
        return ApiResponseBuilder::error(
            "Invalid audit retention days: must be between 1 and 3650",
        );
    }

    // Get current configuration
    let mut config = secure_service.config();

    // Apply updates
    if let Some(enable_rbac) = request.enable_rbac {
        config.enable_rbac = enable_rbac;
    }
    if let Some(enable_encryption) = request.enable_encryption {
        config.enable_encryption = enable_encryption;
    }
    if let Some(enable_audit_log) = request.enable_audit_log {
        config.enable_audit_log = enable_audit_log;
    }
    if let Some(audit_retention_days) = request.audit_retention_days {
        config.audit_retention_days = audit_retention_days;
    }

    let response = SecurityConfigResponse {
        enable_rbac: config.enable_rbac,
        enable_encryption: config.enable_encryption,
        enable_audit_log: config.enable_audit_log,
        audit_retention_days: config.audit_retention_days,
    };

    // Persist configuration changes through the service (deduplicated path)
    if let Err(e) = secure_service.update_config(config).await {
        tracing::error!("Failed to persist security config: {}", e);
        return ApiResponseBuilder::error("配置保存失败");
    }

    // Log the configuration update
    if let Some(audit_log) = secure_service.audit_log() {
        let changes = serde_json::json!({
            "enable_rbac": request.enable_rbac,
            "enable_encryption": request.enable_encryption,
            "enable_audit_log": request.enable_audit_log,
            "audit_retention_days": request.audit_retention_days,
        });

        let _ = audit_log
            .log(
                crate::shared::event::security::AuditLogEntry::new(
                    "security_config_update".to_string(),
                    Some(claims.user_id.clone()),
                )
                .with_details(format!("Updated security configuration: {}", changes)),
            )
            .await;
    }

    tracing::info!("Security configuration updated successfully by user: {}", claims.user_id);

    ApiResponseBuilder::success(response)
}

/// Get user roles
///
/// Returns the roles assigned to the current user.
#[axum::debug_handler]
pub async fn get_user_roles(
    State(state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<String>>> {
    tracing::info!("Getting roles for user: {}", claims.user_id);

    // Initialize secure event service if needed
    let secure_service = match state.initialize_secure_event_service().await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize secure event service: {}", e);
            return ApiResponseBuilder::error("Failed to initialize security service");
        }
    };

    // Get user roles from access control
    let access_control = secure_service.access_control();
    let roles = match access_control.get_user_roles(&claims.user_id).await {
        Ok(roles) => roles,
        Err(e) => {
            tracing::error!("Failed to get user roles: {}", e);
            return ApiResponseBuilder::error("Failed to retrieve user roles");
        }
    };

    // Log the role access
    if let Some(audit_log) = secure_service.audit_log() {
        let _ = audit_log
            .log(
                crate::shared::event::security::AuditLogEntry::new(
                    "user_roles_access".to_string(),
                    Some(claims.user_id.clone()),
                )
                .with_details(format!("Accessed user roles: {:?}", roles)),
            )
            .await;
    }

    ApiResponseBuilder::success(roles)
}

/// Convert domain audit log entry to API response
fn convert_audit_log_entry(entry: AuditLogEntry) -> AuditLogResponse {
    AuditLogResponse {
        id: entry.id,
        log_type: "audit".to_string(), // Default log type since it's not in the struct
        user_id: entry.user_id.unwrap_or_default(),
        event_id: entry.event_id.unwrap_or_default(),
        event_type: entry.event_type,
        event_level: entry.event_level.and_then(|level| level.parse::<i32>().ok()),
        action: entry.action,
        result: entry.result.unwrap_or_default(),
        details: entry.details.and_then(|d| serde_json::from_str(&d).ok()),
        ip_address: entry.ip_address,
        user_agent: entry.user_agent,
        created_at: chrono::DateTime::parse_from_str(&entry.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_conversion() {
        let entry = AuditLogEntry {
            id: "test-id".to_string(),
            action: "read".to_string(),
            user_id: Some("user123".to_string()),
            event_id: Some("event456".to_string()),
            event_type: Some("system.user_auth".to_string()),
            event_level: Some("2".to_string()),
            result: Some("allowed".to_string()),
            details: Some(r#"{"test": "data"}"#.to_string()),
            ip_address: Some("127.0.0.1".to_string()), // 测试数据使用localhost
            user_agent: Some("Mozilla/5.0".to_string()),
            created_at: "2024-01-01 12:00:00".to_string(),
        };

        let response = convert_audit_log_entry(entry);

        assert_eq!(response.id, "test-id");
        assert_eq!(response.log_type, "audit");
        assert_eq!(response.user_id, "user123");
        assert_eq!(response.action, "read");
        assert_eq!(response.result, "allowed");
        assert!(response.details.is_some());
    }
}

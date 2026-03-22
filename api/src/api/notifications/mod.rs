/// Notification management API endpoints
///
/// This module provides REST API endpoints for managing notification rules
/// and viewing notification history.
pub mod management;

// Re-export API handlers
use axum::{
    routing::{get, post},
    Router,
};
pub use management::*;

use crate::shared::app_state::AppState;

/// Create notification API router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Notification rules management
        .route("/rules", get(get_notification_rules).post(create_notification_rule))
        .route(
            "/rules/:rule_id",
            get(get_notification_rule)
                .put(update_notification_rule)
                .delete(delete_notification_rule),
        )
        // Notification history
        .route("/history", get(get_notification_history))
        // Test notifications
        .route("/test", post(send_test_notification))

    // Note: SSE endpoints have been moved to /api/v1/events/sse
}

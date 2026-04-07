// Infrastructure Layer - SSE Connection Manager
// Manages Server-Sent Events (SSE) connections and event distribution

use std::sync::Arc;

use axum::response::Response;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::{
    application::data_context::DataContext,
    domain::event::entities::Event,
    infrastructure::event::channels::sse_channel::{SseMessage, SseNotificationChannel},
};

/// SSE Connection Manager
///
/// Manages SSE connections and event distribution in the infrastructure layer.
/// Filters events by workspace so users only see their current workspace's devices.
pub struct SseConnectionManager {
    /// SSE notification channel for managing connections
    sse_channel: Arc<SseNotificationChannel>,

    /// Data context for device workspace lookup during broadcast
    data_context: Option<Arc<DataContext>>,
}

impl SseConnectionManager {
    /// Create a new SSE connection manager
    pub fn new() -> Self {
        Self { sse_channel: Arc::new(SseNotificationChannel::new()), data_context: None }
    }

    /// Create with DataContext for device workspace lookup
    pub fn with_data_context(data_context: Arc<DataContext>) -> Self {
        Self { sse_channel: Arc::new(SseNotificationChannel::new()), data_context: Some(data_context) }
    }

    /// Create an authenticated SSE connection
    ///
    /// # Arguments
    /// * `user_id` - User identifier
    /// * `workspace_id` - Workspace to scope events to
    /// * `_event_types` - Optional filter for event types (not yet implemented)
    /// * `_event_levels` - Optional filter for event severity levels (not yet implemented)
    ///
    /// # Returns
    /// An Axum Response with SSE stream
    pub async fn create_connection(
        &self,
        user_id: String,
        workspace_id: String,
        _event_types: Option<Vec<String>>,
        _event_levels: Option<Vec<String>>,
    ) -> Response {
        info!("Creating authenticated SSE connection for user: {} workspace: {}", user_id, workspace_id);

        self.sse_channel.create_sse_stream(user_id, workspace_id).await
    }

    /// Create a public (unauthenticated) SSE connection
    ///
    /// # Arguments
    /// * `user_id` - User identifier (can be "anonymous")
    /// * `workspace_id` - Workspace to scope events to
    /// * `_event_types` - Optional filter for event types (not yet implemented)
    /// * `_event_levels` - Optional filter for event severity levels (not yet implemented)
    ///
    /// # Returns
    /// An Axum Response with SSE stream
    pub async fn create_public_connection(
        &self,
        user_id: String,
        workspace_id: String,
        _event_types: Option<Vec<String>>,
        _event_levels: Option<Vec<String>>,
    ) -> Response {
        info!("Creating public SSE connection for user: {} workspace: {}", user_id, workspace_id);

        self.sse_channel.create_sse_stream(user_id, workspace_id).await
    }

    /// Broadcast an event to all matching connections
    ///
    /// # Arguments
    /// * `event` - The event to broadcast
    pub async fn broadcast_event(&self, event: &Event) {
        debug!("Broadcasting event to SSE connections");

        // Build dotted event_type string matching frontend expectations (e.g. "device.connection")
        let event_type_str = format!("{}.{}", event.event_type().type_string(), event.event_type().subtype_string());

        // Extract device_id from source if present
        let device_id = event.source().device_id().map(|s| s.to_string());

        // Look up workspace_id from device cache so SSE filters can route events
        let workspace_id = if let Some(ref dev_id) = device_id {
            self.data_context
                .as_ref()
                .and_then(|dc| dc.get_device(dev_id))
                .and_then(|d| d.workspace_id)
        } else {
            None
        };

        // Build base data payload
        let mut data = serde_json::json!({
            "event_type": event_type_str,
            "device_id": device_id,
            "workspace_id": workspace_id,
            "level": format!("{}", event.level()),
            "timestamp": event.timestamp().to_rfc3339(),
            "content": event.content(),
            "source": event.source()
        });

        // Enrich property_change events with structured fields
        if event_type_str == "device.property_change" {
            // Extract property_id from source.source_id (format: "device_id:property_id")
            let source_id = event.source().source_id();
            if let Some(colon_pos) = source_id.find(':') {
                let property_id = &source_id[colon_pos + 1..];
                data["property_id"] = serde_json::Value::String(property_id.to_string());
            }

            // Extract property_name from content title: "Property Changed: DeviceName - PropertyName"
            let title = event.content().title();
            if let Some(dash_pos) = title.rfind(" - ") {
                let property_name = &title[dash_pos + 3..];
                data["property_name"] = serde_json::Value::String(property_name.to_string());
            }

            // Extract new_value from content elements (look for "Current value: X")
            for element in event.content().elements() {
                if let crate::domain::event::value_objects::rich_content::ContentElement::Text { content, .. } = element {
                    if let Some(val) = content.strip_prefix("Current value: ") {
                        data["new_value"] = serde_json::Value::String(val.to_string());
                        break;
                    }
                }
            }
        }

        let sse_message = SseMessage {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type_str.clone(),
            data,
            timestamp: chrono::Utc::now(),
        };

        if let Err(e) = self.sse_channel.broadcast(sse_message).await {
            error!("Failed to broadcast event: {}", e);
        }
    }

    /// Send an event to a specific user
    ///
    /// # Arguments
    /// * `user_id` - Target user identifier
    /// * `event` - The event to send
    pub async fn send_to_user(&self, user_id: &str, _event: &Event) {
        debug!("Sending event to user: {}", user_id);

        // Convert event to SSE message
        let sse_message = SseMessage {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "event".to_string(),
            data: serde_json::json!({
                "message": "Event notification",
                "user_id": user_id
            }),
            timestamp: chrono::Utc::now(),
        };

        if let Err(e) = self.sse_channel.send_to_user(user_id, sse_message).await {
            error!("Failed to send event to user {}: {}", user_id, e);
        }
    }

    /// Get connection statistics
    pub async fn get_overview(&self) -> SseOverview {
        let connection_count = self.sse_channel.get_connection_count().await;

        SseOverview {
            total_connections: connection_count,
            active_connections: connection_count,
            total_events_sent: 0, // TODO: Implement event counter
            average_latency_ms: 0.0,
        }
    }

    /// Get list of active connections
    pub async fn get_connections(&self) -> Vec<SseConnectionInfo> {
        let connections = self.sse_channel.get_connections().await;

        // Convert SseConnection to SseConnectionInfo
        connections
            .into_iter()
            .map(|conn| SseConnectionInfo {
                user_id: conn.user_id,
                workspace_id: conn.workspace_id,
                connected_at: conn.connected_at.to_rfc3339(),
                event_types: None,
                event_levels: None,
            })
            .collect()
    }

    /// Get total connection count
    pub async fn get_connection_count(&self) -> usize {
        self.sse_channel.get_connection_count().await
    }

    /// Clean up stale connections
    pub async fn cleanup_stale_connections(&self) {
        debug!("Cleaning up stale SSE connections");
        self.sse_channel.cleanup_stale_connections(std::time::Duration::from_secs(300)).await;
    }
}

impl Default for SseConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// SSE connection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseOverview {
    pub total_connections: usize,
    pub active_connections: usize,
    pub total_events_sent: u64,
    pub average_latency_ms: f64,
}

/// SSE connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseConnectionInfo {
    pub user_id: String,
    pub workspace_id: String,
    pub connected_at: String,
    pub event_types: Option<Vec<String>>,
    pub event_levels: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_creation() {
        let manager = SseConnectionManager::new();
        assert_eq!(manager.get_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_statistics() {
        let manager = SseConnectionManager::new();
        let stats = manager.get_overview().await;

        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_connections, 0);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let manager = SseConnectionManager::new();
        manager.cleanup_stale_connections().await;
        // Should not panic
    }
}

use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};

use axum::response::{
    sse::{Event as SseEvent, KeepAlive},
    IntoResponse, Response, Sse,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::domain::event::{
    aggregates::notification_aggregate::NotificationChannelType,
    services::{NotificationChannel, NotificationMessage},
    Result,
};

/// SSE connection information
#[derive(Debug, Clone)]
pub struct SseConnection {
    pub connection_id: String,
    pub user_id: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_ping: chrono::DateTime<chrono::Utc>,
}

/// SSE message that will be sent to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseMessage {
    pub id: String,
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// SSE notification channel handler
pub struct SseNotificationChannel {
    connections: Arc<RwLock<HashMap<String, SseConnection>>>,
    sender: broadcast::Sender<SseMessage>,
    _receiver: broadcast::Receiver<SseMessage>, // Keep one receiver to prevent channel from closing
}

impl SseNotificationChannel {
    /// Create a new SSE notification channel
    pub fn new() -> Self {
        let (sender, receiver) = broadcast::channel(1000);

        Self { connections: Arc::new(RwLock::new(HashMap::new())), sender, _receiver: receiver }
    }

    /// Create an SSE response for a client connection
    pub async fn create_sse_stream(&self, user_id: String) -> Response {
        let connection_id = Uuid::new_v4().to_string();
        let connection = SseConnection {
            connection_id: connection_id.clone(),
            user_id: user_id.clone(),
            connected_at: chrono::Utc::now(),
            last_ping: chrono::Utc::now(),
        };

        // Store the connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(connection_id.clone(), connection);
        }

        info!("New SSE connection established: {} for user: {}", connection_id, user_id);

        // Create a receiver for this connection
        let mut receiver = self.sender.subscribe();
        let connections_ref = self.connections.clone();
        let connection_id_clone = connection_id.clone();

        // Create the SSE stream
        let stream = async_stream::stream! {
            // Send initial connection message
            let welcome_msg = SseEvent::default()
                .id(&connection_id)
                .event("connected")
                .data(format!(r#"{{"message": "Connected to notification service", "connection_id": "{}"}}"#, connection_id));

            yield Ok::<_, Infallible>(welcome_msg);

            // Send periodic keep-alive messages and handle incoming notifications
            let mut keep_alive_interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    // Handle incoming notifications
                    msg = receiver.recv() => {
                        match msg {
                            Ok(sse_message) => {
                                // Check if this message should be sent to this user
                                if should_send_to_user(&user_id, &sse_message) {
                                    let event = SseEvent::default()
                                        .id(&sse_message.id)
                                        .event(&sse_message.event_type)
                                        .data(serde_json::to_string(&sse_message.data).unwrap_or_default());

                                    yield Ok(event);
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                warn!("SSE connection {} lagged behind", connection_id);
                                // Continue processing
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                debug!("SSE broadcast channel closed for connection {}", connection_id);
                                break;
                            }
                        }
                    }

                    // Send keep-alive messages
                    _ = keep_alive_interval.tick() => {
                        // Update last ping time
                        {
                            let mut connections = connections_ref.write().await;
                            if let Some(conn) = connections.get_mut(&connection_id_clone) {
                                conn.last_ping = chrono::Utc::now();
                            }
                        }

                        let ping_event = SseEvent::default()
                            .event("ping")
                            .data("ping");

                        yield Ok(ping_event);
                    }
                }
            }

            // Clean up connection when stream ends
            {
                let mut connections = connections_ref.write().await;
                connections.remove(&connection_id_clone);
            }

            info!("SSE connection closed: {}", connection_id_clone);
        };

        Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
    }

    /// Broadcast a message to all connected clients
    pub async fn broadcast(&self, message: SseMessage) -> Result<usize> {
        match self.sender.send(message.clone()) {
            Ok(receiver_count) => {
                debug!(
                    "Broadcasted SSE message to {} receivers: {}",
                    receiver_count, message.event_type
                );
                Ok(receiver_count)
            }
            Err(_) => {
                warn!("Failed to broadcast SSE message: no receivers");
                Ok(0)
            }
        }
    }

    /// Send a message to a specific user
    pub async fn send_to_user(&self, user_id: &str, message: SseMessage) -> Result<()> {
        // For now, we broadcast to all and filter on the client side
        // In a more sophisticated implementation, we could maintain user-specific channels
        let mut user_message = message;
        user_message.data = serde_json::json!({
            "target_user": user_id,
            "data": user_message.data
        });

        self.broadcast(user_message).await?;
        Ok(())
    }

    /// Get active connection count
    pub async fn get_connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Get active connections
    pub async fn get_connections(&self) -> Vec<SseConnection> {
        self.connections.read().await.values().cloned().collect()
    }

    /// Clean up stale connections
    pub async fn cleanup_stale_connections(&self, max_idle_duration: Duration) -> usize {
        let cutoff_time =
            chrono::Utc::now() - chrono::Duration::from_std(max_idle_duration).unwrap_or_default();
        let mut connections = self.connections.write().await;
        let initial_count = connections.len();

        connections.retain(|_, conn| conn.last_ping > cutoff_time);

        let removed_count = initial_count - connections.len();
        if removed_count > 0 {
            info!("Cleaned up {} stale SSE connections", removed_count);
        }

        removed_count
    }
}

/// Check if a message should be sent to a specific user
fn should_send_to_user(user_id: &str, message: &SseMessage) -> bool {
    // Check if message has a target user specified
    if let Some(target_user) = message.data.get("target_user") {
        if let Some(target_str) = target_user.as_str() {
            return target_str == user_id;
        }
    }

    // Check if message has recipient information
    if let Some(recipient) = message.data.get("recipient") {
        if let Some(recipient_str) = recipient.as_str() {
            // For now, simple string matching. In production, this could be more sophisticated
            return recipient_str == user_id || recipient_str == "admin" || recipient_str == "all";
        }
    }

    // Default: send to all users (for broadcast messages)
    true
}

impl SseMessage {
    /// Create a new SSE message
    pub fn new(event_type: String, data: serde_json::Value) -> Self {
        Self { id: Uuid::new_v4().to_string(), event_type, data, timestamp: chrono::Utc::now() }
    }

    /// Create a notification message
    pub fn notification(title: String, content: String, level: String) -> Self {
        Self::new(
            "notification".to_string(),
            serde_json::json!({
                "title": title,
                "content": content,
                "level": level,
                "timestamp": chrono::Utc::now()
            }),
        )
    }

    /// Create a system status message
    pub fn system_status(status: String, details: serde_json::Value) -> Self {
        Self::new(
            "system_status".to_string(),
            serde_json::json!({
                "status": status,
                "details": details,
                "timestamp": chrono::Utc::now()
            }),
        )
    }

    /// Create a real-time event update message
    pub fn real_time_update(event_data: serde_json::Value) -> Self {
        Self::new("real_time_update".to_string(), event_data)
    }
}
#[async_trait::async_trait]
impl NotificationChannel for SseNotificationChannel {
    fn channel_type(&self) -> NotificationChannelType {
        NotificationChannelType::Sse
    }

    async fn send(&self, message: &NotificationMessage) -> std::result::Result<(), String> {
        let sse_message = SseMessage {
            id: Uuid::new_v4().to_string(),
            event_type: "notification".to_string(),
            data: serde_json::json!({
                "title": message.formatted_title(),
                "content": message.content,
                "level": message.level.as_str(),
                "timestamp": message.timestamp,
                "metadata": message.metadata,
                "recipients": message.recipients
            }),
            timestamp: chrono::Utc::now(),
        };

        // Broadcast to all connections
        self.broadcast(sse_message).await.map_err(|e| e.to_string())?;

        info!("SSE notification sent: {}", message.formatted_title());
        Ok(())
    }

    async fn is_available(&self) -> bool {
        // SSE is always available as it's built into the application
        true
    }

    fn get_config(&self) -> std::collections::HashMap<String, String> {
        let mut config = std::collections::HashMap::new();
        config.insert("enabled".to_string(), "true".to_string());
        config.insert("connection_count".to_string(), "0".to_string()); // Would need async context to get real count
        config
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::Duration;

    use super::*;
    use crate::domain::event::services::{
        NotificationChannel, NotificationLevel, NotificationMessage,
    };

    #[tokio::test]
    async fn test_sse_channel_creation() {
        let channel = SseNotificationChannel::new();
        assert!(channel.is_available().await);
        assert_eq!(channel.get_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_sse_message_creation() {
        let message = SseMessage::notification(
            "Test Alert".to_string(),
            "This is a test".to_string(),
            "warning".to_string(),
        );

        assert_eq!(message.event_type, "notification");
        assert!(message.data.get("title").is_some());
        assert!(message.data.get("content").is_some());
        assert!(message.data.get("level").is_some());
    }

    #[tokio::test]
    async fn test_broadcast_message() {
        let channel = SseNotificationChannel::new();

        let message =
            SseMessage::notification("Test".to_string(), "Content".to_string(), "info".to_string());

        // Should succeed even with no connections
        let result: crate::domain::event::Result<usize> = channel.broadcast(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_notification_channel_handler() {
        let channel = SseNotificationChannel::new();

        let notification = NotificationMessage::new(
            "Test Alert".to_string(),
            "Test content".to_string(),
            NotificationLevel::Warning,
            vec![NotificationChannelType::Sse],
            vec!["test_user".to_string()],
        );

        let result: std::result::Result<(), String> = channel.send(&notification).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_should_send_to_user() {
        let message_with_target = SseMessage::new(
            "test".to_string(),
            serde_json::json!({
                "target_user": "user1",
                "content": "test"
            }),
        );

        assert!(should_send_to_user("user1", &message_with_target));
        assert!(!should_send_to_user("user2", &message_with_target));

        let message_with_recipient = SseMessage::new(
            "test".to_string(),
            serde_json::json!({
                "recipient": "admin",
                "content": "test"
            }),
        );

        assert!(should_send_to_user("admin", &message_with_recipient));
        assert!(should_send_to_user("any_user", &message_with_recipient)); // admin messages go to all

        let broadcast_message = SseMessage::new(
            "test".to_string(),
            serde_json::json!({
                "content": "broadcast"
            }),
        );

        assert!(should_send_to_user("any_user", &broadcast_message));
    }

    #[tokio::test]
    async fn test_cleanup_stale_connections() {
        let channel = SseNotificationChannel::new();

        // Manually add a stale connection for testing
        {
            let mut connections: tokio::sync::RwLockWriteGuard<
                '_,
                std::collections::HashMap<String, SseConnection>,
            > = channel.connections.write().await;
            connections.insert(
                "stale_connection".to_string(),
                SseConnection {
                    connection_id: "stale_connection".to_string(),
                    user_id: "test_user".to_string(),
                    connected_at: chrono::Utc::now() - chrono::Duration::hours(2),
                    last_ping: chrono::Utc::now() - chrono::Duration::hours(1),
                },
            );
        }

        assert_eq!(channel.get_connection_count().await, 1);

        // Clean up connections older than 30 minutes
        let removed = channel.cleanup_stale_connections(Duration::from_secs(30 * 60)).await;
        assert_eq!(removed, 1);
        assert_eq!(channel.get_connection_count().await, 0);
    }
}

pub mod password;
pub mod spawn;
pub mod sql_security;
pub mod trace_util;
pub mod validation;

// Re-export commonly used utilities
pub use spawn::{execute_or_spawn, spawn_safe, spawn_with_error_handling};

// Note: trace_device macro is exported at crate root due to #[macro_export]

/// Publish event with platform-specific handling
///
/// On HarmonyOS: publishes inline (blocking)
/// On other platforms: spawns background task
#[cfg(feature = "harmonyos")]
pub async fn publish_event_safe(
    event_bus: std::sync::Arc<crate::shared::event::EventBus>,
    event: crate::modules::event::entities::Event,
) {
    // On HarmonyOS, publish inline
    if let Err(e) = event_bus.publish(event).await {
        tracing::error!("Failed to publish event: {}", e);
    }
}

/// Publish event with platform-specific handling
///
/// On HarmonyOS: publishes inline (blocking)
/// On other platforms: spawns background task
#[cfg(not(feature = "harmonyos"))]
pub async fn publish_event_safe(
    event_bus: std::sync::Arc<crate::shared::event::EventBus>,
    event: crate::modules::event::entities::Event,
) {
    tokio::spawn(async move {
        if let Err(e) = event_bus.publish(event).await {
            tracing::error!("Failed to publish event: {}", e);
        }
    });
}

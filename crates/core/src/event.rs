//! Event handler contract — trait for event bus subscribers.

use async_trait::async_trait;

use crate::error::Result;
use crate::models::event::Event;

/// Event handler interface
///
/// All event handlers must implement this interface
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle event
    async fn handle(&self, event: &Event) -> Result<()>;

    /// Get handler name (for logging)
    fn name(&self) -> &str;

    /// Determine whether this event should be handled
    fn should_handle(&self, event: &Event) -> bool;

    /// Get handler priority (lower number = higher priority)
    fn priority(&self) -> u8 {
        100
    }
}

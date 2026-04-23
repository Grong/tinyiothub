use std::sync::Arc;

use tinyiothub_engine::event_bus::EventBus;
use tinyiothub_storage::Storage;

/// Trait abstracting the application state for HTTP handlers.
///
/// Implemented by the cloud crate's `AppState` so handlers in this crate
/// can be wired up without knowing concrete types.
pub trait WebState: Clone + Send + Sync + 'static {
    /// Access the unified storage facade.
    fn storage(&self) -> Arc<Storage>;

    /// Access the event bus for publishing domain events.
    fn event_bus(&self) -> Arc<EventBus>;
}

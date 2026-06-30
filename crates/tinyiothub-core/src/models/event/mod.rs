#[allow(clippy::module_inception)]
pub mod event;
pub mod event_id;
pub mod event_level;
pub mod event_source;
pub mod event_type;
pub mod rich_content;

// Backward compatibility: ConnectionStatus unified into DeviceStatus
pub use crate::models::device::DeviceStatus as ConnectionStatus;

pub use event::Event;
pub use event_id::EventId;
pub use event_level::EventLevel;
pub use event_source::EventSource;
pub use event_type::{AiEventType, DeviceEventType, EventType, SystemEventType};
pub use rich_content::{ContentElement, LinkTarget, RichContent, TextFormat};

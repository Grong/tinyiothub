// TinyIoTHub Library
// This enables testing of internal modules

pub mod api;
pub mod application;
pub mod domain;
pub mod dto;
pub mod infrastructure;
pub mod server;
pub mod shared;

// Re-export commonly used types for easier access
pub use domain::event;
pub use infrastructure::persistence::Database;
pub use shared::error::Error;

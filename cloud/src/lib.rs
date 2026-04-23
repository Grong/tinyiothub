// TinyIoTHub Library
// This enables testing of internal modules

pub mod api;
pub mod modules;
pub mod server;
pub mod shared;

// Re-export commonly used types for easier access
pub use shared::persistence::Database;
pub use shared::error::Error;

// TinyIoTHub Library
// This enables testing of internal modules

pub mod api;
pub mod modules;
pub mod server;
pub mod shared;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
mod tests;

// Re-export commonly used types for easier access
pub use shared::{error::Error, persistence::Database};

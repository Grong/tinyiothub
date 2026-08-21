#![allow(clippy::double_must_use)] // async_trait 展开的 BoxFuture 自带 must_use，属已知误报类
// TinyIoTHub Library
// This enables testing of internal modules

pub mod api;
pub mod bootstrap;
pub mod domains;
pub mod router;
pub mod shared;
pub mod state;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
mod tests;

// Re-export commonly used types for easier access
pub use shared::error::Error;
pub use tinyiothub_storage::Db;

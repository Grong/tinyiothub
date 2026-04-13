// TinyIoTHub Library
// This enables testing of internal modules

// 禁用开发阶段的常见警告，保持编译输出清晰
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]

pub mod api;
pub mod application;
pub mod domain;
pub mod dto;
pub mod infrastructure;
pub mod shared;
pub mod utils;

// Re-export commonly used types for easier access
pub use domain::event;
pub use infrastructure::persistence::Database;
pub use shared::error::Error;

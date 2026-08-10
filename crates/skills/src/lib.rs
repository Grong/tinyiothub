//! Agent skills & tools — skill loading, tool registry, and the trust engine.
//!
//! The agent crate provides DB/HTTP integrations in host::skill / host::tools (P4-Task22).
//!
//! ## 设计不变量
//! - 技能/工具注册表；不依赖其他领域 crate


pub mod loader;
pub mod registry;
pub mod tool_types;
pub mod trust;
pub mod types;

pub use loader::*;
pub use types::*;

//! Agent skills & tools — skill loading, tool registry, and the trust engine.
//!
//! The agent crate provides DB/HTTP integrations in host::skill / host::tools (P4-Task22).

pub mod loader;
pub mod registry;
pub mod tool_types;
pub mod trust;
pub mod types;

pub use loader::*;
pub use types::*;

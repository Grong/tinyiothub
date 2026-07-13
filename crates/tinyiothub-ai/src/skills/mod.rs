//! Agent skills — skill types, parsing, and template execution.
//!
//! Cloud provides DB/HTTP integrations in modules::agent::skill.

pub mod loader;
pub mod types;

pub use loader::*;
pub use types::*;

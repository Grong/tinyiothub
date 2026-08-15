//! Agent skills & tools — skill loading, tool registry, and the trust engine.
//!
//! The agent crate provides DB/HTTP integrations in host::skill / host::tools (P4-Task22).
//!
//! ## 设计不变量
//! - 技能/工具注册表与信任引擎：裁决为纯逻辑
//! - 禁止依赖 web、runtime、core 以外的领域 crate

pub mod loader;
pub mod registry;
pub mod tool_types;
pub mod trust;
pub mod types;

pub use loader::*;
pub use types::*;

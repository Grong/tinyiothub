//! LLM contract crate — provider trait, prompt templates, and session types.
//!
//! Pure contracts only: no dependencies on cloud, db, or domain crates.
//! Cloud wires concrete implementations (e.g., Minimax) behind these traits.
//!
//! ## 设计不变量
//! - 只定义 LLM provider 契约与值类型；具体 provider 实现在组合层


pub mod prompt;
pub mod provider;
pub mod session;

pub use provider::{LlmCallMetadata, LlmProvider, LlmResponse};

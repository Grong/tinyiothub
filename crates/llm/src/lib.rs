#![allow(clippy::double_must_use)] // async_trait 展开的 BoxFuture 自带 must_use，属已知误报类
//! LLM contract crate — provider trait, prompt templates, and session types.
//!
//! Pure contracts only: no dependencies on cloud, db, or domain crates.
//! Cloud wires concrete implementations (e.g., Minimax) behind these traits.
//!
//! ## 设计不变量
//! - 只定义 LLM provider 契约与值类型；具体 provider 实现在组合层
//! - 禁止依赖 db、web 与任何领域 crate

pub mod prompt;
pub mod provider;
pub mod session;

pub use provider::{LlmCallMetadata, LlmProvider, LlmResponse};

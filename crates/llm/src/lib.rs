//! LLM contract crate — provider trait, prompt templates, and session types.
//!
//! Pure contracts only: no dependencies on cloud, db, or domain crates.
//! Cloud wires concrete implementations (e.g., Minimax) behind these traits.

pub mod prompt;
pub mod provider;
pub mod session;

pub use provider::{LlmCallMetadata, LlmProvider, LlmResponse};

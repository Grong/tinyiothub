//! Prompt Management — versioned prompt templates for Agent heartbeat and memory.
//!
//! Cloud implements PromptRegistry (e.g., filesystem loader from templates/).
//! AI crate defines the types and trait, keeping prompts provider-agnostic.

pub mod types;

use async_trait::async_trait;
pub use types::PromptTemplate;

/// Registry for versioned prompt templates.
/// Cloud implements this to load prompts from filesystem, DB, or embedded.
#[async_trait]
pub trait PromptRegistry: Send + Sync {
    /// Resolve a prompt template by name, optionally at a specific version.
    /// Returns the latest version if `version` is None.
    async fn resolve(&self, name: &str, version: Option<u32>) -> Option<PromptTemplate>;

    /// List available prompt names.
    async fn list_names(&self) -> Vec<String>;
}

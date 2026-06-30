//! Prompt template types — versioned, with variable substitution and fallback.

use serde::{Deserialize, Serialize};

/// A versioned prompt template with variable placeholders.
///
/// Variables use `${key}` syntax and are substituted at render time.
/// If rendering fails, the `fallback_version` is used as a rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Unique name (e.g., "heartbeat", "reflection", "compile_profile").
    pub name: String,
    /// Monotonic version number.
    pub version: u32,
    /// The template content with `${variable}` placeholders.
    pub content: String,
    /// Variable names expected by this template (for validation).
    #[serde(default)]
    pub variables: Vec<String>,
    /// Fall back to this version if rendering fails.
    #[serde(default)]
    pub fallback_version: Option<u32>,
    /// ISO 8601 timestamp.
    #[serde(default)]
    pub created_at: String,
    /// Author or system that created this version.
    #[serde(default)]
    pub created_by: String,
}

impl PromptTemplate {
    /// Render the template by substituting `${key}` placeholders with values.
    pub fn render(&self, params: &std::collections::HashMap<String, String>) -> String {
        let mut result = self.content.clone();
        for (key, value) in params {
            let placeholder = format!("${{{}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }
}

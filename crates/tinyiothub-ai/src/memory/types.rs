//! Memory types — facts, safety limits, and errors.
//!
//! MemoryFact: parsed representation of a fact extracted from conversation.
//! Used by the reflection engine to auto-accept or enqueue for review.

use serde::{Deserialize, Serialize};

/// A fact extracted from conversation by the reflection engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryFact {
    /// The fact statement itself.
    pub fact: String,
    /// Memory zone: "general", "work", "episode", "core".
    pub zone: String,
    /// Confidence level: "high", "medium", "low".
    pub confidence: String,
    /// Optional tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// If present, this fact supersedes a prior one.
    #[serde(default)]
    pub supersedes: Option<String>,
}

/// Maximum input length for reflection (prompt injection defense).
pub const MAX_REFLECTION_INPUT_CHARS: usize = 32_000;

/// Patterns that indicate prompt injection attempts.
pub const INJECTION_PATTERNS: &[&str] = &[
    "You are",
    "System:",
    "Instructions:",
    "Ignore previous",
    "New instructions:",
];

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Reflection failed: {0}")]
    Reflection(String),
    #[error("Repository error: {0}")]
    Repository(String),
}

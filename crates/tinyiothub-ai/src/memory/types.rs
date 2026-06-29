//! Memory types — facts, safety limits, and errors.

use serde::{Deserialize, Serialize};

/// A fact extracted from conversation for long-term memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    pub key: String,
    pub value: String,
    pub confidence: f32,
    pub source_turn_index: usize,
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

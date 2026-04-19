//! Error context for adding rich diagnostics.

use std::collections::HashMap;

/// Additional context attached to an error.
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    pub message: String,
    pub tags: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            tags: HashMap::new(),
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

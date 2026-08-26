use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// Event identifier value object
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(String);

impl EventId {
    /// Create a new unique event ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Create an event ID from a string (for reconstruction from storage)
    pub fn from_id(id: String) -> Self {
        Self(id)
    }

    /// Create an event ID from a string (alternative method name)
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to owned string
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Display for EventId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EventId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for EventId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

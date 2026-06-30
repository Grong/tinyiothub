//! Session types — chat messages and session key parsing.

use serde::{Deserialize, Serialize};

/// A single turn in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatTurnMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub timestamp: Option<String>,
}

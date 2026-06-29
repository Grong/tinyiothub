//! Session types — stub. Real implementation in a later task.

use serde::{Deserialize, Serialize};

/// Stub ChatTurnMessage — satisfies event/types.rs dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnMessage {
    pub role: String,
    pub content: String,
}

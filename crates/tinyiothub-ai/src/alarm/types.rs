//! Alarm types — stub. Real implementation in a later task.

use serde::{Deserialize, Serialize};

/// Stub Alarm type — satisfies event/types.rs dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub workspace_id: String,
}

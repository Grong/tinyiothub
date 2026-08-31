use serde::{Deserialize, Serialize};

/// Lightweight thing info for listing (avoids sending full Thing internals)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingInfo {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub status: String,
    pub driver_name: Option<String>,
}

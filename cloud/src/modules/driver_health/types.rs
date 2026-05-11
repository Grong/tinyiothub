use serde::{Deserialize, Serialize};

/// Health status for a single loaded driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DriverHealthEntry {
    pub driver_name: String,
    pub version: String,
    pub loaded_at: String,
    pub status: String,
    pub ref_count: usize,
}

/// Overall driver health summary for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceDriverHealth {
    pub workspace_id: String,
    pub drivers: Vec<DriverHealthEntry>,
    pub total_count: usize,
}

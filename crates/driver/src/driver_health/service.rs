use tinyiothub_runtime::driver::registry::DriverRegistry;

use super::types::{DriverHealthEntry, WorkspaceDriverHealth};

pub struct DriverHealthService;

impl DriverHealthService {
    /// Get health information for all drivers loaded in a workspace.
    pub fn get_workspace_health(
        registry: &DriverRegistry,
        workspace_id: &str,
    ) -> WorkspaceDriverHealth {
        let drivers = registry.list_for_workspace(workspace_id);
        let entries: Vec<DriverHealthEntry> = drivers
            .into_iter()
            .map(|(name, version, loaded_at, ref_count)| DriverHealthEntry {
                driver_name: name,
                version,
                loaded_at: loaded_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                status: if ref_count > 0 { "active".to_string() } else { "idle".to_string() },
                ref_count,
            })
            .collect();

        WorkspaceDriverHealth {
            workspace_id: workspace_id.to_string(),
            total_count: entries.len(),
            drivers: entries,
        }
    }
}

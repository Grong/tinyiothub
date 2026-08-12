//! Factory for creating tenant-aware device repositories.

use std::sync::Arc;

use crate::Database;
use crate::device::DeviceRepository;

/// Factory for creating tenant-aware device repositories
#[derive(Debug, Clone)]
pub struct DeviceRepositoryFactory {
    database: Arc<Database>,
    inner_repo: Arc<DeviceRepository>,
}

impl DeviceRepositoryFactory {
    /// Create a new device repository factory
    pub fn new(database: Arc<Database>) -> Self {
        let inner_repo = Arc::new(DeviceRepository::new(database.as_ref().clone()));
        Self { database, inner_repo }
    }

    /// Create a tenant-aware device repository for the given workspace
    pub fn create_for_workspace(&self, workspace_id: String) -> Arc<DeviceRepository> {
        Arc::new(self.inner_repo.as_ref().clone().for_workspace(workspace_id))
    }
}

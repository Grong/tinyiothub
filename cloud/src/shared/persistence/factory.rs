use std::sync::Arc;

use tinyiothub_storage::traits::device::DeviceRepository;
use tinyiothub_storage::sqlite::device::SqliteDeviceRepository;

use crate::shared::persistence::{
    adapters::device::TenantDeviceRepository,
    database::Database,
};

/// Factory for creating tenant-aware device repositories
#[derive(Debug, Clone)]
pub struct DeviceRepositoryFactory {
    database: Arc<Database>,
    inner_repo: Arc<SqliteDeviceRepository>,
}

impl DeviceRepositoryFactory {
    /// Create a new device repository factory
    pub fn new(database: Arc<Database>) -> Self {
        let inner_repo = Arc::new(SqliteDeviceRepository::new(database.as_ref().clone()));
        Self { database, inner_repo }
    }

    /// Create a tenant-aware device repository for the given workspace
    pub fn create_for_workspace(&self, workspace_id: String) -> Arc<dyn DeviceRepository> {
        Arc::new(TenantDeviceRepository::new(
            self.inner_repo.as_ref().clone(),
            workspace_id,
            self.database.as_ref().clone(),
        ))
    }
}
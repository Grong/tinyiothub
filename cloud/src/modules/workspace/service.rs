use std::sync::Arc;

use super::repo::WorkspaceRepository;
use super::types::{Workspace, WorkspaceWithDeviceCount};
use crate::shared::error::Result;

pub struct WorkspaceService {
    repository: Arc<dyn WorkspaceRepository>,
}

impl WorkspaceService {
    pub fn new(repository: Arc<dyn WorkspaceRepository>) -> Self {
        Self { repository }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<WorkspaceWithDeviceCount>> {
        self.repository.find_by_id(id).await
    }

    pub async fn find_by_tenant(
        &self,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithDeviceCount>> {
        self.repository.find_by_tenant(tenant_id, page, page_size).await
    }

    pub async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Workspace> {
        self.repository
            .create(tenant_id, name, description, agent_id, agent_config)
            .await
    }

    pub async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Option<WorkspaceWithDeviceCount>> {
        self.repository
            .update(id, name, description, agent_id, agent_config)
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.repository.delete(id).await
    }

    pub async fn assign_device(&self, device_id: &str, workspace_id: &str) -> Result<()> {
        self.repository.assign_device(device_id, workspace_id).await
    }
}

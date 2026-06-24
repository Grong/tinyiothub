pub mod knowledge;

use std::sync::Arc;

use super::{
    repo::WorkspaceRepository,
    types::{
        ResourceSearchResult, ResourceType, Workspace, WorkspaceResource, WorkspaceWithDeviceCount,
    },
};
use crate::{modules::agent::heartbeat_manager::HeartbeatManager, shared::error::Result};

pub struct WorkspaceService {
    repository: Arc<dyn WorkspaceRepository>,
    heartbeat_manager: Option<Arc<HeartbeatManager>>,
}

impl WorkspaceService {
    pub fn new(repository: Arc<dyn WorkspaceRepository>) -> Self {
        Self { repository, heartbeat_manager: None }
    }

    pub fn set_heartbeat_manager(&mut self, hm: Arc<HeartbeatManager>) {
        self.heartbeat_manager = Some(hm);
    }

    pub async fn list_all_ids(&self) -> Result<Vec<String>> {
        self.repository.find_all_ids().await
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
        let workspace = self.repository.create(tenant_id, name, description, agent_id, agent_config).await?;
        if let Some(ref hm) = self.heartbeat_manager {
            hm.start(&workspace.id).await;
        }
        Ok(workspace)
    }

    pub async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Option<WorkspaceWithDeviceCount>> {
        self.repository.update(id, name, description, agent_id, agent_config).await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        if let Some(ref hm) = self.heartbeat_manager {
            hm.stop(id).await;
        }
        self.repository.delete(id).await
    }

    pub async fn assign_device(&self, device_id: &str, workspace_id: &str) -> Result<()> {
        self.repository.assign_device(device_id, workspace_id).await
    }

    pub async fn list_resources(
        &self,
        workspace_id: &str,
        resource_type: Option<ResourceType>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceResource>> {
        self.repository.list_resources(workspace_id, resource_type, page, page_size).await
    }

    pub async fn find_resource_by_id(
        &self,
        workspace_id: &str,
        resource_id: &str,
    ) -> Result<Option<WorkspaceResource>> {
        self.repository.find_resource_by_id(workspace_id, resource_id).await
    }

    pub async fn create_resource(
        &self,
        workspace_id: &str,
        resource_type: ResourceType,
        name: &str,
        description: Option<&str>,
        file_path: &str,
        tags: &[String],
        metadata: Option<&str>,
    ) -> Result<WorkspaceResource> {
        self.repository
            .create_resource(
                workspace_id,
                resource_type,
                name,
                description,
                file_path,
                tags,
                metadata,
            )
            .await
    }

    pub async fn update_resource(
        &self,
        workspace_id: &str,
        resource_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&[String]>,
        metadata: Option<&str>,
    ) -> Result<Option<WorkspaceResource>> {
        self.repository
            .update_resource(workspace_id, resource_id, name, description, tags, metadata)
            .await
    }

    pub async fn delete_resource(&self, workspace_id: &str, resource_id: &str) -> Result<()> {
        // Delete file first, then DB record
        if let Ok(Some(res)) = self.repository.find_resource_by_id(workspace_id, resource_id).await
        {
            let base_dir = crate::shared::paths::workspace_dir(workspace_id);
            let file_path = base_dir.join("resources").join(&res.file_path);
            if file_path.exists() {
                let _ = tokio::fs::remove_file(&file_path).await;
            }
        }
        self.repository.delete_resource(workspace_id, resource_id).await
    }

    pub async fn search_resources(
        &self,
        workspace_id: &str,
        query: &str,
        resource_type: Option<ResourceType>,
        limit: i64,
    ) -> Result<Vec<ResourceSearchResult>> {
        self.repository.search_resources(workspace_id, query, resource_type, limit).await
    }
}

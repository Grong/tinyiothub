use async_trait::async_trait;
use crate::dto::entity::workspace::{Workspace, WorkspaceWithDeviceCount};
use crate::shared::error::Result;

#[async_trait]
pub trait WorkspaceRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<WorkspaceWithDeviceCount>>;
    async fn find_by_tenant(&self, tenant_id: &str, page: Option<u32>, page_size: Option<u32>) -> Result<Vec<WorkspaceWithDeviceCount>>;
    async fn create(&self, tenant_id: &str, name: &str, description: Option<&str>, agent_id: Option<&str>, agent_config: Option<&str>) -> Result<Workspace>;
    async fn update(&self, id: &str, name: Option<&str>, description: Option<&str>, agent_config: Option<&str>) -> Result<Option<WorkspaceWithDeviceCount>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn assign_device(&self, device_id: &str, workspace_id: &str) -> Result<()>;
}

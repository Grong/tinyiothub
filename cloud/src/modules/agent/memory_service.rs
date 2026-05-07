// Memory Service - Memory context building for AI Agent

use std::sync::Arc;

use super::{
    device_memory::DeviceMemory,
    types::{DeviceSnapshot, MemoryContext, MemoryError},
};
use crate::shared::persistence::repositories::device_memory_repository_impl::SqliteDeviceMemoryRepository;

/// Service for managing agent memory and building context
pub struct AgentMemoryService {
    repo: Arc<SqliteDeviceMemoryRepository>,
}

impl AgentMemoryService {
    pub fn new(repo: Arc<SqliteDeviceMemoryRepository>) -> Self {
        Self { repo }
    }

    pub async fn build_context(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<MemoryContext, MemoryError> {
        let mut context = MemoryContext::new();

        let memories = self
            .repo
            .get_all_for_agent(workspace_id, agent_id)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;

        for memory in memories {
            match DeviceSnapshot::from_domain(&memory) {
                Ok(snapshot) => context.add_device_snapshot(snapshot),
                Err(e) => {
                    tracing::warn!("Failed to parse device snapshot: {}", e);
                }
            }
        }

        Ok(context)
    }

    pub async fn save_device_snapshot(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        snapshot_data: serde_json::Value,
    ) -> Result<(), MemoryError> {
        let memory = DeviceMemory::new(
            workspace_id.to_string(),
            agent_id.to_string(),
            device_id.to_string(),
            snapshot_data,
        );
        self.repo.save(&memory).await.map_err(|e| MemoryError::RepositoryError(e.to_string()))
    }

    pub async fn get_latest_device(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceSnapshot>, MemoryError> {
        let memory = self
            .repo
            .get_latest(workspace_id, agent_id, device_id)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;

        match memory {
            Some(m) => Ok(Some(DeviceSnapshot::from_domain(&m)?)),
            None => Ok(None),
        }
    }

    pub async fn build_memory_prompt(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<String, MemoryError> {
        let context = self.build_context(workspace_id, agent_id).await?;
        Ok(context.to_prompt_fragment())
    }

    pub async fn prune_old_snapshots(
        &self,
        workspace_id: &str,
        agent_id: &str,
        device_id: &str,
        keep_count: i64,
    ) -> Result<u64, MemoryError> {
        self.repo
            .delete_old(workspace_id, agent_id, device_id, keep_count)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))
    }

    pub async fn get_tracked_devices(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, MemoryError> {
        let memories = self
            .repo
            .get_all_for_agent(workspace_id, agent_id)
            .await
            .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;

        let device_ids: std::collections::HashSet<String> =
            memories.into_iter().map(|m| m.device_id).collect();
        Ok(device_ids.into_iter().collect())
    }

    pub async fn clear_agent_memory(
        &self,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<u64, MemoryError> {
        let devices = self.get_tracked_devices(workspace_id, agent_id).await?;
        let mut total_deleted = 0u64;
        for device_id in devices {
            let count = self
                .repo
                .delete_old(workspace_id, agent_id, &device_id, 0)
                .await
                .map_err(|e| MemoryError::RepositoryError(e.to_string()))?;
            total_deleted += count;
        }
        Ok(total_deleted)
    }
}

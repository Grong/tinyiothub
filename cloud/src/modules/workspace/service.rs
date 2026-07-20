pub mod knowledge;

use std::sync::{Arc, Mutex};

use tinyiothub_ai::event::{bus::AiEventPublisher, types::AiEvent};
use tinyiothub_ai::heartbeat::repo::HeartbeatTaskRepository;
use tinyiothub_ai::heartbeat::types::NewHeartbeatTask;

use super::{
    repo::WorkspaceRepository,
    types::{
        ResourceSearchResult, ResourceType, Workspace, WorkspaceResource, WorkspaceWithDeviceCount,
    },
};
use crate::shared::error::Result;

pub struct WorkspaceService {
    repository: Arc<dyn WorkspaceRepository>,
    event_publisher: Mutex<Option<Arc<AiEventPublisher>>>,
    heartbeat_task_repo: Mutex<Option<Arc<dyn HeartbeatTaskRepository>>>,
}

impl WorkspaceService {
    pub fn new(repository: Arc<dyn WorkspaceRepository>) -> Self {
        Self {
            repository,
            event_publisher: Mutex::new(None),
            heartbeat_task_repo: Mutex::new(None),
        }
    }

    pub fn set_event_publisher(&self, publisher: Arc<AiEventPublisher>) {
        *self.event_publisher.lock().unwrap() = Some(publisher);
    }

    pub fn set_heartbeat_task_repo(&self, repo: Arc<dyn HeartbeatTaskRepository>) {
        *self.heartbeat_task_repo.lock().unwrap() = Some(repo);
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
        let workspace =
            self.repository.create(tenant_id, name, description, agent_id, agent_config).await?;
        self.seed_default_heartbeat_tasks(&workspace.id).await;
        if let Some(ref publisher) = *self.event_publisher.lock().unwrap() {
            publisher.publish(AiEvent::WorkspaceCreated { workspace_id: workspace.id.clone() });
        }
        Ok(workspace)
    }

    /// New workspaces start with the default heartbeat task set. Failure to
    /// seed must not fail workspace creation — tasks can be added later.
    async fn seed_default_heartbeat_tasks(&self, workspace_id: &str) {
        let repo = self.heartbeat_task_repo.lock().unwrap().clone();
        let Some(repo) = repo else { return };
        let defaults: Vec<NewHeartbeatTask> = crate::modules::agent::heartbeat::get_default_tasks()
            .into_iter()
            .map(|t| NewHeartbeatTask { priority: t.priority, text: t.text, paused: t.paused })
            .collect();
        if let Err(e) = repo.replace_all(workspace_id, &defaults).await {
            tracing::warn!(%workspace_id, "Failed to seed default heartbeat tasks: {}", e);
        }
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
        if let Some(ref publisher) = *self.event_publisher.lock().unwrap() {
            publisher.publish(AiEvent::WorkspaceDeleted { workspace_id: id.to_string() });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::workspace::types::WorkspaceResource;
    use tinyiothub_ai::heartbeat::repo::HeartbeatTaskRepository;

    struct MockWorkspaceRepository;

    #[async_trait::async_trait]
    impl WorkspaceRepository for MockWorkspaceRepository {
        async fn find_by_id(&self, _id: &str) -> Result<Option<WorkspaceWithDeviceCount>> {
            unimplemented!()
        }
        async fn find_by_tenant(
            &self,
            _tenant_id: &str,
            _page: Option<u32>,
            _page_size: Option<u32>,
        ) -> Result<Vec<WorkspaceWithDeviceCount>> {
            unimplemented!()
        }
        async fn create(
            &self,
            tenant_id: &str,
            name: &str,
            description: Option<&str>,
            _agent_id: Option<&str>,
            _agent_config: Option<&str>,
        ) -> Result<Workspace> {
            Ok(Workspace {
                id: "ws_test".to_string(),
                name: name.to_string(),
                description: description.map(|s| s.to_string()),
                tenant_id: tenant_id.to_string(),
                agent_id: None,
                agent_config: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
        }
        async fn update(
            &self,
            _id: &str,
            _name: Option<&str>,
            _description: Option<&str>,
            _agent_id: Option<&str>,
            _agent_config: Option<&str>,
        ) -> Result<Option<WorkspaceWithDeviceCount>> {
            unimplemented!()
        }
        async fn delete(&self, _id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn assign_device(&self, _device_id: &str, _workspace_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn list_resources(
            &self,
            _workspace_id: &str,
            _resource_type: Option<ResourceType>,
            _page: Option<u32>,
            _page_size: Option<u32>,
        ) -> Result<Vec<WorkspaceResource>> {
            unimplemented!()
        }
        async fn find_resource_by_id(
            &self,
            _workspace_id: &str,
            _resource_id: &str,
        ) -> Result<Option<WorkspaceResource>> {
            unimplemented!()
        }
        async fn create_resource(
            &self,
            _workspace_id: &str,
            _resource_type: ResourceType,
            _name: &str,
            _description: Option<&str>,
            _file_path: &str,
            _tags: &[String],
            _metadata: Option<&str>,
        ) -> Result<WorkspaceResource> {
            unimplemented!()
        }
        async fn update_resource(
            &self,
            _workspace_id: &str,
            _resource_id: &str,
            _name: Option<&str>,
            _description: Option<&str>,
            _tags: Option<&[String]>,
            _metadata: Option<&str>,
        ) -> Result<Option<WorkspaceResource>> {
            unimplemented!()
        }
        async fn delete_resource(&self, _workspace_id: &str, _resource_id: &str) -> Result<()> {
            unimplemented!()
        }
        async fn search_resources(
            &self,
            _workspace_id: &str,
            _query: &str,
            _resource_type: Option<ResourceType>,
            _limit: i64,
        ) -> Result<Vec<ResourceSearchResult>> {
            unimplemented!()
        }
        async fn find_all_ids(&self) -> Result<Vec<String>> {
            unimplemented!()
        }
    }

    async fn heartbeat_repo(
    ) -> crate::modules::agent::heartbeat_repo::SqliteHeartbeatTaskRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("in-memory sqlite");
        for stmt in
            include_str!("../../../migrations/20260629000001_create_heartbeat_tasks.sql")
                .split(';')
        {
            let stmt = stmt.trim();
            if !stmt.is_empty() {
                sqlx::query(stmt).execute(&pool).await.expect("apply migration");
            }
        }
        crate::modules::agent::heartbeat_repo::SqliteHeartbeatTaskRepository::new(pool)
    }

    #[tokio::test]
    async fn create_seeds_default_heartbeat_tasks() {
        let service = WorkspaceService::new(Arc::new(MockWorkspaceRepository));
        let repo = Arc::new(heartbeat_repo().await);
        service.set_heartbeat_task_repo(repo.clone());

        service.create("tenant_1", "ws", None, None, None).await.expect("create");

        let tasks = repo.list_by_workspace("ws_test").await.expect("list tasks");
        assert_eq!(tasks.len(), 4, "new workspace gets the default heartbeat task set");
        assert!(tasks.iter().any(|t| t.priority == "high" && !t.paused));
    }

    #[tokio::test]
    async fn create_without_task_repo_still_succeeds() {
        let service = WorkspaceService::new(Arc::new(MockWorkspaceRepository));
        service.create("tenant_1", "ws", None, None, None).await.expect("create");
    }
}

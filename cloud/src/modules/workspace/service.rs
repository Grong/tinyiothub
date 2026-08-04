use std::sync::{Arc, Mutex};

use tinyiothub_ai::{
    event::{bus::AiEventPublisher, types::AiEvent},
    heartbeat::{repo::HeartbeatTaskRepository, types::NewHeartbeatTask},
};
use tinyiothub_core::agent_hooks::AgentHooks;

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
    agent_hooks: Mutex<Option<Arc<dyn AgentHooks>>>,
}

impl WorkspaceService {
    pub fn new(repository: Arc<dyn WorkspaceRepository>) -> Self {
        Self {
            repository,
            event_publisher: Mutex::new(None),
            heartbeat_task_repo: Mutex::new(None),
            agent_hooks: Mutex::new(None),
        }
    }

    pub fn set_event_publisher(&self, publisher: Arc<AiEventPublisher>) {
        *self.event_publisher.lock().unwrap() = Some(publisher);
    }

    pub fn set_heartbeat_task_repo(&self, repo: Arc<dyn HeartbeatTaskRepository>) {
        *self.heartbeat_task_repo.lock().unwrap() = Some(repo);
    }

    pub fn set_agent_hooks(&self, hooks: Arc<dyn AgentHooks>) {
        *self.agent_hooks.lock().unwrap() = Some(hooks);
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
        let hooks = self.agent_hooks.lock().unwrap().clone();
        let Some(hooks) = hooks else { return };
        let defaults: Vec<NewHeartbeatTask> = hooks
            .default_heartbeat_tasks()
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
        require_action_confirm: Option<bool>,
    ) -> Result<Option<WorkspaceWithDeviceCount>> {
        self.repository
            .update(id, name, description, agent_id, agent_config, require_action_confirm)
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        // Delete first: listeners tear down heartbeat loops and agents on
        // WorkspaceDeleted, so publishing before the row is gone could kill a
        // workspace whose delete then fails.
        self.repository.delete(id).await?;
        if let Some(ref publisher) = *self.event_publisher.lock().unwrap() {
            publisher.publish(AiEvent::WorkspaceDeleted { workspace_id: id.to_string() });
        }
        Ok(())
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
    use tinyiothub_ai::heartbeat::repo::HeartbeatTaskRepository;

    use super::*;
    use crate::modules::workspace::types::WorkspaceResource;

    struct MockWorkspaceRepository {
        delete_fails: std::sync::atomic::AtomicBool,
    }

    impl Default for MockWorkspaceRepository {
        fn default() -> Self {
            Self { delete_fails: std::sync::atomic::AtomicBool::new(false) }
        }
    }

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
                require_action_confirm: true,
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
            _require_action_confirm: Option<bool>,
        ) -> Result<Option<WorkspaceWithDeviceCount>> {
            unimplemented!()
        }
        async fn delete(&self, _id: &str) -> Result<()> {
            if self.delete_fails.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(tinyiothub_core::error::Error::Internal("db down".into()));
            }
            Ok(())
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

    /// In-memory heartbeat task repo — the concrete Sqlite repo lives in the
    /// agent module, which workspace must not reference (P4.0d).
    struct MockHeartbeatTaskRepo {
        tasks: Mutex<Vec<tinyiothub_ai::heartbeat::types::HeartbeatTask>>,
    }

    impl MockHeartbeatTaskRepo {
        fn new() -> Self {
            Self { tasks: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait::async_trait]
    impl HeartbeatTaskRepository for MockHeartbeatTaskRepo {
        async fn list_by_workspace(
            &self,
            workspace_id: &str,
        ) -> std::result::Result<
            Vec<tinyiothub_ai::heartbeat::types::HeartbeatTask>,
            tinyiothub_ai::heartbeat::repo::RepoError,
        > {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.workspace_id == workspace_id)
                .cloned()
                .collect())
        }

        async fn upsert(
            &self,
            _workspace_id: &str,
            _task: &tinyiothub_ai::heartbeat::types::HeartbeatTask,
            _expected_version: i64,
        ) -> std::result::Result<bool, tinyiothub_ai::heartbeat::repo::RepoError> {
            unimplemented!()
        }

        async fn insert(
            &self,
            _workspace_id: &str,
            _priority: &str,
            _text: &str,
        ) -> std::result::Result<
            tinyiothub_ai::heartbeat::types::HeartbeatTask,
            tinyiothub_ai::heartbeat::repo::RepoError,
        > {
            unimplemented!()
        }

        async fn set_paused(
            &self,
            _workspace_id: &str,
            _task_id: i64,
            _paused: bool,
        ) -> std::result::Result<(), tinyiothub_ai::heartbeat::repo::RepoError> {
            unimplemented!()
        }

        async fn delete(
            &self,
            _workspace_id: &str,
            _task_id: i64,
        ) -> std::result::Result<(), tinyiothub_ai::heartbeat::repo::RepoError> {
            unimplemented!()
        }

        async fn replace_all(
            &self,
            workspace_id: &str,
            tasks: &[NewHeartbeatTask],
        ) -> std::result::Result<(), tinyiothub_ai::heartbeat::repo::RepoError> {
            let mut store = self.tasks.lock().unwrap();
            store.retain(|t| t.workspace_id != workspace_id);
            store.extend(tasks.iter().enumerate().map(|(i, t)| {
                tinyiothub_ai::heartbeat::types::HeartbeatTask {
                    id: i as i64 + 1,
                    workspace_id: workspace_id.to_string(),
                    priority: t.priority.clone(),
                    text: t.text.clone(),
                    paused: t.paused,
                    version: 1,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                }
            }));
            Ok(())
        }

        async fn insert_result(
            &self,
            _workspace_id: &str,
            _result: &tinyiothub_ai::heartbeat::types::HeartbeatResult,
        ) -> std::result::Result<(), tinyiothub_ai::heartbeat::repo::RepoError> {
            unimplemented!()
        }
    }

    /// Stub hooks — the real default task set lives in the agent domain; here
    /// we only verify the service seeds whatever the hooks provide.
    struct StubAgentHooks;

    #[async_trait::async_trait]
    impl AgentHooks for StubAgentHooks {
        fn default_heartbeat_tasks(&self) -> Vec<tinyiothub_core::agent_hooks::HeartbeatTaskDef> {
            vec![
                tinyiothub_core::agent_hooks::HeartbeatTaskDef {
                    priority: "high".into(),
                    text: "检查离线设备并尝试自动重连".into(),
                    paused: false,
                },
                tinyiothub_core::agent_hooks::HeartbeatTaskDef {
                    priority: "medium".into(),
                    text: "扫描未处理的高优先级告警".into(),
                    paused: false,
                },
                tinyiothub_core::agent_hooks::HeartbeatTaskDef {
                    priority: "medium".into(),
                    text: "生成设备状态日报摘要".into(),
                    paused: false,
                },
                tinyiothub_core::agent_hooks::HeartbeatTaskDef {
                    priority: "low".into(),
                    text: "检查系统磁盘和内存使用率".into(),
                    paused: true,
                },
            ]
        }

        async fn read_legacy_heartbeat_tasks(
            &self,
            _workspace_dir: &std::path::Path,
        ) -> std::result::Result<Vec<tinyiothub_core::agent_hooks::HeartbeatTaskDef>, String>
        {
            unimplemented!()
        }

        async fn migrate_legacy_heartbeat_tasks(
            &self,
            _workspace_id: &str,
            _workspace_dir: &std::path::Path,
        ) -> std::result::Result<bool, String> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn create_seeds_default_heartbeat_tasks() {
        let service = WorkspaceService::new(Arc::new(MockWorkspaceRepository::default()));
        let repo = Arc::new(MockHeartbeatTaskRepo::new());
        service.set_heartbeat_task_repo(repo.clone());
        service.set_agent_hooks(Arc::new(StubAgentHooks));

        service.create("tenant_1", "ws", None, None, None).await.expect("create");

        let tasks = repo.list_by_workspace("ws_test").await.expect("list tasks");
        assert_eq!(tasks.len(), 4, "new workspace gets the default heartbeat task set");
        assert!(tasks.iter().any(|t| t.priority == "high" && !t.paused));
    }

    #[tokio::test]
    async fn create_without_task_repo_still_succeeds() {
        let service = WorkspaceService::new(Arc::new(MockWorkspaceRepository::default()));
        service.create("tenant_1", "ws", None, None, None).await.expect("create");
    }

    #[tokio::test]
    async fn delete_failure_does_not_publish_workspace_deleted() {
        let repo = Arc::new(MockWorkspaceRepository::default());
        repo.delete_fails.store(true, std::sync::atomic::Ordering::SeqCst);
        let service = WorkspaceService::new(repo);
        let publisher =
            Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())));
        service.set_event_publisher(publisher.clone());

        let result = service.delete("ws_1").await;
        assert!(result.is_err());
        // shutdown() drains the publisher queue deterministically — no sleep.
        publisher.shutdown().await;
        assert_eq!(
            publisher.events_published(),
            0,
            "failed delete must not publish WorkspaceDeleted — listeners would tear down a live workspace"
        );
    }

    #[tokio::test]
    async fn delete_success_publishes_workspace_deleted() {
        let service = WorkspaceService::new(Arc::new(MockWorkspaceRepository::default()));
        let publisher =
            Arc::new(AiEventPublisher::new(Arc::new(tinyiothub_runtime::EventBus::new())));
        service.set_event_publisher(publisher.clone());

        service.delete("ws_1").await.expect("delete");
        publisher.shutdown().await;
        assert_eq!(publisher.events_published(), 1);
    }
}

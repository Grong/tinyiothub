use std::sync::{Arc, Mutex};

use tinyiothub_core::heartbeat::NewHeartbeatTask;

use crate::domains::tenant::hooks::{AgentHooks, WorkspaceEventPublisher};

use tinyiothub_core::error::Result;
use tinyiothub_core::models::workspace::ResourceType;
use tinyiothub_storage::Db;
use tinyiothub_storage::workspace::{ResourceSearchResult, Workspace, WorkspaceResource, WorkspaceWithDeviceCount};

pub struct WorkspaceService {
    db: Arc<Db>,
    event_publisher: Mutex<Option<Arc<dyn WorkspaceEventPublisher>>>,
    heartbeat_task_db: Mutex<Option<Arc<Db>>>,
    agent_hooks: Mutex<Option<Arc<dyn AgentHooks>>>,
}

impl WorkspaceService {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            event_publisher: Mutex::new(None),
            heartbeat_task_db: Mutex::new(None),
            agent_hooks: Mutex::new(None),
        }
    }

    pub fn set_event_publisher(&self, publisher: Arc<dyn WorkspaceEventPublisher>) {
        *self.event_publisher.lock().unwrap() = Some(publisher);
    }

    pub fn set_heartbeat_task_db(&self, db: Arc<Db>) {
        *self.heartbeat_task_db.lock().unwrap() = Some(db);
    }

    pub fn set_agent_hooks(&self, hooks: Arc<dyn AgentHooks>) {
        *self.agent_hooks.lock().unwrap() = Some(hooks);
    }

    pub async fn list_all_ids(&self) -> Result<Vec<String>> {
        self.db.find_all_workspace_ids().await
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<WorkspaceWithDeviceCount>> {
        self.db.find_workspace_by_id(id).await
    }

    pub async fn find_by_tenant(
        &self,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithDeviceCount>> {
        self.db.find_workspaces_by_tenant(tenant_id, page, page_size).await
    }

    pub async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        agent_id: Option<&str>,
        agent_config: Option<&str>,
    ) -> Result<Workspace> {
        let workspace = self
            .db
            .create_workspace(tenant_id, name, description, agent_id, agent_config)
            .await?;
        let seeded = self.seed_default_heartbeat_tasks(&workspace.id).await;
        // Task 9：种子任务在发布 WorkspaceCreated 之前同步推入 agent
        // 运行时内存真源 —— 事件经队列异步派发，其 heartbeat start 回调
        // 读 runner 内存；不先注入则任务集为空、loop 跳过启动。
        if let Some(tasks) = seeded
            && let Some(ref hooks) = *self.agent_hooks.lock().unwrap()
        {
            hooks.heartbeat_tasks_seeded(&workspace.id, tasks);
        }
        if let Some(ref publisher) = *self.event_publisher.lock().unwrap() {
            publisher.publish_workspace_created(workspace.id.clone());
        }
        Ok(workspace)
    }

    /// New workspaces start with the default heartbeat task set. Failure to
    /// seed must not fail workspace creation — tasks can be added later.
    /// 成功时返回 DB 回读的全量任务行（供调用方注入 agent 内存真源）。
    async fn seed_default_heartbeat_tasks(
        &self,
        workspace_id: &str,
    ) -> Option<Vec<tinyiothub_core::heartbeat::HeartbeatTask>> {
        let db = self.heartbeat_task_db.lock().unwrap().clone();
        let db = db?;
        let hooks = self.agent_hooks.lock().unwrap().clone();
        let hooks = hooks?;
        let defaults: Vec<NewHeartbeatTask> = hooks
            .default_heartbeat_tasks()
            .into_iter()
            .map(|t| NewHeartbeatTask {
                priority: t.priority,
                text: t.text,
                paused: t.paused,
            })
            .collect();
        if let Err(e) = db.replace_heartbeat_tasks(workspace_id, &defaults).await {
            tracing::warn!(%workspace_id, "Failed to seed default heartbeat tasks: {}", e);
            return None;
        }
        match db.list_heartbeat_tasks(workspace_id).await {
            Ok(tasks) => Some(tasks),
            Err(e) => {
                tracing::warn!(%workspace_id, "Failed to read back seeded heartbeat tasks: {}", e);
                None
            }
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
        self.db
            .update_workspace(id, name, description, agent_id, agent_config, require_action_confirm)
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        // Delete first: listeners tear down heartbeat loops and agents on
        // WorkspaceDeleted, so publishing before the row is gone could kill a
        // workspace whose delete then fails.
        self.db.delete_workspace(id).await?;
        if let Some(ref publisher) = *self.event_publisher.lock().unwrap() {
            publisher.publish_workspace_deleted(id.to_string());
        }
        Ok(())
    }

    pub async fn assign_device(&self, thing_id: &str, workspace_id: &str) -> Result<()> {
        self.db.assign_device_to_workspace(thing_id, workspace_id).await
    }

    pub async fn list_resources(
        &self,
        workspace_id: &str,
        resource_type: Option<ResourceType>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceResource>> {
        self.db
            .list_workspace_resources(workspace_id, resource_type, page, page_size)
            .await
    }

    pub async fn find_resource_by_id(
        &self,
        workspace_id: &str,
        resource_id: &str,
    ) -> Result<Option<WorkspaceResource>> {
        self.db.find_workspace_resource(workspace_id, resource_id).await
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
        self.db
            .create_workspace_resource(
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
        self.db
            .update_workspace_resource(workspace_id, resource_id, name, description, tags, metadata)
            .await
    }

    pub async fn delete_resource(
        &self,
        workspace_base_dir: &std::path::Path,
        workspace_id: &str,
        resource_id: &str,
    ) -> Result<()> {
        // Delete file first, then DB record
        if let Ok(Some(res)) = self.db.find_workspace_resource(workspace_id, resource_id).await {
            let file_path = workspace_base_dir.join("resources").join(&res.file_path);
            if file_path.exists() {
                let _ = tokio::fs::remove_file(&file_path).await;
            }
        }
        self.db.delete_workspace_resource(workspace_id, resource_id).await
    }

    pub async fn search_resources(
        &self,
        workspace_id: &str,
        query: &str,
        resource_type: Option<ResourceType>,
        limit: i64,
    ) -> Result<Vec<ResourceSearchResult>> {
        self.db
            .search_workspace_resources(workspace_id, query, resource_type, limit)
            .await
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::domains::tenant::hooks::HeartbeatTaskDef;

    /// Tenant-local stub of the agent-owned default-task capability — the
    /// real set lives in the agent domain and must not be named here (G5b).
    struct StubAgentHooks;

    impl AgentHooks for StubAgentHooks {
        fn default_heartbeat_tasks(&self) -> Vec<HeartbeatTaskDef> {
            vec![
                HeartbeatTaskDef {
                    priority: "high".into(),
                    text: "t1".into(),
                    paused: false,
                },
                HeartbeatTaskDef {
                    priority: "medium".into(),
                    text: "t2".into(),
                    paused: false,
                },
                HeartbeatTaskDef {
                    priority: "low".into(),
                    text: "t3".into(),
                    paused: true,
                },
                HeartbeatTaskDef {
                    priority: "low".into(),
                    text: "t4".into(),
                    paused: true,
                },
            ]
        }
    }

    /// Records published workspace lifecycle events (synchronously, so no
    /// drain/shutdown is needed unlike the real queued publisher).
    #[derive(Default)]
    struct RecordingEventPublisher {
        events: std::sync::Mutex<Vec<&'static str>>,
    }

    impl WorkspaceEventPublisher for RecordingEventPublisher {
        fn publish_workspace_created(&self, _workspace_id: String) {
            self.events.lock().unwrap().push("created");
        }
        fn publish_workspace_deleted(&self, _workspace_id: String) {
            self.events.lock().unwrap().push("deleted");
        }
    }

    impl RecordingEventPublisher {
        fn count(&self) -> usize {
            self.events.lock().unwrap().len()
        }
    }

    /// 真实 SQLite 版 Db（E4 去 trait 后替代 MockWorkspaceRepository）。
    /// 返回 (db, pool)：pool 用于 delete-failure 测试 DROP TABLE 注入故障。
    async fn real_db() -> (Arc<Db>, sqlx::SqlitePool) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("in-memory sqlite");
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        // workspaces.tenant_id FK：预置 tenant_1（测试租户）
        // tenants.plan_id → subscription_plans FK。seed_system（Task 3）会预置 plan_free，但此夹具不跑 seed_system，故保留此行。
        sqlx::query("INSERT INTO subscription_plans (id, name, display_name) VALUES ('plan_free', 'free', 'Free')")
            .execute(&pool)
            .await
            .expect("seed plan");
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ('tenant_1', 'T1', 't1')")
            .execute(&pool)
            .await
            .expect("seed tenant");
        (Arc::new(tinyiothub_storage::Db::new(pool.clone())), pool)
    }

    #[tokio::test]
    async fn create_seeds_default_heartbeat_tasks() {
        let (db, _pool) = real_db().await;
        let service = WorkspaceService::new(db);
        let hb_pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("in-memory sqlite");
        tinyiothub_storage::test_helpers::run_all_migrations(&hb_pool)
            .await
            .expect("migrations");
        let db = Arc::new(tinyiothub_storage::Db::new(hb_pool));
        service.set_heartbeat_task_db(db.clone());
        service.set_agent_hooks(Arc::new(StubAgentHooks));

        let ws = service
            .create("tenant_1", "ws", None, None, None)
            .await
            .expect("create");

        let tasks = db.list_heartbeat_tasks(&ws.id).await.expect("list tasks");
        assert_eq!(tasks.len(), 4, "new workspace gets the default heartbeat task set");
        assert!(tasks.iter().any(|t| t.priority == "high" && !t.paused));
    }

    #[tokio::test]
    async fn create_without_task_repo_still_succeeds() {
        let (db, _pool) = real_db().await;
        let service = WorkspaceService::new(db);
        service
            .create("tenant_1", "ws", None, None, None)
            .await
            .expect("create");
    }

    #[tokio::test]
    async fn delete_failure_does_not_publish_workspace_deleted() {
        let (db, pool) = real_db().await;
        // 故障注入：DROP workspaces 表使 delete 必然报错（等效原 mock 的 delete_fails）
        sqlx::query("DROP TABLE workspaces")
            .execute(&pool)
            .await
            .expect("drop table");
        let service = WorkspaceService::new(db);
        let publisher = Arc::new(RecordingEventPublisher::default());
        service.set_event_publisher(publisher.clone());

        let result = service.delete("ws_1").await;
        assert!(result.is_err());
        assert_eq!(
            publisher.count(),
            0,
            "failed delete must not publish WorkspaceDeleted — listeners would tear down a live workspace"
        );
    }

    #[tokio::test]
    async fn delete_success_publishes_workspace_deleted() {
        let (db, _pool) = real_db().await;
        let service = WorkspaceService::new(db);
        let publisher = Arc::new(RecordingEventPublisher::default());
        service.set_event_publisher(publisher.clone());

        service.delete("ws_1").await.expect("delete");
        assert_eq!(publisher.count(), 1);
    }
}

use std::sync::{Arc, Mutex};

use crate::domains::tenant::hooks::WorkspaceEventPublisher;

use tinyiothub_core::error::Result;
use tinyiothub_core::models::workspace::ResourceType;
use tinyiothub_storage::Db;
use tinyiothub_storage::workspace::{ResourceSearchResult, Workspace, WorkspaceResource, WorkspaceWithThingCount};

pub struct WorkspaceService {
    db: Arc<Db>,
    event_publisher: Mutex<Option<Arc<dyn WorkspaceEventPublisher>>>,
}

impl WorkspaceService {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            event_publisher: Mutex::new(None),
        }
    }

    pub fn set_event_publisher(&self, publisher: Arc<dyn WorkspaceEventPublisher>) {
        *self.event_publisher.lock().unwrap() = Some(publisher);
    }

    pub async fn list_all_ids(&self) -> Result<Vec<String>> {
        self.db.find_all_workspace_ids().await
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<WorkspaceWithThingCount>> {
        self.db.find_workspace_by_id(id).await
    }

    pub async fn find_by_tenant(
        &self,
        tenant_id: &str,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<WorkspaceWithThingCount>> {
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
        // 新工作区不再种 legacy heartbeat tasks；唤醒步骤来自 inspection 技能。
        if let Err(error) = crate::domains::agent::session::ensure_inspection_skill(&self.db, &workspace.id).await {
            tracing::warn!(workspace_id = %workspace.id, %error, "seed inspection skill failed");
        }
        if let Some(ref publisher) = *self.event_publisher.lock().unwrap() {
            publisher.publish_workspace_created(workspace.id.clone());
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
        require_action_confirm: Option<bool>,
    ) -> Result<Option<WorkspaceWithThingCount>> {
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

    pub async fn assign_thing(&self, thing_id: &str, workspace_id: &str) -> Result<()> {
        self.db.assign_thing_to_workspace(thing_id, workspace_id).await
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
    async fn create_does_not_seed_legacy_heartbeat_tasks() {
        let (db, _pool) = real_db().await;
        let service = WorkspaceService::new(db.clone());
        let ws = service
            .create("tenant_1", "ws", None, None, None)
            .await
            .expect("create");

        // Wake steps now live in the inspection skill, not heartbeat_tasks.
        let tasks = db.list_heartbeat_tasks(&ws.id).await.expect("list tasks");
        assert!(tasks.is_empty(), "new workspace must not seed legacy tasks");
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

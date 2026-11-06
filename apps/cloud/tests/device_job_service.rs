use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use tinyiothub_cloud::shared::runtime_ports::WorkspaceDeviceJobStore;
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
use tinyiothub_core::device_job::DeviceJobState;
use tinyiothub_core::driver::manifest::{ManifestTool, ToolKindDecl, ToolSafetyDecl, ToolTimeout};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_runtime::DataServer;
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver, device_claim};
use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::job::service::{JobFinal, JobRequest, JobService, JobWait};
use tinyiothub_runtime::job::worker_adapter::{DataServerJobWorkers, JobDeviceWorker, JobWorkerLocator};
use tinyiothub_storage::Db;
use tinyiothub_storage::cache::ThingCache;

struct Validator(u64);

#[async_trait]
impl AccessCallValidator for Validator {
    async fn validate(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        tool: &str,
        _params: Option<&Value>,
    ) -> Result<ValidatedCall, CallError> {
        Ok(ValidatedCall {
            tool: ManifestTool {
                name: tool.to_string(),
                kind: ToolKindDecl::Action,
                target_field: None,
                parameters: json!({"type":"object"}),
                timeout: ToolTimeout {
                    default_ms: self.0,
                    max_ms: self.0,
                },
                safety: ToolSafetyDecl::Control,
                resources: vec![],
                cancellable: true,
            },
            timeout_ms: self.0,
            clamped: false,
        })
    }
}

struct Resources;

#[async_trait]
impl ResourceResolver for Resources {
    async fn resolve_action(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _tool_name: &str,
    ) -> Result<Vec<ResourceClaim>, String> {
        Ok(vec![device_claim()])
    }

    async fn resolve_properties(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _properties: &[String],
    ) -> Result<Vec<ResourceClaim>, String> {
        Ok(vec![device_claim()])
    }
}

#[derive(Default)]
struct Worker;

impl JobDeviceWorker for Worker {
    fn submit(&self, cmd: ThingCommand, _job_id: &str, _lease: Option<ResourceLease>) -> Result<(), Error> {
        assert_eq!(cmd.thing_id, "dev_1");
        Ok(())
    }

    fn cancel_job(&self, _job_id: &str) -> bool {
        false
    }

    fn has_recoverable_call(&self, _job_id: &str) -> bool {
        true
    }

    fn complete_job(&self, _job_id: &str) {}
}

struct Workers;

impl JobWorkerLocator for Workers {
    fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
        Some(Arc::new(Worker))
    }
}

async fn seed_task(db: &Db, task_id: &str) {
    let now = Utc::now();
    let task = AgentTask {
        id: task_id.to_string(),
        workspace_id: "ws_1".to_string(),
        idempotency_key: format!("key_{task_id}"),
        objective: "exercise real device job persistence".to_string(),
        session_key: None,
        created_by: "user_1".to_string(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    };
    let wake = TaskWake {
        task_id: task_id.to_string(),
        event_id: format!("create_{task_id}"),
        signal_json: json!({"workspace_id":"ws_1"}).to_string(),
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    };
    db.harness_create_task_with_wake(&task, &wake).await.expect("seed task");
}

async fn wake_count(db: &Db, task_id: &str) -> i64 {
    // 19C3: terminal wakes live in the workspace Harness generation.
    let pool = db.workspace_pool_for_testing("ws_1").await.expect("workspace pool");
    sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE task_id = ? AND event_id LIKE 'device_job:%'")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("count job wakes")
}

fn request(job_id: &str, task_id: &str) -> JobRequest {
    JobRequest {
        job_id: Some(job_id.to_string()),
        workspace_id: "ws_1".to_string(),
        device_id: "dev_1".to_string(),
        tool: "set_speed".to_string(),
        args_json: json!({"speed":2}),
        task_id: Some(task_id.to_string()),
        run_id: Some(format!("run_{task_id}")),
        tool_call_id: Some(format!("call_{job_id}")),
    }
}

#[tokio::test]
async fn service_terminal_results_and_wakes_use_real_db() {
    let pool = tinyiothub_storage::test_helpers::test_pool().await;
    sqlx::query("INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')")
        .execute(&pool)
        .await
        .expect("seed plan");
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
        .execute(&pool)
        .await
        .expect("seed tenant");
    sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws_1', 'workspace', 't1', 'now', 'now')")
        .execute(&pool)
        .await
        .expect("seed workspace");
    let root = std::env::temp_dir().join(format!("tinyiothub-device-job-service-{}", uuid::Uuid::new_v4()));
    let db = Db::new(pool).with_workspace_store_root(root.join("data/agents"));
    db.create_workspace_store("ws_1").await.expect("workspace store");
    for task_id in ["task_done", "task_cancelled", "task_timeout"] {
        seed_task(&db, task_id).await;
    }

    let service = JobService::new(
        Arc::new(WorkspaceDeviceJobStore::new(Arc::new(db.clone()))),
        Arc::new(Workers),
        Arc::new(Validator(30)),
        Arc::new(Resources),
        Arc::new(ArbitrationRegistry::default()),
    );

    let done = service
        .submit(request("job_done", "task_done"))
        .await
        .expect("submit done");
    assert!(
        service
            .finish(&done.id, JobFinal::Done(json!({"ok":true})))
            .await
            .expect("done")
            .wake_created
    );
    assert!(
        !service
            .finish(&done.id, JobFinal::Done(json!({"ok":true})))
            .await
            .expect("duplicate")
            .wake_created
    );
    assert_eq!(wake_count(&db, "task_done").await, 1);

    let cancelled = service
        .submit(request("job_cancelled", "task_cancelled"))
        .await
        .expect("submit cancelled");
    service
        .finish(&cancelled.id, JobFinal::Cancelled { within_grace: true })
        .await
        .expect("cancelled");
    assert_eq!(wake_count(&db, "task_cancelled").await, 1);

    let timeout = service
        .submit(request("job_timeout", "task_timeout"))
        .await
        .expect("submit timeout");
    let JobWait::Final(expired) = service.wait(&timeout.id, Duration::from_secs(1)).await else {
        panic!("timeout must be terminal");
    };
    assert_eq!(expired.state, DeviceJobState::Failed);
    assert_eq!(wake_count(&db, "task_timeout").await, 1);
    let _ = tokio::fs::remove_dir_all(&root).await;
}

#[tokio::test]
async fn production_data_server_and_job_service_do_not_retain_each_other() {
    let db = Db::new(tinyiothub_storage::test_helpers::test_pool().await);
    let data_server = Arc::new(DataServer::new(
        Arc::new(tinyiothub_cloud::shared::runtime_ports::ThingCacheAdapter(Arc::new(
            ThingCache::new(),
        ))),
        Arc::new(EventBus::new()),
    ));
    let service = Arc::new(JobService::new(
        Arc::new(WorkspaceDeviceJobStore::new(Arc::new(db))),
        Arc::new(DataServerJobWorkers::new(data_server.as_ref().clone())),
        Arc::new(Validator(30)),
        Arc::new(Resources),
        Arc::new(ArbitrationRegistry::default()),
    ));
    data_server.set_job_result_sink(service.clone());
    let weak_server = Arc::downgrade(&data_server);
    let weak_service = Arc::downgrade(&service);

    drop(service);
    drop(data_server);

    assert!(
        weak_server.upgrade().is_none() && weak_service.upgrade().is_none(),
        "the production DataServer/JobService pair must be released with its external owners"
    );
}

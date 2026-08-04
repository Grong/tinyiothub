//! Agent-side implementation of `tinyiothub_core::agent_hooks::AgentHooks`.
//!
//! Adapts the agent domain's heartbeat-task knowledge (default task set,
//! legacy `HEARTBEAT.md` parsing, file→DB migration) to the core trait so
//! `modules::workspace` carries no `modules::agent` dependency edge.

use std::path::Path;
use std::sync::Arc;

use tinyiothub_ai::heartbeat::repo::HeartbeatTaskRepository;
use tinyiothub_core::agent_hooks::{AgentHooks, HeartbeatTaskDef};

use crate::modules::agent::heartbeat::{self, HeartbeatTask};

/// [`AgentHooks`] backed by the agent domain's real implementations.
pub struct AgentHooksImpl {
    /// Repo used by the legacy file→DB migration. Same underlying pool as
    /// the heartbeat runner's repo; constructed by the composition layer.
    task_repo: Arc<dyn HeartbeatTaskRepository>,
}

impl AgentHooksImpl {
    pub fn new(task_repo: Arc<dyn HeartbeatTaskRepository>) -> Self {
        Self { task_repo }
    }
}

fn to_def(t: HeartbeatTask) -> HeartbeatTaskDef {
    HeartbeatTaskDef { priority: t.priority, text: t.text, paused: t.paused }
}

#[async_trait::async_trait]
impl AgentHooks for AgentHooksImpl {
    fn default_heartbeat_tasks(&self) -> Vec<HeartbeatTaskDef> {
        heartbeat::get_default_tasks().into_iter().map(to_def).collect()
    }

    async fn read_legacy_heartbeat_tasks(
        &self,
        workspace_dir: &Path,
    ) -> Result<Vec<HeartbeatTaskDef>, String> {
        heartbeat::read_heartbeat_tasks(workspace_dir)
            .await
            .map(|tasks| tasks.into_iter().map(to_def).collect())
            .map_err(|e| e.to_string())
    }

    async fn migrate_legacy_heartbeat_tasks(
        &self,
        workspace_id: &str,
        workspace_dir: &Path,
    ) -> Result<bool, String> {
        heartbeat::migrate_file_tasks_to_db(self.task_repo.as_ref(), workspace_id, workspace_dir)
            .await
            .map_err(|e| e.to_string())
    }
}

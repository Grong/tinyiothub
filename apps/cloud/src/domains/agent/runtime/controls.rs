use std::sync::Arc;

use crate::domains::agent::thing_agent::task::coordinator::TaskError;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use tinyiothub_core::agent_task::TaskStoreError;
use tinyiothub_core::device_job::DeviceJobError;
use tinyiothub_core::harness::{CommandKind, HarnessCommand};
use tinyiothub_storage::{Db, DbError};
use tinyiothub_web::security::Claims;

use super::RecoveryMode;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::runtime::{projection, tasks::error_response};

fn task_error(error: TaskError) -> Response {
    match error {
        TaskError::NotFound => error_response(StatusCode::NOT_FOUND, "Task 不存在"),
        TaskError::Store(TaskStoreError::ConcurrentUpdate) => error_response(StatusCode::CONFLICT, "Task 已并发更新"),
        TaskError::Store(TaskStoreError::Validation(message)) => error_response(StatusCode::CONFLICT, message),
        other => {
            tracing::warn!(%other, "runtime Task control failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用")
        }
    }
}

async fn pause_resume(
    state: AgentState,
    claims: Claims,
    workspace_id: String,
    task_id: String,
    pause: bool,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    let result = if pause {
        manager.pause_task(&workspace_id, &task_id).await
    } else {
        manager.resume_task(&workspace_id, &task_id).await
    };
    match result {
        Ok(_) => projection::response(&state, &workspace_id, &task_id).await,
        Err(error) => task_error(error),
    }
}

pub async fn pause(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    pause_resume(state, claims, workspace_id, task_id, true).await
}

pub async fn resume(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    pause_resume(state, claims, workspace_id, task_id, false).await
}

pub async fn cancel(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    match manager.cancel_task(&workspace_id, &task_id).await {
        Ok(_) => projection::response(&state, &workspace_id, &task_id).await,
        Err(error) => task_error(error),
    }
}

pub async fn stop_job(
    State(state): State<AgentState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, task_id, job_id)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = check_agent_admin_response(&state, &claims, &workspace_id).await {
        return response;
    }
    let Some(manager) = state.thing_agents.as_ref() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用");
    };
    match manager.stop_task_job(&workspace_id, &task_id, &job_id).await {
        Ok(_) => projection::response(&state, &workspace_id, &task_id).await,
        Err(DeviceJobError::NotFound) => error_response(StatusCode::NOT_FOUND, "Device Job 不存在"),
        Err(DeviceJobError::Conflict(message)) | Err(DeviceJobError::Validation(message)) => {
            error_response(StatusCode::CONFLICT, message)
        }
        Err(error) => {
            tracing::warn!(%error, "single Device Job stop failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Device Job 服务不可用")
        }
    }
}

#[derive(Clone)]
pub struct WorkflowControls {
    db: Arc<Db>,
    workspace_id: String,
}

impl WorkflowControls {
    pub fn new(db: Arc<Db>, workspace_id: impl Into<String>) -> Self {
        Self {
            db,
            workspace_id: workspace_id.into(),
        }
    }

    pub async fn pause(&self, task_id: &str) -> Result<(), DbError> {
        self.db.pause_workflow_task(&self.workspace_id, task_id).await
    }

    pub async fn resume(&self, task_id: &str) -> Result<String, DbError> {
        self.db.resume_workflow_task(&self.workspace_id, task_id).await
    }

    pub async fn dispatch_permitted(&self, effect_id: &str) -> Result<bool, DbError> {
        self.db.workflow_dispatch_permitted(&self.workspace_id, effect_id).await
    }

    pub async fn cancel(&self, task_id: &str) -> Result<(), DbError> {
        let task_ids = self.db.cancel_workflow_task(&self.workspace_id, task_id).await?;
        if task_ids.is_empty() {
            return Ok(());
        }
        let recovery = super::EffectRecovery::new(self.db.clone());
        let items = self.db.list_effect_recovery(&self.workspace_id).await?;
        for item in items.into_iter().filter(|item| {
            task_ids.contains(&item.intent.task_id)
                && matches!(
                    item.intent.state,
                    tinyiothub_core::harness::EffectState::Prepared
                        | tinyiothub_core::harness::EffectState::Dispatching
                )
        }) {
            recovery
                .request_cancellation(&self.workspace_id, &item.intent.effect_id)
                .await
                .map_err(DbError::Internal)?;
        }
        self.db.drain_workflow_cancellation(&self.workspace_id).await
    }
}

pub trait CommandControl: Send + Sync {
    fn interrupt_model_call(&self, command: &HarnessCommand);
    fn steer_before_model_call(&self, command: &HarnessCommand);
    fn cancel_model_call(&self, command: &HarnessCommand);
}

#[derive(Clone)]
pub struct CommandRecovery {
    db: Arc<Db>,
}

impl CommandRecovery {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn consume_next(
        &self,
        workspace_id: &str,
        target_task_id: &str,
        now: DateTime<Utc>,
        control: &dyn CommandControl,
    ) -> Result<Option<HarnessCommand>, String> {
        let consumed = self
            .db
            .consume_next_command(workspace_id, target_task_id, now)
            .await
            .map_err(|error| error.to_string())?;
        if let Some(command) = &consumed {
            match command.kind {
                CommandKind::Interrupt => control.interrupt_model_call(command),
                CommandKind::Steer => control.steer_before_model_call(command),
                CommandKind::Cancel => control.cancel_model_call(command),
                CommandKind::Pause | CommandKind::Queue => {}
            }
        }
        Ok(consumed)
    }

    pub async fn recover_workspace(
        &self,
        workspace_id: &str,
        mode: RecoveryMode,
        control: &dyn CommandControl,
    ) -> Result<(), String> {
        let now = Utc::now();
        self.db
            .expire_harness_commands(workspace_id, now)
            .await
            .map_err(|error| error.to_string())?;
        if matches!(mode, RecoveryMode::Restart | RecoveryMode::Restore) {
            let requested = self
                .db
                .list_requested_harness_commands(workspace_id)
                .await
                .map_err(|error| error.to_string())?;
            for command in requested
                .into_iter()
                .filter(|command| command.kind == CommandKind::Interrupt)
            {
                self.db
                    .reject_harness_command(workspace_id, &command.command_id, now, "restart_without_model_call")
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
        let requested = self
            .db
            .list_requested_harness_commands(workspace_id)
            .await
            .map_err(|error| error.to_string())?;
        let mut targets = Vec::new();
        for command in requested {
            if !targets.contains(&command.target_task_id) {
                targets.push(command.target_task_id);
            }
        }
        for target in targets {
            self.consume_next(workspace_id, &target, now, control).await?;
        }
        Ok(())
    }
}

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use tinyiothub_core::device_job::DeviceJobError;
use tinyiothub_core::harness::{CommandKind, HarnessCommand};
use tinyiothub_storage::{Db, DbError};
use tinyiothub_web::security::Claims;

use super::RecoveryMode;

use crate::domains::agent::AgentState;
use crate::domains::agent::host::handler::agent_tasks::check_agent_admin_response;
use crate::domains::agent::runtime::{projection, tasks::error_response};
use crate::domains::agent::thing_agent::task::coordinator::TaskError;
use tinyiothub_core::agent_task::TaskStoreError;

fn control_error(error: DbError) -> Response {
    match error {
        DbError::NotFound { .. } => error_response(StatusCode::NOT_FOUND, "Task 不存在"),
        DbError::Validation { message } => error_response(StatusCode::CONFLICT, message),
        DbError::Conflict { .. } | DbError::CasConflict { .. } => {
            error_response(StatusCode::CONFLICT, "Task 已并发更新")
        }
        other => {
            tracing::warn!(%other, "runtime Task control failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, "Task 服务不可用")
        }
    }
}

/// Error mapping for controls that go through the in-process coordinator. The
/// status codes match the workspace-store mapping so the HTTP contract does not
/// depend on which owner answered.
fn coordinator_error(error: TaskError) -> Response {
    match error {
        TaskError::NotFound => error_response(StatusCode::NOT_FOUND, "Task 不存在"),
        TaskError::Store(TaskStoreError::Validation(message)) => error_response(StatusCode::CONFLICT, message),
        TaskError::Store(TaskStoreError::ConcurrentUpdate) => error_response(StatusCode::CONFLICT, "Task 已并发更新"),
        other => {
            tracing::warn!(%other, "runtime Task coordinator control failed");
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
    // Pause/resume are pure Task-state facts: neither aborts a live token nor
    // cancels an Effect, so the workspace store is authoritative and complete.
    // The coordinator's own resume would re-derive the waiting status from the
    // legacy approval table, which this cutover no longer writes.
    let controls = WorkflowControls::new(state.db.clone(), &workspace_id);
    let result = if pause {
        controls.pause(&task_id).await
    } else {
        controls.resume(&task_id).await.map(|_| ())
    }
    .map_err(control_error);
    match result {
        Ok(_) => {
            state.harness_stream.notify(&workspace_id);
            projection::response(&state, &workspace_id, &task_id).await
        }
        Err(response) => response,
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
    let controls = WorkflowControls::new(state.db.clone(), &workspace_id);
    let result: Result<(), Response> = match state.thing_agents.as_ref() {
        Some(manager) => {
            // The coordinator owns the Task transition, the live Run, and the
            // Job stops, so it runs first. Cancelling an Effect before that
            // transition would write a derived Task state (`cancelled` for a
            // Prepared Effect, `cancelling` for a Dispatching one) into the row
            // and make the coordinator treat the Task as already cancelled.
            // Workflow-owned Subagents are cancelled while the root is still
            // non-`cancelling`, so their workspace drain cannot finalize it.
            if let Err(error) = controls.cancel_children(&task_id).await {
                state.harness_stream.notify(&workspace_id);
                return control_error(error);
            }
            match manager.cancel_task(&workspace_id, &task_id).await {
                Ok(_) => {
                    if let Err(error) = controls.cancel_root_effects_if_terminal(&task_id).await {
                        state.harness_stream.notify(&workspace_id);
                        return control_error(error);
                    }
                    Ok(())
                }
                Err(TaskError::NotFound) => controls.cancel(&task_id).await.map_err(control_error),
                Err(error) => {
                    state.harness_stream.notify(&workspace_id);
                    Err(coordinator_error(error))
                }
            }
        }
        None => controls.cancel(&task_id).await.map_err(control_error),
    };
    match result {
        Ok(()) => {
            state.harness_stream.notify(&workspace_id);
            projection::response(&state, &workspace_id, &task_id).await
        }
        Err(response) => response,
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
    let job_effect = match state.db.harness_task_job(&workspace_id, &task_id, &job_id).await {
        Ok(job_effect) => job_effect,
        Err(error) => return control_error(error),
    };
    if let Some(effect) = job_effect.as_ref().and_then(|job_effect| job_effect.effect.as_ref()) {
        let recovery = super::EffectRecovery::new(state.db.clone());
        if let Err(error) = recovery.request_cancellation(&workspace_id, &effect.effect_id).await {
            tracing::warn!(%workspace_id, %job_id, %error, "single Device Job effect stop failed");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "Device Job 服务不可用");
        }
    }
    // The Harness job is authoritative. The in-process worker manager can still
    // own a Job the workspace projection does not have (legacy platform Jobs),
    // so it is consulted in both cases; only when neither source knows the Job
    // does the route answer 404.
    match state.thing_agents.as_ref() {
        Some(manager) => match manager.stop_task_job(&workspace_id, &task_id, &job_id).await {
            Ok(_) => {}
            // A manager that no longer knows this Job must not turn a
            // committed Harness stop into 404.
            Err(DeviceJobError::NotFound) if job_effect.is_some() => {}
            Err(DeviceJobError::NotFound) => {
                return error_response(StatusCode::NOT_FOUND, "Device Job 不存在");
            }
            Err(DeviceJobError::Conflict(message)) | Err(DeviceJobError::Validation(message)) => {
                return error_response(StatusCode::CONFLICT, message);
            }
            Err(error) => {
                tracing::warn!(%error, "single Device Job stop failed");
                return error_response(StatusCode::SERVICE_UNAVAILABLE, "Device Job 服务不可用");
            }
        },
        None => {
            if job_effect.is_none() {
                return error_response(StatusCode::NOT_FOUND, "Device Job 不存在");
            }
        }
    }
    state.harness_stream.notify(&workspace_id);
    projection::response(&state, &workspace_id, &task_id).await
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
        self.db.guard_recovery_matrix(&self.workspace_id).await?;
        let task_ids = self.db.cancel_workflow_task(&self.workspace_id, task_id).await?;
        if task_ids.is_empty() {
            return Ok(());
        }
        self.cancel_effects(&task_ids).await?;
        self.db.drain_workflow_cancellation(&self.workspace_id).await
    }

    /// Cancel Workflow-owned Subagents and their in-flight Effects before the
    /// coordinator cancels the root. The root is still non-`cancelling` here, so
    /// the workspace-wide drain these calls trigger cannot finalize it.
    /// Subagents do not nest, so one level of children is the whole tree.
    pub async fn cancel_children(&self, task_id: &str) -> Result<(), DbError> {
        self.db.guard_recovery_matrix(&self.workspace_id).await?;
        let mut child_ids = Vec::new();
        for child in self
            .db
            .harness_active_child_task_ids(&self.workspace_id, task_id)
            .await?
        {
            child_ids.extend(self.db.cancel_workflow_task(&self.workspace_id, &child).await?);
        }
        self.cancel_effects(&child_ids).await
    }

    /// Settle the root's own in-flight Effects only once the coordinator has
    /// finalized it. While the coordinator leaves the Task `cancelling` (active
    /// Jobs), cancelling an Effect would trigger the workspace drain, which
    /// cannot see those Jobs and would declare the Task terminal before the
    /// `cancelled_unknown -> failed` decision runs.
    pub async fn cancel_root_effects_if_terminal(&self, task_id: &str) -> Result<(), DbError> {
        let Some(task) = self
            .db
            .harness_get_task(&self.workspace_id, task_id)
            .await
            .map_err(|error| DbError::Internal(error.to_string()))?
        else {
            return Ok(());
        };
        if matches!(
            task.status,
            tinyiothub_core::agent_task::TaskStatus::Completed
                | tinyiothub_core::agent_task::TaskStatus::Failed
                | tinyiothub_core::agent_task::TaskStatus::Cancelled
        ) {
            self.cancel_effects(&[task_id.to_string()]).await?;
        }
        Ok(())
    }

    async fn cancel_effects(&self, task_ids: &[String]) -> Result<(), DbError> {
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
        Ok(())
    }
}

pub trait CommandControl: Send + Sync {
    fn interrupt_model_call(&self, command: &HarnessCommand);
    fn steer_before_model_call(&self, command: &HarnessCommand);
    fn cancel_model_call(&self, command: &HarnessCommand);
}

/// Consumes durable Harness commands (steer / interrupt / queue).
///
/// **Unwired in this branch (L4):** no production route or loop constructs a
/// `CommandRecovery`, so steer/interrupt/queue are not reachable from the app.
/// The DB state machine and its tests are the only callers. Do not assume a
/// live command consumer when writing control-path code.
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

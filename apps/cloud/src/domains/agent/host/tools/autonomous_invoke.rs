//! Autonomous `invoke_action` variant (T11, O18 thin wrapper).
//!
//! Reuses [`InvokeActionTool`] (thing.rs) for validation and dispatch, but
//! replaces the human confirmation-token branch with the T4 autonomy policy
//! gate plus RunContext injection:
//!
//! 1. `gate_check` re-reads the policy on EVERY call (O7 — kill switch is
//!    immediate; DB read failure maps to fail-closed `policy_read_failed`).
//! 2. Deny returns a structured, LLM-readable payload
//!    `{"denied": true, "reason": ...}` — never a tool error.
//! 3. Allow delegates to `inner.execute()`. If the workspace still has
//!    `require_action_confirm` ON, the inner tool answers
//!    `confirmation_required`; the autonomous variant auto-confirms (the
//!    policy gate already authorized this action) and dispatches directly.
//! 4. Every dispatched action is recorded in the `events` table with
//!    `actor = "agent"` (T6 resonance guard handoff) so the thing-event
//!    trigger never wakes the loop on its own output.
//!
//! Per-run/per-thing action cap (O9, `max_actions_per_run`, default 3): the
//! T9 runner increments `RunContextInner::action_counts[thing_id]` at
//! dispatch time — BEFORE zeroclaw calls `Tool::execute` (the ToolCall event
//! is sent first and the consume task is expected to run at this tool's
//! first await point). This tool therefore treats the observed count as
//! "in-flight inclusive" and gates on `count - 1` previously dispatched
//! actions. Denied attempts consume budget too (anti retry-loop fuse).
//!
//! Note: that ordering is a scheduling expectation, not a happens-before
//! guarantee. Under a parallel tool-call batch the consume task may lag, so
//! the observed count can be stale and the gate may OVER-deny (block an
//! action that was still within budget) — the safe direction. The hourly
//! fuse (`max_actions_per_hour`, re-read from the DB on every call) is the
//! deterministic backstop that bounds total spend regardless of ordering.

use std::sync::Arc;

use crate::domains::agent::thing_agent::seams::policy::{DefaultPolicy, PolicyDecision, PolicyInput, PolicySeam};
use crate::domains::agent::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL, RunContextInner};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Digest;
use sqlx::SqlitePool;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_core::harness::{
    CreateEffect, Decision, DecisionKind, DecisionMutation, DecisionState, EffectMutation, EffectState, ExecutionMode,
    HarnessEventInput, Transition, TransitionPrecondition,
};
use tinyiothub_core::models::event::EventLevel;
use tinyiothub_policy::autonomy::AutonomyPolicy;
use tinyiothub_runtime::access::resources::ResourceResolver;
use tinyiothub_storage::Db;
use tokio::sync::RwLock;

use super::thing::{CONFIRMATION_TTL, InvokeActionTool, resource_specs, take_pending_action, tool_err, tool_ok};
use crate::domains::event::{
    bus::ThingEventBus,
    router::{ThingEventInput, ThrottleState, route_thing_event},
};

/// Tool arguments (same contract as the chat `invoke_action`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    // alias：提案/报告的 parameters 常随模板写成蛇形键（thing_id）。
    #[serde(alias = "thing_id")]
    thing_id: String,
    #[serde(alias = "action_name", alias = "action")]
    action_name: String,
    params: Option<Value>,
}

/// Dispatch a device command through the DataServer command queue.
/// Mirrors the dispatch tail in thing.rs — keep in sync (O18 forbids
/// editing thing.rs). Returns the executed/simulated payload, or an error
/// result when the queue rejects it.
///
/// E1/E4c 仲裁：确认门之后的真实下发同样 claim 资源——
/// - `resources` 接线时按工具声明 claim（与 chat 工具路径一致，评审 Issue 3）；
///   `None` 时沿用 E1 合成 `__device__` 每设备互斥。
/// - stop 闸门在归属已确认的调用点先行拒绝（`stopped: <reason>`）。
/// - 租约移交给 worker（`execute_command_with_lease`），claim 覆盖真实执行
///   窗口而不是入队瞬间（评审 Issue 4）。
///
/// pub(crate)：提案批准执行（workspace_heartbeat）的 invoke_action 自动确认
/// 复用同一下发尾巴，避免第三份拷贝。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_command(
    data_server: Option<&Arc<tinyiothub_runtime::DataServer>>,
    workspace_id: &str,
    thing_id: &str,
    action_name: &str,
    params: Option<&Value>,
    arbitration: Option<&ArbitrationRegistry>,
    resources: Option<&dyn ResourceResolver>,
    holder: &str,
) -> ToolResult {
    let lease = match arbitration {
        Some(registry) => {
            if let Some(reason) = registry.stop_reason(thing_id) {
                return tool_err(format!("stopped: {reason}")).expect("static message");
            }
            let acquired = match resources {
                Some(resolver) => match resolver.resolve_action(workspace_id, thing_id, action_name).await {
                    Ok(claims) => registry.try_acquire_resources(
                        thing_id,
                        holder.to_string(),
                        &resource_specs(&claims),
                        DEFAULT_LEASE_TTL,
                    ),
                    Err(e) => return tool_err(format!("资源解析失败: {e}")).expect("static message"),
                },
                None => registry.try_acquire_default(thing_id, holder.to_string()),
            };
            match acquired {
                Ok(lease) => Some(lease),
                Err(busy) if resources.is_some() => return tool_err(busy.to_string()).expect("static message"),
                Err(busy) => {
                    return tool_err(format!("busy: 设备正被 {} 占用，请稍后重试", busy.holder))
                        .expect("static message");
                }
            }
        }
        None => None,
    };
    dispatch_command_with_lease(data_server, thing_id, action_name, params, lease)
}

/// 下发尾巴：调用方已持有租约（或 `None`），租约随命令移交 worker。
///
/// 无 DataServer（模拟路径）时租约在本函数返回时释放。
pub(crate) fn dispatch_command_with_lease(
    data_server: Option<&Arc<tinyiothub_runtime::DataServer>>,
    thing_id: &str,
    action_name: &str,
    params: Option<&Value>,
    lease: Option<tinyiothub_runtime::arbitration::ResourceLease>,
) -> ToolResult {
    match data_server.cloned() {
        Some(data_server) => {
            let cmd = tinyiothub_core::models::thing_command::ThingCommand {
                id: uuid::Uuid::new_v4().to_string(),
                thing_id: thing_id.to_string(),
                name: action_name.to_string(),
                display_name: None,
                description: None,
                parameters: params.map(|p| p.to_string()),
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            match data_server.execute_command_with_lease(cmd, lease) {
                Ok(()) => tool_ok(json!({
                    "thingId": thing_id,
                    "actionName": action_name,
                    "status": "executed",
                    "message": "操作已下发执行"
                }))
                .expect("payload serializes"),
                Err(e) => tool_err(format!("操作执行失败: {}", e)).expect("static message"),
            }
        }
        None => {
            // 模拟路径不执行任何驱动调用：租约随函数返回释放。
            drop(lease);
            tracing::warn!("DataServer not available, action execution is simulated");
            tool_ok(json!({
                "thingId": thing_id,
                "actionName": action_name,
                "status": "simulated",
                "message": "操作已记录（DataServer 未就绪，实际执行已模拟）"
            }))
            .expect("payload serializes")
        }
    }
}

/// 下发尾巴（等待执行完成）：供 composite 顺序步骤使用。
///
/// composite 的 run-wide 租约已由调用方全程持有（`lease` 通常为 `None`），
/// 本函数等待 worker 回报执行终态——保证上一步命令真正离开驱动后才进入
/// 下一步，且 run-wide claim 的覆盖范围包含 worker 执行窗口。
pub(crate) async fn dispatch_command_with_lease_and_wait(
    data_server: Option<&Arc<tinyiothub_runtime::DataServer>>,
    thing_id: &str,
    action_name: &str,
    params: Option<&Value>,
    lease: Option<tinyiothub_runtime::arbitration::ResourceLease>,
) -> ToolResult {
    let Some(data_server) = data_server.cloned() else {
        // 模拟路径不执行任何驱动调用：租约随函数返回释放。
        drop(lease);
        tracing::warn!("DataServer not available, action execution is simulated");
        return tool_ok(json!({
            "thingId": thing_id,
            "actionName": action_name,
            "status": "simulated",
            "message": "操作已记录（DataServer 未就绪，实际执行已模拟）"
        }))
        .expect("payload serializes");
    };

    let cmd = tinyiothub_core::models::thing_command::ThingCommand {
        id: uuid::Uuid::new_v4().to_string(),
        thing_id: thing_id.to_string(),
        name: action_name.to_string(),
        display_name: None,
        description: None,
        parameters: params.map(|p| p.to_string()),
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };
    match data_server.execute_command_with_lease_and_wait(cmd, lease).await {
        Ok(outcome) if outcome.cancelled => tool_err(format!("操作已取消: {action_name}")).expect("static message"),
        Ok(outcome) if !outcome.success => tool_err(format!(
            "操作执行失败: {}",
            outcome.error.unwrap_or_else(|| "driver reported failure".to_string())
        ))
        .expect("static message"),
        Ok(_) => tool_ok(json!({
            "thingId": thing_id,
            "actionName": action_name,
            "status": "executed",
            "message": "操作已执行完成"
        }))
        .expect("payload serializes"),
        Err(e) => tool_err(format!("操作执行失败: {}", e)).expect("static message"),
    }
}

/// Swappable per-run context holder (O8: one autonomous agent per workspace,
/// the run context is replaced every run). The factory swaps the inner Arc on
/// each `get_or_create`; tools clone it out per execute.
pub type RunContextSlot = Arc<RwLock<Option<Arc<RwLock<RunContextInner>>>>>;

pub fn new_run_context_slot(ctx: Arc<RwLock<RunContextInner>>) -> RunContextSlot {
    Arc::new(RwLock::new(Some(ctx)))
}

/// Thin autonomous wrapper around [`InvokeActionTool`].
pub struct AutonomousInvokeActionTool {
    inner: InvokeActionTool,
    policy_repo: Arc<Db>,
    run_ctx: RunContextSlot,
    pool: SqlitePool,
    workspace_id: String,
    event_bus: Arc<ThingEventBus>,
    throttle: Arc<ThrottleState>,
    /// Provider tool-call id of the call about to execute (filled by the agent
    /// loop's pre-execution hook). Required for durable approval identity.
    tool_call_ids: Option<tinyiothub_agent::port::tool::ToolCallIdSlot>,
    /// E2 Policy 接缝（None = 内置 gate_check）。
    policy_seam: Option<Arc<dyn PolicySeam>>,
}

impl AutonomousInvokeActionTool {
    pub fn new(
        inner: InvokeActionTool,
        policy_repo: Arc<Db>,
        run_ctx: RunContextSlot,
        pool: SqlitePool,
        workspace_id: String,
        event_bus: Arc<ThingEventBus>,
        throttle: Arc<ThrottleState>,
    ) -> Self {
        Self {
            inner,
            policy_repo,
            run_ctx,
            pool,
            workspace_id,
            event_bus,
            throttle,
            tool_call_ids: None,
            policy_seam: None,
        }
    }

    pub fn with_tool_call_ids(mut self, slot: Option<tinyiothub_agent::port::tool::ToolCallIdSlot>) -> Self {
        self.tool_call_ids = slot;
        self
    }

    /// E2：注入 Policy 接缝替换实现（只能收窄）。
    pub fn with_policy_seam(mut self, seam: Arc<dyn PolicySeam>) -> Self {
        self.policy_seam = Some(seam);
        self
    }

    async fn task_identity(&self) -> Option<(String, String)> {
        let slot = self.run_ctx.read().await.clone()?;
        let ctx = slot.read().await;
        Some((ctx.task_id.clone()?, ctx.run_id.clone()))
    }

    /// Durable identity for the approval Effect. The session comes from the
    /// Run context (`task_session_key`; a Task without one falls back to its
    /// id, then the workspace id). The tool-call id is the provider id threaded
    /// by the agent loop's pre-execution hook — never an async trace scan or a
    /// random id. Missing identity fails closed.
    async fn approval_identity(&self) -> Option<(String, String)> {
        let slot = self.run_ctx.read().await.clone()?;
        let ctx = slot.read().await;
        let session_id = ctx
            .task_session_key
            .clone()
            .or_else(|| ctx.task_id.clone())
            .unwrap_or_else(|| self.workspace_id.clone());
        let tool_call_id = self
            .tool_call_ids
            .as_ref()
            .and_then(|slot| slot.lock().unwrap_or_else(|error| error.into_inner()).clone())?;
        Some((session_id, tool_call_id))
    }

    fn approval_pending(decision_id: &str, task_id: &str) -> ToolResult {
        tool_ok(json!({
            "status": "approval_pending",
            "approvalId": decision_id,
            "taskId": task_id,
            "message": "操作已提交人工审批，批准后才会执行。"
        }))
        .expect("payload serializes")
    }

    /// A retried identical tool call returns the existing approval instead of
    /// a Conflict. Identity means the same Effect id and the same frozen
    /// request digest; a different request under the same id is not a retry.
    async fn existing_approval(&self, effect_id: &str, decision_id: &str, request_digest: &str) -> Option<String> {
        let intent = self
            .policy_repo
            .effect_intent(&self.workspace_id, effect_id)
            .await
            .ok()??;
        if intent.request_digest != request_digest {
            return None;
        }
        let decision = self
            .policy_repo
            .effect_decision(&self.workspace_id, effect_id)
            .await
            .ok()??;
        (decision.decision_id == decision_id).then_some(decision.decision_id)
    }

    /// Originate the single approval mechanism: one workspace transaction
    /// inserts the Prepared Effect and its requested `tool_approval` Decision.
    /// No platform `task_approvals` row is written.
    async fn request_task_approval(
        &self,
        inner_output: &str,
        input: &Input,
        task_id: String,
        run_id: String,
    ) -> anyhow::Result<ToolResult> {
        // The inner legacy token must not remain usable through the chat confirmation
        // endpoint. Task identity and frozen execution fields come from this validated
        // tool call, never from the in-memory PendingActionStore.
        if let Some(token) = serde_json::from_str::<Value>(inner_output)
            .ok()
            .and_then(|value| value.get("token").and_then(Value::as_str).map(str::to_string))
        {
            self.inner.pending_actions.remove(&token);
        }
        let task = self.policy_repo.harness_get_task(&self.workspace_id, &task_id).await;
        let Ok(Some(task)) = task else {
            return tool_err("Task approval unavailable; action NOT dispatched".to_string());
        };
        if task.workspace_id != self.workspace_id || task.current_run_id.as_deref() != Some(run_id.as_str()) {
            return tool_err("Task Run changed; action NOT dispatched".to_string());
        }
        let Some((session_id, tool_call_id)) = self.approval_identity().await else {
            return tool_err("Task approval unavailable; action NOT dispatched".to_string());
        };
        let now = chrono::Utc::now();
        let args_json = input.params.clone().unwrap_or_else(|| json!({}));
        // The public approval summary never carries raw tool arguments. Only
        // the subject/projection sees this redacted copy; the Effect keeps the
        // frozen raw request internally for dispatch.
        let redacted_args = tinyiothub_core::redaction::RedactionPolicy::default().redact(&args_json);
        let request_json = json!({
            "deviceId": input.thing_id.clone(),
            "argsJson": args_json.clone(),
        });
        let request_digest = format!("{:x}", sha2::Sha256::digest(request_json.to_string().as_bytes()));
        let effect_id = format!("approval-effect:{task_id}:{tool_call_id}");
        let decision_id = format!("approval-decision:{task_id}:{tool_call_id}");
        let transition_id = format!("approval-request:{effect_id}");
        if let Some(existing) = self.existing_approval(&effect_id, &decision_id, &request_digest).await {
            return Ok(Self::approval_pending(&existing, &task_id));
        }
        let precondition = TransitionPrecondition {
            effect_id: effect_id.clone(),
            expected_state: EffectState::Prepared,
            expected_version: 0,
        };
        let event_payload = json!({
            "effectId": effect_id,
            "state": EffectState::Prepared.as_str(),
            "result": Value::Null,
        });
        let event = HarnessEventInput {
            event_id: format!("{transition_id}:event"),
            workspace_id: self.workspace_id.clone(),
            session_id: session_id.clone(),
            execution_mode: ExecutionMode::Production,
            run_id: Some(run_id.clone()),
            task_id: Some(task_id.clone()),
            event_type: EffectState::Prepared.event_type().to_string(),
            occurred_at: now,
            cause_id: None,
            schema_v: 1,
            payload_digest: format!("{:x}", sha2::Sha256::digest(event_payload.to_string().as_bytes())),
            payload_json: event_payload,
        };
        let decision_event_payload = json!({"decisionId": decision_id, "kind": DecisionKind::ToolApproval.as_str()});
        let decision_event = HarnessEventInput {
            event_id: format!("{transition_id}:decision"),
            workspace_id: self.workspace_id.clone(),
            session_id: session_id.clone(),
            execution_mode: ExecutionMode::Production,
            run_id: Some(run_id.clone()),
            task_id: Some(task_id.clone()),
            event_type: "decision.requested".to_string(),
            occurred_at: now,
            cause_id: None,
            schema_v: 1,
            payload_digest: format!(
                "{:x}",
                sha2::Sha256::digest(decision_event_payload.to_string().as_bytes())
            ),
            payload_json: decision_event_payload,
        };
        let decision = Decision {
            decision_id: decision_id.clone(),
            kind: DecisionKind::ToolApproval,
            state: DecisionState::Requested,
            session_id: session_id.clone(),
            run_id: run_id.clone(),
            task_id: task_id.clone(),
            effect_id: Some(effect_id.clone()),
            // Approval summary only: device, tool, frozen args, reason. The
            // raw Effect request never reaches this row.
            subject: json!({
                "deviceId": input.thing_id.clone(),
                "tool": input.action_name.clone(),
                "argsJson": redacted_args,
                "reason": "workspace action confirmation required",
            }),
            options: json!(["approved", "denied"]),
            created_at: now,
            updated_at: now,
            expires_at: Some(now + chrono::Duration::from_std(CONFIRMATION_TTL)?),
            answer: None,
            answer_actor: None,
            answer_event_id: None,
            answer_evidence: None,
            consumed_at: None,
            consumed_by: None,
        };
        let transition = Transition {
            transition_id,
            precondition: precondition.clone(),
            effect: Some(EffectMutation {
                effect_id: effect_id.clone(),
                insert_if_missing: true,
                state: EffectState::Prepared,
                recovery_from: None,
                state_version: 1,
                external_ref: None,
                result: None,
                updated_at: now,
            }),
            create_effect: Some(CreateEffect {
                session_id,
                task_id: task_id.clone(),
                run_id,
                tool_call_id,
                tool_name: input.action_name.clone(),
                request_json,
                request_digest: request_digest.clone(),
                idempotency_key: effect_id.clone(),
            }),
            event: Some(event),
            additional_events: vec![decision_event],
            decision: Some(DecisionMutation {
                decision,
                expected_state: None,
                insert_if_missing: true,
            }),
            run_task: None,
            job: None,
            wake: None,
        };
        match self
            .policy_repo
            .apply_workspace_transition(&self.workspace_id, &precondition, &transition)
            .await
        {
            Ok(_) => {}
            Err(
                tinyiothub_storage::error::DbError::Conflict { .. }
                | tinyiothub_storage::error::DbError::CasConflict { .. },
            ) => {
                // A concurrent identical call may have committed first.
                if let Some(existing) = self.existing_approval(&effect_id, &decision_id, &request_digest).await {
                    return Ok(Self::approval_pending(&existing, &task_id));
                }
                tracing::warn!(task_id = %task.id, "Task approval creation conflicted");
                return Err(anyhow::anyhow!("Task approval creation failed; action NOT dispatched"));
            }
            Err(error) => {
                tracing::warn!(task_id = %task.id, %error, "Task approval creation failed");
                return Err(anyhow::anyhow!("Task approval creation failed; action NOT dispatched"));
            }
        }
        Ok(Self::approval_pending(&decision_id, &task_id))
    }

    /// Consume the confirmation token minted by the inner tool and dispatch
    /// directly (policy gate replaces human confirmation, O18). Returns None
    /// when the token vanished or mismatches (thing/action/workspace) —
    /// practically unreachable; the caller maps None to a tool error rather
    /// than leaking the confirmation_required payload (and its token) to the
    /// LLM.
    async fn auto_confirm(&self, inner_output: &str, input: &Input) -> Option<ToolResult> {
        let token = serde_json::from_str::<Value>(inner_output)
            .ok()?
            .get("token")?
            .as_str()?
            .to_string();
        let pending = take_pending_action(&self.inner.pending_actions, &token)?;
        if pending.thing_id != input.thing_id
            || pending.action_name != input.action_name
            || pending.workspace_id != self.workspace_id
        {
            return None;
        }
        Some(
            dispatch_command(
                self.inner.data_server.as_ref(),
                &self.workspace_id,
                &pending.thing_id,
                &pending.action_name,
                pending.params.as_ref(),
                self.inner.arbitration.as_deref(),
                self.inner.resources.as_deref(),
                &self.inner.arbitration_holder,
            )
            .await,
        )
    }

    /// Record the dispatched action in the events table with actor="agent"
    /// (T6 resonance-guard handoff). Routing never errors out the tool —
    /// the action already happened; a persist failure is logged only.
    async fn record_agent_event(&self, input: &Input, status: &str) {
        let event_input = ThingEventInput {
            thing_id: input.thing_id.clone(),
            workspace_id: self.workspace_id.clone(),
            event_name: input.action_name.clone(),
            level: EventLevel::Info,
            data: json!({
                "actionName": input.action_name.clone(),
                "params": input.params.clone(),
                "status": status,
                "source": "thing_agent",
            }),
            ts: None,
            template_events: None,
        };
        // No AlarmService: an agent action record is not a device condition.
        let routed = route_thing_event(&self.pool, &self.throttle, None, &self.event_bus, "agent", event_input).await;
        if routed.malformed {
            tracing::warn!(
                thing_id = %input.thing_id,
                action = %input.action_name,
                "agent action event persist failed"
            );
        }
    }
}

impl Attributable for AutonomousInvokeActionTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        <Self as Tool>::name(self)
    }
}

#[async_trait]
impl Tool for AutonomousInvokeActionTool {
    fn name(&self) -> &str {
        // E2：模型可见的固定设备动作名。runner 轨迹匹配同时接受旧名
        // `invoke_action`（隐藏别名，兼容存量报告）。
        "call_device"
    }

    fn description(&self) -> &str {
        "对物执行操作（自治模式）。仅 thingType='device' 的物支持此操作。\
         每次调用都会经过工作空间自治策略门（模式/黑白名单/频率上限）；\
         被拒绝时返回 {\"denied\": true, \"reason\": ...}，请尊重拒绝结果不要重试同一操作。\
         当你需要在自治循环中控制设备（如开关、重启、设置参数）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "action": {
                    "type": "string",
                    "description": "操作名称（必需）"
                },
                "params": {
                    "type": "object",
                    "description": "操作参数（可选，JSON 键值对）"
                }
            },
            "required": ["thingId", "action"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let input: Input = serde_json::from_value(args.clone()).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        let denied = |reason: &str| {
            // O14：所有拒绝统一在此打点（策略门 / 保险丝 / 读失败）。
            tracing::info!(
                metric = "agent_action_denied",
                workspace_id = %self.workspace_id,
                thing = %input.thing_id,
                action = %input.action_name,
                reason = %reason,
                "autonomous action denied"
            );
            tool_ok(json!({
                "denied": true,
                "reason": reason,
                "thingId": input.thing_id.clone(),
                "actionName": input.action_name.clone(),
            }))
        };

        // 1. Policy gate — fresh read on EVERY call (O7: kill switch takes
        // effect immediately). Any read failure is fail-closed.
        let policy = match self.policy_repo.load_autonomy_policy(&self.workspace_id).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    workspace_id = %self.workspace_id,
                    "autonomy policy read failed — fail closed"
                );
                return denied("policy_read_failed");
            }
        };
        let hourly = match self
            .policy_repo
            .count_autonomy_actions_last_hour(&self.workspace_id)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    workspace_id = %self.workspace_id,
                    "hourly action count read failed — fail closed"
                );
                return denied("policy_read_failed");
            }
        };

        // 2. Prior dispatches for this thing in this run. The T9 runner
        // counts the ToolCall event at dispatch time — BEFORE zeroclaw
        // invokes this tool (the consume task runs at this tool's first
        // await point) — so the observed count includes the in-flight call;
        // subtract it. Denied attempts consume budget too (anti retry-loop
        // fuse): the runner counts every dispatch, allowed or not.
        let prior_actions = match self.run_ctx.read().await.as_ref() {
            Some(ctx) => ctx
                .read()
                .await
                .action_counts
                .get(&input.thing_id)
                .copied()
                .unwrap_or(0)
                .saturating_sub(1),
            None => 0,
        };

        let seam: Arc<dyn PolicySeam> = self.policy_seam.clone().unwrap_or_else(|| Arc::new(DefaultPolicy));
        let policy_ref: Option<&AutonomyPolicy> = policy.as_ref();
        if let PolicyDecision::Deny { code, .. } = seam.evaluate(&PolicyInput {
            workspace_id: &self.workspace_id,
            action_name: &input.action_name,
            actions_this_run: prior_actions,
            actions_last_hour: hourly,
            policy: policy_ref,
        }) {
            return denied(&code);
        }

        // 3. Validation + dispatch via the inner tool (thing.rs reuse).
        let result = self.inner.execute(args).await?;
        if !result.success {
            return Ok(result);
        }

        // 4. The policy gate replaces the human confirmation branch (O18):
        // when the workspace still has require_action_confirm ON, the inner
        // tool mints a token instead of dispatching — auto-confirm it (the
        // gate already authorized this action).
        let status = serde_json::from_str::<Value>(&result.output)
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_string))
            .unwrap_or_default();
        let final_result = if status == "confirmation_required" {
            if let Some((task_id, run_id)) = self.task_identity().await {
                self.request_task_approval(&result.output, &input, task_id, run_id)
                    .await?
            } else {
                // Non-Task autonomous/heartbeat behavior retains policy auto-confirm.
                // Never pass the confirmation_required payload through on
                // auto-confirm failure: it carries a live token to the LLM.
                match self.auto_confirm(&result.output, &input).await {
                    Some(r) => r,
                    None => {
                        tool_err("auto-confirm failed: token mismatch or expired; action NOT dispatched".to_string())
                            .expect("static message")
                    }
                }
            }
        } else {
            result
        };

        // 5. T6 hard handoff: record the dispatched action with
        // actor="agent" (resonance guard — the thing-event trigger must not
        // wake the loop on its own output).
        let final_status = serde_json::from_str::<Value>(&final_result.output)
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_string))
            .unwrap_or_default();
        if final_result.success && (final_status == "executed" || final_status == "simulated") {
            // O14：放行并真实下发的动作打点（与 deny 侧配平）。
            tracing::info!(
                metric = "agent_action_allowed",
                workspace_id = %self.workspace_id,
                thing = %input.thing_id,
                action = %input.action_name,
                status = %final_status,
                "autonomous action dispatched"
            );
            self.record_agent_event(&input, &final_status).await;
        }
        Ok(final_result)
    }
}

#[cfg(test)]
fn self_pending_actions() -> Arc<super::thing::PendingActionStore> {
    Arc::new(super::thing::PendingActionStore::new())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::domains::agent::thing_agent::RunContextInner;
    use crate::domains::thing::service::ThingService;
    use tinyiothub_agent::port::tool::Tool;
    use tinyiothub_policy::autonomy::{AutonomyPolicy, PermissionMode};

    use super::*;
    use crate::domains::agent::host::test_utils::seed_test_workspace;
    use tinyiothub_storage::Db;

    // ── helpers ────────────────────────────────────────────────

    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    async fn seed_thing(pool: &SqlitePool, workspace_id: &str, thing_id: &str, thing_type: &str) {
        seed_test_workspace(pool, "tenant-1", workspace_id).await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, ?, ?)")
            .bind(thing_id)
            .bind(format!("Thing {thing_id}"))
            .bind(workspace_id)
            .bind(thing_type)
            .execute(pool)
            .await
            .expect("insert device");
    }

    async fn register_action(pool: &SqlitePool, thing_id: &str, action_name: &str) {
        sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES (?, ?, ?)")
            .bind(format!("act-{action_name}-{thing_id}"))
            .bind(thing_id)
            .bind(action_name)
            .execute(pool)
            .await
            .expect("register action");
    }

    fn act_policy() -> AutonomyPolicy {
        AutonomyPolicy {
            permission_mode: PermissionMode::Ask,
            allowed_actions: vec!["*".to_string()],
            denied_actions: vec!["wipe_thing".to_string()],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }

    struct Fixture {
        pool: SqlitePool,
        tool: AutonomousInvokeActionTool,
        ctx: Arc<RwLock<RunContextInner>>,
        bus: Arc<ThingEventBus>,
        policy_repo: Arc<tinyiothub_storage::Db>,
    }

    async fn fixture(workspace_id: &str) -> Fixture {
        let pool = test_pool().await;
        let policy_repo = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let bus = Arc::new(ThingEventBus::new());
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: workspace_id.to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
            arbitration: None,
            arbitration_holder: "test".to_string(),
            approval: None,
            access: None,
            resources: None,
            jobs: None,
            run_ctx: None,
        };
        let tool = AutonomousInvokeActionTool::new(
            inner,
            policy_repo.clone(),
            new_run_context_slot(Arc::clone(&ctx)),
            pool.clone(),
            workspace_id.to_string(),
            bus.clone(),
            Arc::new(ThrottleState::new(60)),
        );
        Fixture {
            pool,
            tool,
            ctx,
            bus,
            policy_repo,
        }
    }

    fn args(thing_id: &str, action: &str) -> Value {
        json!({"thingId": thing_id, "actionName": action, "params": {"speed": 1}})
    }

    fn output_json(result: &ToolResult) -> Value {
        serde_json::from_str(&result.output).expect("tool output must be JSON")
    }

    /// Mirror the production dispatch order: the T9 runner counts the
    /// ToolCall event BEFORE zeroclaw invokes `Tool::execute`.
    async fn dispatch(fx: &Fixture, thing_id: &str, action: &str) -> ToolResult {
        *fx.ctx
            .write()
            .await
            .action_counts
            .entry(thing_id.to_string())
            .or_insert(0) += 1;
        fx.tool.execute(args(thing_id, action)).await.expect("execute")
    }

    async fn agent_event_count(pool: &SqlitePool, thing_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE thing_id = ? AND actor = 'agent'")
            .bind(thing_id)
            .fetch_one(pool)
            .await
            .expect("count agent events")
    }

    // ── deny cases (structured, LLM-readable) ──────────────────

    #[tokio::test]
    async fn deny_when_read_only_returns_structured_denied() {
        let fx = fixture("ws-ro").await;
        seed_thing(&fx.pool, "ws-ro", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        let mut policy = act_policy();
        policy.permission_mode = PermissionMode::ReadOnly;
        fx.policy_repo
            .save_autonomy_policy("ws-ro", &policy, "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "dev-1", "reboot").await;
        assert!(
            result.success,
            "deny must be a readable result, not an error: {result:?}"
        );
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "action_not_allowed");
        assert_eq!(
            agent_event_count(&fx.pool, "dev-1").await,
            0,
            "denied action writes no event"
        );
    }

    #[tokio::test]
    async fn deny_when_no_policy_row_fails_closed() {
        let fx = fixture("ws-none").await;
        seed_thing(&fx.pool, "ws-none", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;

        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "policy_not_configured");
    }

    /// E2 替换证明：Policy 接缝替换实现走真实工具路径生效（只能收窄）。
    struct DenyAllSeam;

    impl PolicySeam for DenyAllSeam {
        fn evaluate(&self, _input: &PolicyInput<'_>) -> PolicyDecision {
            PolicyDecision::Deny {
                code: "seam_deny".to_string(),
                message: "replacement seam denies everything".to_string(),
            }
        }
    }

    #[tokio::test]
    async fn replacement_policy_seam_is_consulted() {
        let fx = fixture("ws-seam").await;
        seed_thing(&fx.pool, "ws-seam", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy_policy("ws-seam", &act_policy(), "test")
            .await
            .unwrap();

        let tool = fx.tool.with_policy_seam(Arc::new(DenyAllSeam));
        *fx.ctx
            .write()
            .await
            .action_counts
            .entry("dev-1".to_string())
            .or_insert(0) += 1;
        let result = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "seam_deny");
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 0);
    }

    #[tokio::test]
    async fn deny_when_action_blacklisted() {
        let fx = fixture("ws-bl").await;
        seed_thing(&fx.pool, "ws-bl", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "wipe_thing").await;
        fx.policy_repo
            .save_autonomy_policy("ws-bl", &act_policy(), "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "dev-1", "wipe_thing").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "action_denied");
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 0);
    }

    #[tokio::test]
    async fn deny_when_action_not_in_allowlist() {
        let fx = fixture("ws-al").await;
        seed_thing(&fx.pool, "ws-al", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        let mut policy = act_policy();
        policy.allowed_actions = vec!["set_fan".to_string()];
        fx.policy_repo
            .save_autonomy_policy("ws-al", &policy, "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "action_not_allowed");
    }

    #[tokio::test]
    async fn deny_when_run_cap_reached_after_exactly_max_dispatches() {
        let fx = fixture("ws-cap").await;
        seed_thing(&fx.pool, "ws-cap", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy_policy("ws-cap", &act_policy(), "test")
            .await
            .unwrap();

        // max_actions_per_run = 3: the first three dispatches pass, the
        // fourth is denied — the hard cap is enforced inside the tool (O9).
        for i in 1..=3 {
            let result = dispatch(&fx, "dev-1", "reboot").await;
            let out = output_json(&result);
            assert!(out.get("denied").is_none(), "dispatch {i} must be allowed: {out}");
        }
        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "run_action_cap");
        assert_eq!(
            agent_event_count(&fx.pool, "dev-1").await,
            3,
            "only allowed actions write events"
        );
    }

    #[tokio::test]
    async fn deny_when_hourly_fuse_reached() {
        let fx = fixture("ws-fuse").await;
        seed_thing(&fx.pool, "ws-fuse", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        let mut policy = act_policy();
        policy.max_actions_per_hour = 2;
        fx.policy_repo
            .save_autonomy_policy("ws-fuse", &policy, "test")
            .await
            .unwrap();
        // Two earlier runs already spent the hourly budget.
        for i in 0..2 {
            sqlx::query(
                "INSERT INTO agent_runs (id, workspace_id, trigger_type, outcome, report, created_at)
                 VALUES (?, 'ws-fuse', 'timer', 'success', json_object('action_count', 1), datetime('now', ?))",
            )
            .bind(format!("run-{i}"))
            .bind(format!("-{i} minutes"))
            .execute(&fx.pool)
            .await
            .expect("insert agent_run");
        }

        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "hourly_fuse");
    }

    #[tokio::test]
    async fn deny_when_policy_read_fails() {
        // 无 trait 后故障注入方式变更（E3）：给 Db 门面一个缺
        // workspace_autonomy_policy 表的空库，load_autonomy 必然 DbError，
        // 与原 FailingRepo 的 "db down" 等效（fail-closed 路径一致）。
        let failing_repo = Db::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect(":memory:")
                .await
                .expect("empty pool"),
        );

        let pool = test_pool().await;
        seed_thing(&pool, "ws-err", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-err".to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
            arbitration: None,
            arbitration_holder: "test".to_string(),
            approval: None,
            access: None,
            resources: None,
            jobs: None,
            run_ctx: None,
        };
        let tool = AutonomousInvokeActionTool::new(
            inner,
            Arc::new(failing_repo),
            new_run_context_slot(Arc::clone(&ctx)),
            pool.clone(),
            "ws-err".to_string(),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
        );

        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;
        let result = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(result.success, "policy read failure must not be a tool error");
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "policy_read_failed");
    }

    #[tokio::test]
    async fn autonomous_invoke_policy_gate_reads_fresh_per_call() {
        // O7 锁定(本文件 execute 第 1 步,autonomous_invoke.rs:278-280:
        // 「Policy gate — fresh read on EVERY call (O7: kill switch takes
        // effect immediately)」)+ R1(spec §2 读门时机):两次调用之间只翻
        // 持久层的 policy 行,不重建 tool、不走任何内存缓存——下一次调用
        // 必须立即按新档位裁决。防有人把 load_autonomy_policy 「优化」成
        // run 触发时快照。
        let fx = fixture("ws-o7").await;
        seed_thing(&fx.pool, "ws-o7", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy_policy("ws-o7", &act_policy(), "test")
            .await
            .unwrap();

        // Ask 档:放行。
        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert!(out.get("denied").is_none(), "Ask mode must allow: {out}");

        // kill switch:同一 DB 行翻成 ReadOnly —— 下一次调用立即拒。
        let mut readonly = act_policy();
        readonly.permission_mode = PermissionMode::ReadOnly;
        fx.policy_repo
            .save_autonomy_policy("ws-o7", &readonly, "test")
            .await
            .unwrap();
        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "action_not_allowed");

        // 翻回 Ask:立即恢复放行(双向都锁,无快照窗口)。
        fx.policy_repo
            .save_autonomy_policy("ws-o7", &act_policy(), "test")
            .await
            .unwrap();
        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert!(
            out.get("denied").is_none(),
            "flipping back to Act must allow immediately: {out}"
        );
        assert_eq!(
            agent_event_count(&fx.pool, "dev-1").await,
            2,
            "only the two allowed dispatches write events"
        );
    }

    // ── allow path ─────────────────────────────────────────────

    #[tokio::test]
    async fn allow_dispatches_and_records_agent_actor_event() {
        let fx = fixture("ws-ok").await;
        seed_thing(&fx.pool, "ws-ok", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy_policy("ws-ok", &act_policy(), "test")
            .await
            .unwrap();
        let mut rx = fx.bus.subscribe();

        let result = dispatch(&fx, "dev-1", "reboot").await;
        assert!(result.success, "allow path must succeed: {:?}", result.error);
        let out = output_json(&result);
        assert!(out.get("denied").is_none(), "allowed call must not carry denied: {out}");
        // No DataServer in tests → the dispatch branch reports "simulated"
        // (validation + command construction passed; queue push skipped).
        assert!(
            out["status"] == "simulated" || out["status"] == "executed",
            "expected dispatch status, got: {out}"
        );

        // T6 hard handoff: the action's event is marked actor="agent".
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 1);
        let (actor, subtype): (String, String) =
            sqlx::query_as("SELECT actor, event_subtype FROM events WHERE thing_id = 'dev-1' AND actor = 'agent'")
                .fetch_one(&fx.pool)
                .await
                .expect("agent event row");
        assert_eq!(actor, "agent");
        assert_eq!(subtype, "reboot");

        let signal = rx.recv().await.expect("bus signal for the agent action");
        assert_eq!(signal.actor, "agent");
        assert_eq!(signal.thing_id, "dev-1");
        assert_eq!(signal.event_name, "reboot");
    }

    #[tokio::test]
    async fn task_tool_call_creates_one_requested_decision_before_any_dispatch() {
        let dir = tempfile::tempdir().expect("Task approval test directory");
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(dir.path().join("task-approval.db"))
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .expect("Task approval pool");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("Task approval migrations");
        let db = Arc::new(Db::new(pool.clone()).with_workspace_store_root(dir.path().join("data/agents")));
        seed_thing(&pool, "ws-task-approval", "dev-task", "device").await;
        register_action(&pool, "dev-task", "reboot").await;
        db.save_autonomy_policy("ws-task-approval", &act_policy(), "test")
            .await
            .expect("policy");
        db.create_workspace_store("ws-task-approval")
            .await
            .expect("workspace store");
        let now = chrono::Utc::now();
        let task = tinyiothub_core::agent_task::AgentTask {
            id: "task-tool-approval".into(),
            workspace_id: "ws-task-approval".into(),
            idempotency_key: "task-tool-approval-key".into(),
            objective: "approval boundary test".into(),
            session_key: None,
            created_by: "user-1".into(),
            status: tinyiothub_core::agent_task::TaskStatus::Active,
            current_run_id: Some("run-tool-approval".into()),
            state_version: 0,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let wake = tinyiothub_core::agent_task::TaskWake {
            task_id: task.id.clone(),
            event_id: "task-tool-created".into(),
            signal_json: json!({"workspace_id":task.workspace_id}).to_string(),
            dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Dispatched,
            created_at: now,
            dispatched_at: Some(now),
            resolved_at: Some(now),
        };
        db.harness_create_task_with_wake(&task, &wake)
            .await
            .expect("harness Task");
        let ctx = Arc::new(RwLock::new(RunContextInner {
            run_id: "run-tool-approval".into(),
            workspace_id: "ws-task-approval".into(),
            task_id: Some(task.id.clone()),
            task_session_key: Some("session-tool-approval".into()),
            ..Default::default()
        }));
        ctx.write().await.action_counts.insert("dev-task".into(), 1);
        let pending_actions = self_pending_actions();
        let tool_call_ids: tinyiothub_agent::port::tool::ToolCallIdSlot =
            Arc::new(std::sync::Mutex::new(Some("call-approval-1".into())));
        let tools = crate::domains::agent::host::autonomous_factory::build_autonomous_tools(
            &pool,
            "ws-task-approval",
            db.clone(),
            new_run_context_slot(ctx),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
            &super::super::ThingToolContext {
                pending_actions: Some(pending_actions.clone()),
                ..Default::default()
            },
            Some(tool_call_ids),
        );
        let call_device = tools
            .iter()
            .find(|tool| tool.name() == "call_device")
            .expect("call_device");
        let result = call_device
            .execute(json!({
                "thingId":"dev-task",
                "action":"reboot",
                "params":{"speed":1,"credentials":{"password":"s3cret"}}
            }))
            .await
            .expect("tool call");
        let output = output_json(&result);
        assert_eq!(output["status"], "approval_pending", "{}", result.output);
        let approval_id = output["approvalId"].as_str().expect("approvalId");
        let workspace = db
            .workspace_pool_for_testing("ws-task-approval")
            .await
            .expect("workspace pool");
        let decisions: Vec<(String, String, String, Option<String>, String)> =
            sqlx::query_as("SELECT decision_id, kind, state, effect_id, subject_json FROM decisions")
                .fetch_all(&workspace)
                .await
                .expect("decisions");
        assert_eq!(
            decisions.len(),
            1,
            "the tool call must originate exactly one Decision: {}",
            result.output
        );
        let (decision_id, kind, state, effect_id, subject) = &decisions[0];
        assert_eq!(decision_id, approval_id);
        assert_eq!(kind, "tool_approval");
        assert_eq!(state, "requested");
        assert!(effect_id.is_some());
        let subject: Value = serde_json::from_str(subject).expect("subject JSON");
        assert_eq!(subject["deviceId"], "dev-task");
        assert_eq!(subject["tool"], "reboot");
        assert_eq!(subject["argsJson"]["speed"], 1);
        assert_eq!(subject["argsJson"]["credentials"]["password"], "[REDACTED]");
        assert!(
            !subject.to_string().contains("s3cret"),
            "the decision subject must not carry a nested secret: {subject}"
        );
        let raw_request: String = sqlx::query_scalar("SELECT request_json FROM effect_intents WHERE effect_id = ?")
            .bind(effect_id.as_ref().expect("effect id"))
            .fetch_one(&workspace)
            .await
            .expect("raw effect request");
        assert!(
            raw_request.contains("s3cret"),
            "the internal Effect request keeps the raw args for dispatch"
        );
        let requested: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM effect_intents WHERE effect_id = ? AND state = 'prepared' AND state_version = 1",
        )
        .bind(effect_id.as_ref().expect("effect id"))
        .fetch_one(&workspace)
        .await
        .expect("prepared effect");
        assert_eq!(requested, 1, "one Prepared Effect in the same transition");
        let (effect_session, effect_call): (String, String) =
            sqlx::query_as("SELECT session_id, tool_call_id FROM effect_intents WHERE effect_id = ?")
                .bind(effect_id.as_ref().expect("effect id"))
                .fetch_one(&workspace)
                .await
                .expect("effect identity");
        assert_eq!(effect_session, "session-tool-approval");
        assert_eq!(effect_call, "call-approval-1");
        let decision_session: String = sqlx::query_scalar("SELECT session_id FROM decisions WHERE decision_id = ?")
            .bind(decision_id)
            .fetch_one(&workspace)
            .await
            .expect("decision session");
        assert_eq!(decision_session, "session-tool-approval");
        let platform: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_approvals")
            .fetch_one(&pool)
            .await
            .expect("platform approvals");
        assert_eq!(platform, 0, "the platform approval table has no writers");
        let task = db
            .harness_get_task("ws-task-approval", &task.id)
            .await
            .expect("Task")
            .expect("Task row");
        assert!(
            db.harness_has_requested_tool_approval("ws-task-approval", &task.id)
                .await
                .expect("pending decision"),
            "waiting_status sees the requested Decision"
        );
        assert!(result.output.find("token").is_none());
        assert!(
            pending_actions.is_empty(),
            "legacy confirmation token must be invalidated"
        );
        assert_eq!(agent_event_count(&pool, "dev-task").await, 0);
    }

    #[tokio::test]
    async fn allow_auto_confirms_when_workspace_confirm_gate_on() {
        // require_action_confirm defaults ON (fail-closed, see thing.rs).
        // The autonomous variant replaces the human confirmation branch with
        // the policy gate: an allowed action must dispatch, not mint a token.
        let fx = fixture("ws-conf").await;
        seed_thing(&fx.pool, "ws-conf", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy_policy("ws-conf", &act_policy(), "test")
            .await
            .unwrap();
        let require: i64 = sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = 'ws-conf'")
            .fetch_one(&fx.pool)
            .await
            .unwrap();
        assert_eq!(require, 1, "test precondition: confirm gate defaults ON");

        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert!(
            out["status"] == "simulated" || out["status"] == "executed",
            "autonomous variant must auto-confirm, got: {out}"
        );
        assert!(out.get("token").is_none(), "no human token in autonomous mode: {out}");
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 1);
    }

    // ── auto-confirm failure paths ─────────────────────────────

    /// Tool whose OUTER workspace (policy gate + auto-confirm workspace
    /// check) differs from the INNER tool's workspace (token minter). The
    /// inner tool binds confirmation tokens to its own workspace, so the
    /// outer auto-confirm must reject them.
    async fn cross_workspace_fixture(
        outer_ws: &str,
        inner_ws: &str,
    ) -> (SqlitePool, AutonomousInvokeActionTool, Arc<RwLock<RunContextInner>>) {
        let pool = test_pool().await;
        seed_test_workspace(&pool, "tenant-1", outer_ws).await;
        let policy_repo = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        policy_repo
            .save_autonomy_policy(outer_ws, &act_policy(), "test")
            .await
            .unwrap();
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: inner_ws.to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
            arbitration: None,
            arbitration_holder: "test".to_string(),
            approval: None,
            access: None,
            resources: None,
            jobs: None,
            run_ctx: None,
        };
        let tool = AutonomousInvokeActionTool::new(
            inner,
            policy_repo,
            new_run_context_slot(Arc::clone(&ctx)),
            pool.clone(),
            outer_ws.to_string(),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
        );
        (pool, tool, ctx)
    }

    #[tokio::test]
    async fn workspace_mismatch_blocks_auto_confirm() {
        // The inner tool mints a token bound to ws-inner-mm; the outer tool's
        // workspace is ws-outer-mm — the pending action must NOT be confirmed.
        let (pool, tool, ctx) = cross_workspace_fixture("ws-outer-mm", "ws-inner-mm").await;
        seed_thing(&pool, "ws-inner-mm", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;

        let result = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(
            !result.success,
            "cross-workspace token must not auto-confirm: {result:?}"
        );
        assert!(result.error.as_deref().unwrap_or("").contains("auto-confirm"));
        assert_eq!(agent_event_count(&pool, "dev-1").await, 0, "no dispatch, no event");
    }

    #[tokio::test]
    async fn auto_confirm_failure_does_not_leak_token() {
        // On auto-confirm mismatch the tool must return a tool_err, never the
        // inner confirmation_required payload (which carries a live token).
        let (pool, tool, ctx) = cross_workspace_fixture("ws-outer-lk", "ws-inner-lk").await;
        seed_thing(&pool, "ws-inner-lk", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;

        let result = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(!result.success);
        assert!(
            !result.output.contains("token") && !result.output.contains("confirmation_required"),
            "failure must not echo the confirmation payload to the LLM: {}",
            result.output
        );

        // Unit-level: an unknown/vanished token also yields None (mapped to
        // the same tool_err above), never a passthrough.
        let input = Input {
            thing_id: "dev-1".into(),
            action_name: "reboot".into(),
            params: None,
        };
        let fake = json!({"status": "confirmation_required", "token": "no-such-token"}).to_string();
        assert!(tool.auto_confirm(&fake, &input).await.is_none());
    }

    // ── inner behavior reuse ───────────────────────────────────

    #[tokio::test]
    async fn non_device_thing_returns_inner_error_without_event() {
        let fx = fixture("ws-space").await;
        seed_thing(&fx.pool, "ws-space", "space-1", "space").await;
        fx.policy_repo
            .save_autonomy_policy("ws-space", &act_policy(), "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "space-1", "reboot").await;
        assert!(!result.success, "non-device thing must be rejected by inner validation");
        assert!(result.error.as_deref().unwrap_or("").contains("操作不支持"));
        assert_eq!(agent_event_count(&fx.pool, "space-1").await, 0);
    }

    #[tokio::test]
    async fn unregistered_action_returns_inner_error_without_event() {
        let fx = fixture("ws-unreg").await;
        seed_thing(&fx.pool, "ws-unreg", "dev-1", "device").await;
        fx.policy_repo
            .save_autonomy_policy("ws-unreg", &act_policy(), "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "dev-1", "not_registered").await;
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("未在物"));
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 0);
    }

    /// E1 仲裁（Task 9 fix）：开确认门的 workspace 里，auto-confirm 的
    /// 真实下发同样抢设备互斥锁——他人持锁时 busy 拒绝（带持有者名），
    /// 释放后放行。
    #[tokio::test]
    async fn auto_confirm_dispatch_respects_arbitration_lock() {
        let pool = test_pool().await;
        seed_thing(&pool, "ws-arb-ac", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        let policy_repo = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        policy_repo
            .save_autonomy_policy("ws-arb-ac", &act_policy(), "test")
            .await
            .unwrap();
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let registry = Arc::new(ArbitrationRegistry::default());
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-arb-ac".to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
            arbitration: Some(registry.clone()),
            arbitration_holder: "autonomous:ws-arb-ac".to_string(),
            approval: None,
            access: None,
            resources: None,
            jobs: None,
            run_ctx: None,
        };
        let tool = AutonomousInvokeActionTool::new(
            inner,
            policy_repo,
            new_run_context_slot(Arc::clone(&ctx)),
            pool.clone(),
            "ws-arb-ac".to_string(),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
        );
        // 测试前置：确认门默认 ON（走 auto-confirm 路径，评审 Issue 1 的漏洞路径）。
        let require: i64 = sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = 'ws-arb-ac'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(require, 1, "test precondition: confirm gate defaults ON");

        // 另一执行体持锁 → auto-confirm 下发被 busy 拒绝（带持有者名）。
        let lease = registry
            .try_acquire_default("dev-1", "watch-run-A".to_string())
            .expect("run-A acquire");
        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;
        let busy = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(!busy.success, "持锁期间必须 busy: {busy:?}");
        let msg = busy.error.unwrap_or_default();
        assert!(msg.contains("watch-run-A"), "busy 必须带持有者名: {msg}");

        // 释放后放行（无 DataServer → simulated）。
        drop(lease);
        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;
        let ok = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(ok.success, "释放后必须放行: {:?}", ok.error);
        let out = output_json(&ok);
        assert!(out["status"] == "simulated" || out["status"] == "executed", "{out}");
    }

    /// Issue 3：确认门路径（auto-confirm）也按工具声明 claim 资源，而不是
    /// 只拿合成 `__device__`。声明资源被占时 busy 点名 holder + resource。
    #[tokio::test]
    async fn auto_confirm_claims_declared_resources() {
        struct ActuatorResolver;

        #[async_trait::async_trait]
        impl ResourceResolver for ActuatorResolver {
            async fn resolve_action(
                &self,
                _ws: &str,
                _thing_id: &str,
                _tool_name: &str,
            ) -> Result<Vec<tinyiothub_runtime::access::resources::ResourceClaim>, String> {
                Ok(vec![tinyiothub_runtime::access::resources::ResourceClaim {
                    id: "actuator".into(),
                    exclusive: true,
                    capacity: 1,
                }])
            }

            async fn resolve_properties(
                &self,
                _ws: &str,
                _thing_id: &str,
                _properties: &[String],
            ) -> Result<Vec<tinyiothub_runtime::access::resources::ResourceClaim>, String> {
                Ok(vec![])
            }
        }

        let pool = test_pool().await;
        seed_thing(&pool, "ws-res-ac", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        let policy_repo = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        policy_repo
            .save_autonomy_policy("ws-res-ac", &act_policy(), "test")
            .await
            .unwrap();
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let registry = Arc::new(ArbitrationRegistry::default());
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-res-ac".to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
            arbitration: Some(registry.clone()),
            arbitration_holder: "autonomous:ws-res-ac".to_string(),
            approval: None,
            access: None,
            resources: Some(Arc::new(ActuatorResolver)),
            jobs: None,
            run_ctx: None,
        };
        let tool = AutonomousInvokeActionTool::new(
            inner,
            policy_repo,
            new_run_context_slot(Arc::clone(&ctx)),
            pool.clone(),
            "ws-res-ac".to_string(),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
        );

        // 确认门 ON（走 auto-confirm 路径）。
        let require: i64 = sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = 'ws-res-ac'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(require, 1, "test precondition: confirm gate defaults ON");

        // 另一执行体占住声明的 actuator → auto-confirm 下发 busy。
        let held = registry
            .try_acquire_resources(
                "dev-1",
                "watch-run-A".into(),
                &[crate::domains::agent::thing_agent::ResourceSpec {
                    id: "actuator".into(),
                    mode: crate::domains::agent::thing_agent::ResourceMode::Exclusive,
                    capacity: 1,
                }],
                DEFAULT_LEASE_TTL,
            )
            .expect("run-A 占 actuator");
        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;
        let busy = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(!busy.success, "声明资源被占必须 busy: {busy:?}");
        assert_eq!(busy.error.as_deref(), Some("busy: watch-run-A holds actuator"));

        // 释放后放行（无 DataServer → simulated）。
        drop(held);
        *ctx.write().await.action_counts.entry("dev-1".to_string()).or_insert(0) += 1;
        let ok = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(ok.success, "释放后必须放行: {:?}", ok.error);
    }

    #[tokio::test]
    async fn missing_run_context_treated_as_zero_prior_actions() {
        // Defensive: the factory always binds a context, but an unbound slot
        // must not panic — gate decides on policy + hourly fuse alone.
        let pool = test_pool().await;
        seed_thing(&pool, "ws-norun", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        let policy_repo = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        policy_repo
            .save_autonomy_policy("ws-norun", &act_policy(), "test")
            .await
            .unwrap();
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-norun".to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
            arbitration: None,
            arbitration_holder: "test".to_string(),
            approval: None,
            access: None,
            resources: None,
            jobs: None,
            run_ctx: None,
        };
        let tool = AutonomousInvokeActionTool::new(
            inner,
            policy_repo,
            Arc::new(RwLock::new(None)),
            pool,
            "ws-norun".to_string(),
            Arc::new(ThingEventBus::new()),
            Arc::new(ThrottleState::new(60)),
        );
        let result = tool.execute(args("dev-1", "reboot")).await.expect("execute");
        assert!(result.success);
    }
}

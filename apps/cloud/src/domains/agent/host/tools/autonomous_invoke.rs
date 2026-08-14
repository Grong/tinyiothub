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

use crate::domains::agent::loop_::thing_agent::RunContextInner;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_core::models::event::EventLevel;
use tinyiothub_policy::autonomy::{GateVerdict, PolicyRepository, gate_check};
use tokio::sync::RwLock;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::thing::{InvokeActionTool, take_pending_action, tool_err, tool_ok};
use crate::domains::event::{
    bus::ThingEventBus,
    router::{ThingEventInput, ThrottleState, route_thing_event},
};

/// Tool arguments (same contract as the chat `invoke_action`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    thing_id: String,
    action_name: String,
    params: Option<Value>,
}

/// Dispatch a device command through the DataServer command queue.
/// Mirrors the dispatch tail in thing.rs:666-700 — keep in sync (O18 forbids
/// editing thing.rs). Returns the executed/simulated payload, or an error
/// result when the queue rejects it.
fn dispatch_command(
    data_server: Option<&Arc<tinyiothub_runtime::DataServer>>,
    thing_id: &str,
    action_name: &str,
    params: Option<&Value>,
) -> ToolResult {
    match data_server.cloned() {
        Some(data_server) => {
            let cmd = tinyiothub_core::models::device_command::DeviceCommand {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: thing_id.to_string(),
                name: action_name.to_string(),
                display_name: None,
                description: None,
                parameters: params.map(|p| p.to_string()),
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            match data_server.execute_command(cmd) {
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
    policy_repo: Arc<PolicyRepository>,
    run_ctx: RunContextSlot,
    pool: SqlitePool,
    workspace_id: String,
    event_bus: Arc<ThingEventBus>,
    throttle: Arc<ThrottleState>,
}

impl AutonomousInvokeActionTool {
    pub fn new(
        inner: InvokeActionTool,
        policy_repo: Arc<PolicyRepository>,
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
        }
    }

    /// Consume the confirmation token minted by the inner tool and dispatch
    /// directly (policy gate replaces human confirmation, O18). Returns None
    /// when the token vanished or mismatches (thing/action/workspace) —
    /// practically unreachable; the caller maps None to a tool error rather
    /// than leaking the confirmation_required payload (and its token) to the
    /// LLM.
    fn auto_confirm(&self, inner_output: &str, input: &Input) -> Option<ToolResult> {
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
        Some(dispatch_command(
            self.inner.data_server.as_ref(),
            &pending.thing_id,
            &pending.action_name,
            pending.params.as_ref(),
        ))
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
        // MUST stay "invoke_action": the T9 runner trajectory matching and
        // the T10 prompt refer to this name.
        "invoke_action"
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
                "actionName": {
                    "type": "string",
                    "description": "操作名称（必需）"
                },
                "params": {
                    "type": "object",
                    "description": "操作参数（可选，JSON 键值对）"
                }
            },
            "required": ["thingId", "actionName"]
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
        let policy = match self.policy_repo.load_autonomy(&self.workspace_id).await {
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
        let hourly = match self.policy_repo.count_actions_last_hour(&self.workspace_id).await {
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

        if let GateVerdict::Deny { reason } = gate_check(policy.as_ref(), &input.action_name, prior_actions, hourly) {
            return denied(&reason);
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
            // Never pass the confirmation_required payload through on
            // auto-confirm failure: it carries a live token to the LLM.
            match self.auto_confirm(&result.output, &input) {
                Some(r) => r,
                None => tool_err("auto-confirm failed: token mismatch or expired; action NOT dispatched".to_string())
                    .expect("static message"),
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::domains::agent::loop_::thing_agent::RunContextInner;
    use crate::domains::thing::service::ThingService;
    use tinyiothub_policy::autonomy::{AutonomyMode, AutonomyPolicy};
    use zeroclaw::tools::Tool;

    use super::*;
    use crate::domains::agent::host::test_utils::seed_test_workspace;
    use tinyiothub_storage::policy::PolicyRepository;

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

    async fn seed_device(pool: &SqlitePool, workspace_id: &str, thing_id: &str, thing_type: &str) {
        seed_test_workspace(pool, "tenant-1", workspace_id).await;
        sqlx::query("INSERT INTO devices (id, name, workspace_id, thing_type) VALUES (?, ?, ?, ?)")
            .bind(thing_id)
            .bind(format!("Device {thing_id}"))
            .bind(workspace_id)
            .bind(thing_type)
            .execute(pool)
            .await
            .expect("insert device");
    }

    async fn register_action(pool: &SqlitePool, thing_id: &str, action_name: &str) {
        sqlx::query("INSERT INTO thing_actions (id, device_id, name) VALUES (?, ?, ?)")
            .bind(format!("act-{action_name}-{thing_id}"))
            .bind(thing_id)
            .bind(action_name)
            .execute(pool)
            .await
            .expect("register action");
    }

    fn act_policy() -> AutonomyPolicy {
        AutonomyPolicy {
            mode: AutonomyMode::Act,
            allowed_actions: vec!["*".to_string()],
            denied_actions: vec!["wipe_device".to_string()],
            max_actions_per_run: 3,
            max_actions_per_hour: 30,
        }
    }

    struct Fixture {
        pool: SqlitePool,
        tool: AutonomousInvokeActionTool,
        ctx: Arc<RwLock<RunContextInner>>,
        bus: Arc<ThingEventBus>,
        policy_repo: Arc<tinyiothub_storage::policy::PolicyRepository>,
    }

    async fn fixture(workspace_id: &str) -> Fixture {
        let pool = test_pool().await;
        let policy_repo = Arc::new(tinyiothub_storage::policy::PolicyRepository::new(pool.clone()));
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let bus = Arc::new(ThingEventBus::new());
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: workspace_id.to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
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
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE device_id = ? AND actor = 'agent'")
            .bind(thing_id)
            .fetch_one(pool)
            .await
            .expect("count agent events")
    }

    // ── deny cases (structured, LLM-readable) ──────────────────

    #[tokio::test]
    async fn deny_when_mode_off_returns_structured_denied() {
        let fx = fixture("ws-off").await;
        seed_device(&fx.pool, "ws-off", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        let mut policy = act_policy();
        policy.mode = AutonomyMode::Off;
        fx.policy_repo.save_autonomy("ws-off", &policy, "test").await.unwrap();

        let result = dispatch(&fx, "dev-1", "reboot").await;
        assert!(
            result.success,
            "deny must be a readable result, not an error: {result:?}"
        );
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "autonomy_not_act");
        assert_eq!(
            agent_event_count(&fx.pool, "dev-1").await,
            0,
            "denied action writes no event"
        );
    }

    #[tokio::test]
    async fn deny_when_no_policy_row_fails_closed() {
        let fx = fixture("ws-none").await;
        seed_device(&fx.pool, "ws-none", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;

        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "autonomy_not_act");
    }

    #[tokio::test]
    async fn deny_when_action_blacklisted() {
        let fx = fixture("ws-bl").await;
        seed_device(&fx.pool, "ws-bl", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "wipe_device").await;
        fx.policy_repo
            .save_autonomy("ws-bl", &act_policy(), "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "dev-1", "wipe_device").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "action_denied");
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 0);
    }

    #[tokio::test]
    async fn deny_when_action_not_in_allowlist() {
        let fx = fixture("ws-al").await;
        seed_device(&fx.pool, "ws-al", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        let mut policy = act_policy();
        policy.allowed_actions = vec!["set_fan".to_string()];
        fx.policy_repo.save_autonomy("ws-al", &policy, "test").await.unwrap();

        let result = dispatch(&fx, "dev-1", "reboot").await;
        let out = output_json(&result);
        assert_eq!(out["denied"], true);
        assert_eq!(out["reason"], "action_not_allowed");
    }

    #[tokio::test]
    async fn deny_when_run_cap_reached_after_exactly_max_dispatches() {
        let fx = fixture("ws-cap").await;
        seed_device(&fx.pool, "ws-cap", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy("ws-cap", &act_policy(), "test")
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
        seed_device(&fx.pool, "ws-fuse", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        let mut policy = act_policy();
        policy.max_actions_per_hour = 2;
        fx.policy_repo.save_autonomy("ws-fuse", &policy, "test").await.unwrap();
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
        // 无 trait 后故障注入方式变更（E3）：给 PolicyRepository 一个缺
        // workspace_autonomy_policy 表的空库，load_autonomy 必然 DbError，
        // 与原 FailingRepo 的 "db down" 等效（fail-closed 路径一致）。
        let failing_repo = PolicyRepository::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect(":memory:")
                .await
                .expect("empty pool"),
        );

        let pool = test_pool().await;
        seed_device(&pool, "ws-err", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-err".to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
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

    // ── allow path ─────────────────────────────────────────────

    #[tokio::test]
    async fn allow_dispatches_and_records_agent_actor_event() {
        let fx = fixture("ws-ok").await;
        seed_device(&fx.pool, "ws-ok", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy("ws-ok", &act_policy(), "test")
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
            sqlx::query_as("SELECT actor, event_subtype FROM events WHERE device_id = 'dev-1' AND actor = 'agent'")
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
    async fn allow_auto_confirms_when_workspace_confirm_gate_on() {
        // require_action_confirm defaults ON (fail-closed, see thing.rs).
        // The autonomous variant replaces the human confirmation branch with
        // the policy gate: an allowed action must dispatch, not mint a token.
        let fx = fixture("ws-conf").await;
        seed_device(&fx.pool, "ws-conf", "dev-1", "device").await;
        register_action(&fx.pool, "dev-1", "reboot").await;
        fx.policy_repo
            .save_autonomy("ws-conf", &act_policy(), "test")
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
        let policy_repo = Arc::new(tinyiothub_storage::policy::PolicyRepository::new(pool.clone()));
        policy_repo
            .save_autonomy(outer_ws, &act_policy(), "test")
            .await
            .unwrap();
        let ctx = Arc::new(RwLock::new(RunContextInner::default()));
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: inner_ws.to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
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
        seed_device(&pool, "ws-inner-mm", "dev-1", "device").await;
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
        seed_device(&pool, "ws-inner-lk", "dev-1", "device").await;
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
        assert!(tool.auto_confirm(&fake, &input).is_none());
    }

    // ── inner behavior reuse ───────────────────────────────────

    #[tokio::test]
    async fn non_device_thing_returns_inner_error_without_event() {
        let fx = fixture("ws-space").await;
        seed_device(&fx.pool, "ws-space", "space-1", "space").await;
        fx.policy_repo
            .save_autonomy("ws-space", &act_policy(), "test")
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
        seed_device(&fx.pool, "ws-unreg", "dev-1", "device").await;
        fx.policy_repo
            .save_autonomy("ws-unreg", &act_policy(), "test")
            .await
            .unwrap();

        let result = dispatch(&fx, "dev-1", "not_registered").await;
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("未在物"));
        assert_eq!(agent_event_count(&fx.pool, "dev-1").await, 0);
    }

    #[tokio::test]
    async fn missing_run_context_treated_as_zero_prior_actions() {
        // Defensive: the factory always binds a context, but an unbound slot
        // must not panic — gate decides on policy + hourly fuse alone.
        let pool = test_pool().await;
        seed_device(&pool, "ws-norun", "dev-1", "device").await;
        register_action(&pool, "dev-1", "reboot").await;
        let policy_repo = Arc::new(tinyiothub_storage::policy::PolicyRepository::new(pool.clone()));
        policy_repo
            .save_autonomy("ws-norun", &act_policy(), "test")
            .await
            .unwrap();
        let inner = InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: "ws-norun".to_string(),
            data_server: None,
            pending_actions: self_pending_actions(),
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

#[cfg(test)]
fn self_pending_actions() -> Arc<super::thing::PendingActionStore> {
    Arc::new(super::thing::PendingActionStore::new())
}

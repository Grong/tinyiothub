# Assistant Legacy Cleanup Implementation Plan (Plan 4B / 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 spec §12 的 Batch 0–6 分批退役被统一 Session Loop 取代的旧路径与命名：冻结旧文档、删除死代码与旧触发器/旧提示词路径、退役 heartbeat/Workflow 实验、收敛 `AgentTask`/`host/`/`brain_events`/`RuntimeEvent` 命名与前端旧视图，并在最后收敛文档。安全/Effect/Decision/恢复内核与 Task/Run 持久化语义一律保留。

**Architecture:** 每批独立提交、独立门禁，先迁后删：先让 device-event / watch 唤醒走 `TriggerRouter`（Plan 4A 已接报警），再删除 legacy workspace loop 的 `run_pipeline`/触发器；AutonomyMode 退役映射到 Schedule `enabled` + 会话 `permission_mode`；Workflow API 与前端视图先停用后删除，Postgres/SQLite 表保留只读一个版本；最后做命名别名收敛与文档冻结。

**Tech Stack:** Rust 2024、Axum、Tokio、SQLx/SQLite、异步任务、Lit/TS、Node 24、vitest/playwright。

## Plan Boundary

- **已完成**：Plan 1（Session Loop/原语/事件/API）、Plan 2（Schedule/TriggerRouter/HUD）、Plan 3（Codex 式前端）、Plan 4A（权限/审批/报警/巡检技能）。
- **本计划**：spec §12 Batch 0–6 的清理与命名收敛，包含 device-event/watch 唤醒迁移（删除旧触发器前必须完成）。
- **保留不动**：Effect / Decision / unknown fence / 对账 / 恢复合同、policy/trust/approval/arbitration、`hub_ledger runtime.v1` 审计账本、Task/Run 持久化与恢复、skills loader、ToolRegistry、MCP、`session_events` 与 `/chat/sessions/*` API。
- **数据库**：本计划**不 drop 任何表**。`heartbeat_tasks`、`agent_workflows`、`workflow_revisions`、`agent_actions`/`agent_runs`、`brain_events` 视图保留只读；下掉留待后续独立迁移（含备份/回滚）。
- **Task runtime 不删除**：`task/coordinator.rs`、`runner.rs`、`registry.rs`、`active_runs.rs`、`seams/*`、`approval_port.rs`、`host/autonomous_factory.rs` 仍服务持久化 Task/Run；只退役其 legacy wake 入口与独立 prompt 拼装。
- **不做**：新功能；计划模式（plan mode）；多层子代理。

---

## Global Constraints

- **安全层不动**：policy / trust / approval / Effect / Decision / unknown fence / 对账 / 恢复合同全部保持；任何清理不得放松设备写审批与 `full_access` 边界。
- **只删被本次设计取代的代码**；与本次无关的死代码不动（CLAUDE.md Surgical Changes）。
- **分批门禁**：每批删除必须在新路径上线、对应测试通过后进行；禁止大爆炸；删除与对应测试同提交。
- **数据库不随代码删**：表/视图先保留只读，独立迁移后再下掉。
- **命名**：`Workspace → Assistant → Session → Turn`，`Task → Run`，`Turn → Subagent`；禁用词清单（spec §5.4.5）对新代码生效；`Task` 只表示持久化工作单元。
- **提交**：每 Task 一 commit；不提交无关工作区文件（`docs/superpowers/specs/2026-11-02-agent-harness-core-design.md` 与 `docs/superpowers/plans/2026-11-03-agent-harness-core.md` 的修改只在本计划 Task 0/6 中进行）。
- **验证**：每批跑 `cargo fmt --all -- --check`、`cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings`、`cargo test -p tinyiothub-cloud`、`cargo test -p db`；涉及前端时 Node 24（`export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`）+ `npm test && npm run type-check && npm run lint && npm run build`。

---

## File Structure（按 Batch）

### Batch 0

- Modify: `docs/superpowers/specs/2026-11-02-agent-harness-core-design.md`（状态横幅）
- Modify: `docs/superpowers/plans/2026-11-03-agent-harness-core.md`（冻结横幅；最终 Batch 6 决定去留）
- Delete: `apps/cloud/src/domains/agent/tools/types.rs`、`host/tools/handler.rs`、`host/config/handler.rs`
- Modify: `apps/cloud/src/domains/agent/tools/mod.rs`、`host/tools/mod.rs`、`host/config/mod.rs`

### Batch 1

- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`（`EventWake` 增加 `dedup_key`；支持 `thing_event`/`watch`）
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`（event 消息按 trigger 分支；权限来自 schedule）
- Modify: `apps/cloud/src/domains/agent/thing_agent/manager.rs`（停止挂载 ThingEventTrigger/WatchTrigger；删除 legacy workspace loop 与 run_pipeline）
- Delete: `thing_agent/trigger/{thing_event,watch,timer,heartbeat}.rs`、`thing_agent/seams/trigger.rs`、`thing_agent/pushback.rs`（确认无 Task 依赖后）
- Delete: `apps/cloud/src/domains/agent/host/tools/dispatch_task.rs`
- Modify: `host/tools/service.rs`、`host/tools/mod.rs`、`prompt/constitution.rs`、`host/handler/workspace_heartbeat.rs`（denylist 引用）
- Modify: `apps/cloud/src/shared/service_manager.rs`（watch engine 仍保留；不再启动 legacy loop）

### Batch 2

- Modify: `host/heartbeat.rs`（删除 md 解析/迁移，只留默认任务或整文件删除）
- Modify: `host/handler/workspace_heartbeat.rs`（删除 legacy 双读分支；tasks/config 端点转为只读或删除）
- Modify: `host/trigger/agent_hooks.rs`、`shared/service_manager.rs`、`tenant/workspace/service.rs`（默认任务/seed 改为技能）
- Modify: `crates/core/src/policy.rs`、`crates/policy/src/autonomy.rs`、`crates/db/src/policy.rs`、`host/tools/autonomous_invoke.rs`、`host/ports.rs`、`session/wake.rs`、`session/workspace_api.rs`、`host/handler/agent_tasks.rs`
- Migration: `crates/db/migrations/20261108000002_assistant_schedule_permission.sql`（`assistant_schedules.permission_mode`）

### Batch 3

- Delete: `workflows/{mod,templates,dry_run,runner,supervisor}.rs`
- Keep: `workflows/health.rs`、`workflows/compaction.rs`、`workflows/projection_contract.rs`（迁入 `session/health.rs` 或保留）
- Modify: `domains/agent/harness.rs`（删除 Workflow CRUD/templates/dry-run/simulations 路由；保留 overview/subagents/supervisor/compaction）
- Rename: `runtime/harness_stream.rs` → `runtime/session_stream.rs`（模块/字段/路径引用）
- Frontend delete: `harness-workflow-edit.ts`、`harness-templates.ts`、`harness-dry-run.ts`（+ tests/css）、`api/harness.ts` 相应函数、`types/harness.ts` 相应 DTO

### Batch 4

- Delete: `host/skill.rs`、`host/reflect.rs`（以及 `host/types.rs` 的 deprecated DTOs）
- Move: `host/{chat,service,types,autonomous_factory,pool_adapter,thing_agent_host,ports,config,persist}` → `session/`；`host/{composite,tools}` → `tool/`；`host/{scaffold,heartbeat}` → `prompt/`
- Rename aliases: `AgentTask` → `Task`、`RunReport` → `Run`、`AgentRunRow` → `RunRow`、`AgentRunOutput` → `RunOutput`（一版 `pub type` 兼容）
- `brain_events` 收窄：保留 judgments 分支，新增 session_events 合并查询（Rust 层合并）；旧 tab/source 一版兼容
- `RuntimeEvent` 仅保留安全/审计用途；Task SSE/事件 API 直接返回 `SessionEvent`

### Batch 5

- Delete: `web/src/ui/views/runtime-*.ts`（保留 `runtime-url-state` 一版）、`harness-overview/workflow-edit/templates/dry-run/supervisor/compaction/status*.ts`（保留 health/supervisor 改造后视图）
- Delete: `chat/grouped-render.ts`、`views/a2ui-fallback.ts`、`styles/chat/{layout,text,grouped,tool-cards,sidebar}.css`
- Create: `web/src/ui/views/assistant-url-state.ts`（canonical + 旧 hash 重定向）
- Modify: `app.ts`（路由/懒加载/导航）

### Batch 6

- Modify: `AGENTS.md`、`README.md`、`docs/runbooks/*`、`docs/designs/*`（状态横幅，不重写历史）
- Move/Delete: `docs/superpowers/plans/2026-11-03-agent-harness-core.md`（untracked；删除或移入 archive）

---

### Task 0: Batch 0 — 冻结文档与清空死代码

**Files:**

- Modify: `docs/superpowers/specs/2026-11-02-agent-harness-core-design.md:4`
- Modify: `docs/superpowers/plans/2026-11-03-agent-harness-core.md:1`
- Delete: `apps/cloud/src/domains/agent/tools/types.rs`
- Delete: `apps/cloud/src/domains/agent/host/tools/handler.rs`
- Delete: `apps/cloud/src/domains/agent/host/config/handler.rs`
- Modify: `apps/cloud/src/domains/agent/tools/mod.rs:18`、`apps/cloud/src/domains/agent/host/tools/mod.rs:7`、`apps/cloud/src/domains/agent/host/config/mod.rs:1`

**Interfaces:**

- Consumes: nothing.
- Produces: 无新接口；删除后 `cargo check` 无引用。

- [ ] **Step 1: 冻结横幅**

在 `docs/superpowers/specs/2026-11-02-agent-harness-core-design.md` 的 `Status: DRAFT` 行之后插入：

```markdown
> **FROZEN 2026-11-08 — historical record.** 产品方向已被 `2026-11-07-assistant-harness-design.md` 取代；
> 安全、Effect/Decision、恢复合同仍为规范。请勿继续在此文件追加设计。
```

在 `docs/superpowers/plans/2026-11-03-agent-harness-core.md` 的 H1 之后插入同一横幅（把 “historical record” 改为 “implementation plan frozen；未实现的步骤作废”）。

- [ ] **Step 2: 删除空 stub 与死模块**

```bash
git rm apps/cloud/src/domains/agent/host/tools/handler.rs \
  apps/cloud/src/domains/agent/host/config/handler.rs \
  apps/cloud/src/domains/agent/tools/types.rs
```

并从：

- `apps/cloud/src/domains/agent/host/tools/mod.rs` 删除 `pub mod handler;`
- `apps/cloud/src/domains/agent/host/config/mod.rs` 删除 `pub mod handler;`
- `apps/cloud/src/domains/agent/tools/mod.rs` 删除 `pub mod types;` 与 `pub use types::*;`

- [ ] **Step 3: 编译与测试**

```bash
cargo check -p tinyiothub-cloud
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
```

Expected: 无引用错误；若 `tools/mod.rs` 仍有对 `types` 的 re-export 引用，一并删除。

- [ ] **Step 4: 提交**

```bash
git add docs/superpowers/specs/2026-11-02-agent-harness-core-design.md \
  docs/superpowers/plans/2026-11-03-agent-harness-core.md \
  apps/cloud/src/domains/agent/host/tools/mod.rs \
  apps/cloud/src/domains/agent/host/config/mod.rs \
  apps/cloud/src/domains/agent/tools/mod.rs
git commit -m "chore(agent): freeze legacy docs and drop dead tool stubs"
```

---

### Task 1: Batch 1a — device-event / watch 唤醒迁入 TriggerRouter

**Files:**

- Modify: `apps/cloud/src/domains/agent/session/trigger.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/domains/agent/thing_agent/manager.rs`（仅 watch engine 订阅点）
- Test: `trigger.rs` / `wake.rs` 测试模块

**Interfaces:**

- `EventWake { workspace_id, trigger, source_id, summary, dedup_key: Option<String> }`（新增字段）。
- `TriggerRouter::submit_event` 的限流键改为 `{workspace}:{trigger}:{dedup_key}`（无 dedup 时退化为 `{workspace}:{trigger}`）。
- 新 trigger 值：`"thing_event"`、`"watch"`。
- 纯函数：
  - `pub(crate) fn thing_event_wake(signal: &ThingEventSignal) -> EventWake`
  - `pub(crate) fn watch_hit_wake(hit: &WatchHit) -> Option<EventWake>`（`owner_task_id.is_some()` → `None`）

- [ ] **Step 1: 写失败测试**

在 `trigger.rs` 测试追加：

```rust
#[tokio::test]
async fn dedup_key_scopes_the_rate_window() {
    let db = Arc::new(test_db().await);
    let router = TriggerRouter::new(db, Arc::new(RecordingExecutor::default()));
    let wake = |rule: &str| EventWake {
        workspace_id: "ws-1".into(),
        trigger: "watch".into(),
        source_id: rule.into(),
        summary: "hit".into(),
        dedup_key: Some(format!("watch:{rule}")),
    };
    for _ in 0..3 {
        assert!(router.submit_event(wake("r1")));
    }
    assert!(!router.submit_event(wake("r1")), "same rule limited");
    assert!(router.submit_event(wake("r2")), "different rule has its own budget");
}

#[test]
fn watch_and_thing_mappings_produce_scoped_wakes() {
    let watch = watch_wake("ws", "rule-1", None, &serde_json::json!(42), "Healthy", "A").unwrap();
    assert_eq!(watch.trigger, "watch");
    assert_eq!(watch.source_id, "rule-1");
    assert_eq!(watch.dedup_key.as_deref(), Some("watch:rule-1"));
    assert!(watch_wake("ws", "rule-1", Some("task-1"), &serde_json::json!(42), "Healthy", "A").is_none());

    let thing = thing_event_wake("ws", "thing-1", "offline", "evt-1");
    assert_eq!(thing.trigger, "thing_event");
    assert_eq!(thing.source_id, "evt-1");
    assert_eq!(thing.dedup_key.as_deref(), Some("thing:thing-1:event:offline"));
}
```

- [ ] **Step 2: 运行测试，确认失败**

```bash
cargo test -p tinyiothub-cloud --lib session::trigger::tests -v
cargo test -p tinyiothub-cloud --lib session::wake::tests -v
```

Expected: FAIL（缺少 `dedup_key` / 映射函数）。

- [ ] **Step 3: 写最小实现**

`trigger.rs`：

```rust
#[derive(Debug, Clone)]
pub struct EventWake {
    pub workspace_id: String,
    pub trigger: String,
    pub source_id: String,
    pub summary: String,
    pub dedup_key: Option<String>,
}

impl EventWake {
    fn rate_key(&self) -> String {
        match &self.dedup_key {
            Some(key) => format!("{}:{}:{}", self.workspace_id, self.trigger, key),
            None => format!("{}:{}", self.workspace_id, self.trigger),
        }
    }
}
```

`submit_event` 使用 `wake.rate_key()`；`event_wake_from_ai` 补 `dedup_key: Some(format!("alarm:{}:{}", alarm.thing_id, alarm.alarm_type))`（沿用旧 watchdog 分键，防止同一设备同类告警刷屏）。

新增（用原始字段做纯映射，便于无 fixture 测试）：

```rust
pub(crate) fn thing_event_wake(
    workspace_id: &str,
    thing_id: &str,
    event_name: &str,
    event_id: &str,
) -> EventWake {
    EventWake {
        workspace_id: workspace_id.to_string(),
        trigger: "thing_event".to_string(),
        source_id: event_id.to_string(),
        summary: format!("设备事件 {event_name}（{thing_id}）"),
        dedup_key: Some(format!("thing:{thing_id}:event:{event_name}")),
    }
}

pub(crate) fn watch_wake(
    workspace_id: &str,
    rule_id: &str,
    owner_task_id: Option<&str>,
    current_value: &serde_json::Value,
    health: &str,
    zone: &str,
) -> Option<EventWake> {
    if owner_task_id.is_some() {
        return None;
    }
    Some(EventWake {
        workspace_id: workspace_id.to_string(),
        trigger: "watch".to_string(),
        source_id: rule_id.to_string(),
        summary: format!("Watch 命中 {rule_id}：value={current_value} health={health} zone={zone}"),
        dedup_key: Some(format!("watch:{rule_id}")),
    })
}
```

调用点（Task 2 的 watch 消费与 device-event 迁移）分别传 `hit.*` 字段与 `signal.*` 字段；`hit.owner_task_id.as_deref()`、`&format!("{:?}", hit.health)`。

同时更新所有 `EventWake { ... }` 字面量（`event_wake_from_ai`、Plan 4A 的 trigger/wake 测试）补 `dedup_key`，用 `rg -n 'EventWake \{' apps/cloud/src` 定位。

`wake.rs` 的 `run_event` 消息按 trigger 分支：

```rust
        let message = match wake.trigger.as_str() {
            "watch" => format!(
                "收到设备值守命中：{}\n请评估影响；涉及写设备必须走审批。",
                wake.summary
            ),
            "thing_event" => format!(
                "收到设备事件：{}\n请判断是否需要处理；涉及写设备必须走审批。",
                wake.summary
            ),
            _ => format!(
                "收到告警：{}\n请按「应急预案」技能（如未加载则按默认职责）评估影响、给出处置建议；涉及写设备必须走审批。",
                wake.summary
            ),
        };
```

- [ ] **Step 4: 运行测试，确认通过**

```bash
cargo test -p tinyiothub-cloud --lib session::trigger::tests -v
cargo test -p tinyiothub-cloud --lib session::wake::tests -v
cargo check -p tinyiothub-cloud
```

- [ ] **Step 5: 提交**

```bash
git add apps/cloud/src/domains/agent/session/trigger.rs \
  apps/cloud/src/domains/agent/session/wake.rs
git commit -m "feat(session): map thing events and watch hits to session wakes"
```

---

### Task 2: Batch 1b — 停用并删除 legacy workspace loop

**Files:**

- Modify: `apps/cloud/src/domains/agent/thing_agent/manager.rs`
- Delete: `thing_agent/trigger/thing_event.rs`、`trigger/watch.rs`、`trigger/timer.rs`、`trigger/heartbeat.rs`、`trigger/mod.rs`、`seams/trigger.rs`、`pushback.rs`
- Modify: `thing_agent/mod.rs`
- Modify: `apps/cloud/src/shared/service_manager.rs`（不再启动 legacy loop / watch trigger）
- Modify: `host/trigger/agent_hooks.rs`（不再提供 legacy 默认任务，若 Batch 2 一并处理则在 Batch 2 删除）
- Test: manager 测试、watch 测试迁移

**Interfaces:**

- 保留：`manager.start_task_runtime`、Task runtime（coordinator/runner/registry/active_runs/seams/approval_port）。
- 删除：`manager.start/stop` 的 per-workspace legacy loop（Scheduler + wake triggers + run_pipeline）、`is_running/workspace_count/active_run_registry` 若无消费者一并删除。
- watch engine 仍由 `watch_engine_loop` 运行；其 `hit_tx` 改为发送到新的 Session 唤醒适配器（task-owned hit 仍由引擎内部 `TaskWake` 处理）。

- [ ] **Step 1: 接通 watch → TriggerRouter（替代 WatchTrigger）**

在 `manager.rs` 的 watch 启动块（~509-535）把 `WatchTrigger` 替换为：

```rust
        let mut hit_rx = hit_rx;
        let router = self.session_router.clone(); // 新增字段，启动时由 service_manager 注入
        let hit_ws = ws.clone();
        let watch_session_task = tokio::spawn(async move {
            while let Some(hit) = hit_rx.recv().await {
                if hit.workspace_id != hit_ws {
                    continue;
                }
                if let Some(wake) = crate::domains::agent::session::trigger::watch_wake(
                    &hit.workspace_id,
                    &hit.rule_id,
                    hit.owner_task_id.as_deref(),
                    &hit.current_value,
                    &format!("{:?}", hit.health),
                    &hit.zone,
                ) {
                    if !router.submit_event(wake) {
                        tracing::warn!(workspace_id = %hit.workspace_id, rule_id = %hit.rule_id, "watch wake not admitted");
                    }
                }
            }
        });
```

`manager` 增加 `session_router: Option<Arc<session::TriggerRouter>>` 与 `set_session_router()`；`service_manager` 在构造 TriggerRouter 后、调用 `start()` 之前注入：

```rust
            thing_agent_manager.set_session_router(trigger_router.clone());
            let trigger_handle = trigger_router.clone().start();
            self.service_handles.write().await.push(trigger_handle);
```

（`thing_agent_manager` 是 `Arc<ThingAgentManager>`，与现有 `start_task_runtime` 的调用点同作用域。）

- [ ] **Step 2: 删除 legacy loop 与触发器**

```bash
git rm apps/cloud/src/domains/agent/thing_agent/trigger/thing_event.rs \
  apps/cloud/src/domains/agent/thing_agent/trigger/watch.rs \
  apps/cloud/src/domains/agent/thing_agent/trigger/timer.rs \
  apps/cloud/src/domains/agent/thing_agent/trigger/heartbeat.rs \
  apps/cloud/src/domains/agent/thing_agent/trigger/mod.rs \
  apps/cloud/src/domains/agent/thing_agent/seams/trigger.rs \
  apps/cloud/src/domains/agent/thing_agent/pushback.rs
```

从 `thing_agent/mod.rs` 删除对应 `pub mod`/re-export；从 `manager.rs` 删除 `start/stop` 中挂载 trigger 的整段、`run_pipeline`、`forward_signals`、`deliver`、heartbeat prompt 相关字段与方法（`heartbeat` 字段若仅被 legacy 使用一并删除；`task_runtime` 相关全部保留）。`AgentHooks::default_heartbeat_tasks` 的调用点若在 legacy loop 中，随删除消失。

- [ ] **Step 3: 更新 startup/stop 与调用方**

`service_manager.rs` 删除 `for ws { thing_agent_manager.start(ws) }`、`set_wake_rate_limit_per_min`、`reload_heartbeat_tasks`、`heartbeat_last_ticks` 相关 wiring（HUD 已不用它们）；保留 `start_task_runtime` 与 watch deps 注入。`orchestrator` 的 `WorkspaceCreated/Deleted` 回调改为只调用 `manager.start_task_runtime`/watch 注册（WorkspaceDeleted 仍停止 watch/task 资源），不再启动 legacy loop。

编译驱动清理：

```bash
cargo check -p tinyiothub-cloud 2>&1 | tee /tmp/cleanup1.check
```

按错误逐个删除残留引用；不允许用 `#[allow(dead_code)]` 掩盖。

- [ ] **Step 4: 测试调整**

把 `thing_agent_loop_tests.rs`/`agent_loop_e2e_tests.rs` 中依赖 legacy `run_pipeline`/trigger 的用例：
- device-event/watch 唤醒改为 `TriggerRouter::submit_event` 的单元/E2E；
- 删除仅测试 legacy 触发器的文件（`trigger/*` 内联测试随文件删除）；
- 保留 Task runtime E2E（`harness_core_e2e_tests.rs` 的 Task/Effect/恢复用例）。

Run:

```bash
cargo test -p tinyiothub-cloud --lib session::trigger::tests
cargo test -p tinyiothub-cloud --lib thing_agent
cargo test -p tinyiothub-cloud --test watch_e2e_tests
cargo check -p tinyiothub-cloud
```

- [ ] **Step 5: 提交**

```bash
git add -A apps/cloud/src/domains/agent/thing_agent \
  apps/cloud/src/shared/service_manager.rs \
  apps/cloud/src/domains/agent/orchestrator
git commit -m "refactor(agent): retire the legacy workspace wake loop"
```

---

### Task 3: Batch 1c — 删除 `dispatch_thing_task` 与旧命名别名

**Files:**

- Delete: `apps/cloud/src/domains/agent/host/tools/dispatch_task.rs`
- Modify: `host/tools/mod.rs`、`host/tools/service.rs`、`prompt/constitution.rs`、`host/handler/workspace_heartbeat.rs`
- Modify: `crates/core/src/agent_task.rs`、`crates/core/src/agent_runs.rs`、`crates/db/src/agent_runs.rs`、`crates/agent/src/runtime/agent/pool.rs`
- Test: `thing_agent_loop_tests.rs` 中 dispatch 用例

**Interfaces:**

- 删除工具 `dispatch_thing_task`（子代理统一用 `spawn_subagent`）。
- 一版兼容别名：`pub type Task = AgentTask;`、`pub type Run = RunReport;`、`pub type RunRow = AgentRunRow;`、`pub type RunOutput = AgentRunOutput;`。

- [ ] **Step 1: 删除工具与引用**

```bash
git rm apps/cloud/src/domains/agent/host/tools/dispatch_task.rs
```

从 `host/tools/mod.rs` 删除 `pub mod dispatch_task;` 与 re-export；`host/tools/service.rs` 删除 import、构造与 safety 分类行；`prompt/constitution.rs` 删除对应能力描述；`workspace_heartbeat.rs` 的 approval denylist 删除该工具名及测试行；删除 `thing_agent_loop_tests.rs` 中 `dispatch_thing_task` 用例。

- [ ] **Step 2: 添加兼容别名**

`crates/core/src/agent_task.rs` 末尾：

```rust
/// 兼容别名（一个版本）：领域内新代码使用 `Task`。
pub type Task = AgentTask;
```

`crates/core/src/agent_runs.rs` 在 `RunReport` 之后：

```rust
/// 兼容别名（一个版本）：领域内新代码使用 `Run`。
pub type Run = RunReport;
```

`crates/db/src/agent_runs.rs`：

```rust
/// 兼容别名（一个版本）：领域内新代码使用 `RunRow`。
pub type RunRow = AgentRunRow;
```

`crates/agent/src/runtime/agent/pool.rs`：

```rust
/// 兼容别名（一个版本）：领域内新代码使用 `RunOutput`。
pub type RunOutput = AgentRunOutput;
```

- [ ] **Step 3: 编译与测试**

```bash
cargo check -p tinyiothub-cloud
cargo test -p tinyiothub-cloud --lib host::tools::service::tests
```

- [ ] **Step 4: 提交**

```bash
git add -A crates/core/src/agent_task.rs crates/core/src/agent_runs.rs \
  crates/db/src/agent_runs.rs crates/agent/src/runtime/agent/pool.rs \
  apps/cloud/src/domains/agent/host/tools \
  apps/cloud/src/domains/agent/prompt/constitution.rs \
  apps/cloud/src/domains/agent/host/handler/workspace_heartbeat.rs \
  apps/cloud/src/tests/thing_agent_loop_tests.rs
git commit -m "refactor(agent): drop dispatch_thing_task and add Task/Run aliases"
```

---

### Task 4: Batch 2 — heartbeat 退役与 AutonomyMode 收敛

**Files:**

- Create: `crates/db/migrations/20261108000002_assistant_schedule_permission.sql`
- Modify: `crates/db/src/assistant_schedule.rs`（读取/写入 `permission_mode`）
- Delete: `apps/cloud/src/domains/agent/host/heartbeat.rs`
- Modify: `host/handler/workspace_heartbeat.rs`、`host/trigger/agent_hooks.rs`、`shared/service_manager.rs`、`tenant/workspace/service.rs`
- Modify: `crates/core/src/policy.rs`、`crates/policy/src/autonomy.rs`、`crates/db/src/policy.rs`、`host/tools/autonomous_invoke.rs`、`host/ports.rs`、`session/wake.rs`、`session/workspace_api.rs`、`host/handler/agent_tasks.rs`

**Interfaces:**

- `assistant_schedules.permission_mode TEXT NOT NULL DEFAULT 'ask'`；`AssistantSchedule.permission_mode: PermissionMode`。
- `AutonomyMode` 删除；映射：
  - `Off` → `assistant_schedules.enabled = false`（Assistant `paused`）
  - `Diagnose` → `PermissionMode::ReadOnly`
  - `Act` → `PermissionMode::Ask`
- `AutonomyPolicy` 保留 `allowed_actions`/`denied_actions`/限额，字段 `mode` 改为 `permission_mode: PermissionMode`（JSON 读取兼容旧 `mode`）。

- [ ] **Step 1: 迁移与 Schedule 字段**

Migration：

```sql
ALTER TABLE assistant_schedules ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'ask';
```

`AssistantSchedule` 加 `permission_mode: PermissionMode`（serde/DB 映射），`ensure/update/get/due` 全部读写该列；`update_assistant_schedule` 的签名增加 `permission_mode: Option<PermissionMode>`。

- [ ] **Step 2: 数据迁移（一次性，幂等）**

`workspace_autonomy_policy` 的真实列是 `mode TEXT NOT NULL CHECK (mode IN ('off','diagnose','act'))`。在 `ensure`（`crates/db/src/assistant_schedule.rs`）插入行之前读取：

```rust
    let (enabled, permission_mode) = match sqlx::query_scalar::<_, String>(
        "SELECT mode FROM workspace_autonomy_policy WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(mode)) if mode.eq_ignore_ascii_case("off") => (false, "ask"),
        Ok(Some(mode)) if mode.eq_ignore_ascii_case("diagnose") => (true, "read_only"),
        Ok(Some(_)) => (true, "ask"),
        Ok(None) => (true, "ask"),
        Err(error) => {
            tracing::warn!(%error, %workspace_id, "load legacy autonomy mode failed; defaulting to ask");
            (true, "ask")
        }
    };
```

随后的 `INSERT OR IGNORE` 绑定这两值（列顺序 `enabled, permission_mode`）。补测试：
- 先写入 `mode='off'` 的 policy 行 → `ensure` 出的 schedule `enabled == false`；
- `mode='diagnose'` → `permission_mode == ReadOnly`；
- `mode='act'` → `permission_mode == Ask`。

- [ ] **Step 3: 删除 AutonomyMode 及调用点改造**

`crates/core/src/policy.rs`：删除 `AutonomyMode` 与其 parse/as_str；`AutonomyPolicy { permission_mode: PermissionMode, allowed_actions, denied_actions, max_actions_per_run, max_actions_per_hour }`；`from_db_json` 兼容旧 `mode` 字段：

```rust
    let permission_mode = match raw.get("permission_mode").and_then(|v| v.as_str()) {
        Some("read_only") => PermissionMode::ReadOnly,
        Some("full_access") => PermissionMode::Ask, // 旧数据不允许 workspace 级 full_access
        Some("ask") => PermissionMode::Ask,
        _ => match raw.get("mode").and_then(|v| v.as_str()) {
            Some("diagnose") => PermissionMode::ReadOnly,
            Some("off") => PermissionMode::ReadOnly,
            _ => PermissionMode::Ask,
        },
    };
```

`gate_check`：`ReadOnly` 拒绝所有写动作（沿用现有 allowlist 错误信息）；`Ask` 保留 allow/deny 列表与限额。

`session/wake.rs`：删除 autonomy 读取；改为读 schedule 行本身的 `enabled` 与 `permission_mode`：

```rust
        if !schedule.enabled { return Skipped; }
        let permission_mode = schedule.permission_mode;
        ... `.with_permission_mode(permission_mode)`
```

`run_event` 需要读取 schedule（`self.db.get_assistant_schedule(&workspace_id)`）；无行时按 `enabled=true, Ask` 的默认行为继续（避免事件唤醒因缺行失效），随后由 `book`。

`session/workspace_api.rs`：`paused = !schedule.enabled`；HUD 展示 `permissionMode` 可选加入 schedule DTO。

`host/tools/autonomous_invoke.rs`：`policy.gate(...)` 改为 `permission_mode` 版本；保留 deny/allow 与限额。

`host/handler/agent_tasks.rs`：删除 `policy` 的 `mode` 字段，读写 `permission_mode`；`Off` 的 drain 行为改为 `enabled=false` 时的既有 drain 语义（若存在 `assistant/pause` 语义则保持）。

- [ ] **Step 4: heartbeat md 退役**

```bash
git rm apps/cloud/src/domains/agent/host/heartbeat.rs
```

从 `host/mod.rs` 删除 `pub mod heartbeat;`；`workspace_heartbeat.rs` 删除 `load_tasks` 的迁移/文件回退（直接 `db.list_heartbeat_tasks`，失败返回空并 warn）；删除 `get_logs` 的 `legacy_logs_from_agent_actions` 分支及 `parse_action_content/parse_action_trace`；`service_manager.rs` 删除 `migrate_file_tasks_to_db` 启动步骤与 boot reseed（任务已由 `inspection` 技能承接）；`workspace/service.rs` 删除 `seed_default_heartbeat_tasks` 与 `get_default_tasks` 依赖（仅保留技能种子）。`AgentHooks::default_heartbeat_tasks` 改为返回空（或在 Batch 1 已删除）。

`heartbeat_tasks` 表保留只读：保留 `Db::list_heartbeat_tasks`（供 `skills_seed` 读取旧任务生成巡检正文）。`update_tasks` 端点改为“一版兼容”：仍返回 200，但写入被忽略并返回空列表 + `deprecated: true` 字段，提示改用巡检技能；`get_tasks` 继续返回旧任务（只读）。这样 Batch 5 删除旧前端之前不会出现运行时错误。

- [ ] **Step 5: 测试**

新增/调整：

- `crates/db`：schedule 权限列往返、`ensure` 从 policy 映射（off/diagnose/act）。
- `session/wake`：scheduled/event 使用 schedule.permission_mode；`enabled=false` 跳过（原 `AutonomyMode::Off` 测试改为 enabled=false）。
- `policy`：`ReadOnly` 拒绝写、`Ask` 保留限额；旧 JSON `mode` 兼容。
- `workspace_heartbeat`：tasks 端点返回 410；日志只读不再读 `agent_actions`。
- 删除 heartbeat md 解析/迁移测试（随文件删除）。

Run:

```bash
cargo test -p db policy
cargo test -p db assistant_schedule
cargo test -p tinyiothub-cloud --lib session
cargo test -p tinyiothub-cloud --lib thing_agent
cargo test -p tinyiothub-cloud --lib host::handler::workspace_heartbeat
cargo check -p tinyiothub-cloud
```

- [ ] **Step 6: 提交**

```bash
git add -A crates crates/db/migrations/20261108000002_assistant_schedule_permission.sql \
  apps/cloud/src/domains/agent/session \
  apps/cloud/src/domains/agent/host \
  apps/cloud/src/shared/service_manager.rs \
  apps/cloud/src/domains/tenant/workspace/service.rs
git commit -m "refactor(agent): retire heartbeat tasks and autonomy mode"
```

---

### Task 5: Batch 3 — Workflow 实验退役

**Files:**

- Delete: `workflows/{mod,templates,dry_run,runner,supervisor}.rs`
- Keep/Move: `workflows/health.rs`、`workflows/compaction.rs`、`workflows/projection_contract.rs`
- Modify: `domains/agent/harness.rs`、`api/mod.rs`（router mount 保持）
- Rename: `runtime/harness_stream.rs` → `runtime/session_stream.rs`
- Frontend: delete `harness-workflow-edit.ts`、`harness-templates.ts`、`harness-dry-run.ts`（+ test/css）、`api/harness.ts` 对应函数、`types/harness.ts` 对应 DTO、`app.ts` 路由

**Interfaces:**

- 保留 HTTP：`GET /harness/overview`、`GET /harness/main/{task_id}/subagents`、`GET /harness/supervisor`、`POST /harness/supervisor/dead-wakes/{task}/{event}/retry`、compaction（若管理员仍需）。
- 删除 HTTP：workflows CRUD/versions/snapshots/dry-run/simulations/templates。
- `HarnessStreamHub` → `SessionStreamHub`（`AgentState.session_stream`），路径 `/agent/tasks/{id}/stream` 保留一版（避免前端破坏），Batch 5 再改名。

- [ ] **Step 1: 删除 Workflow 代码与路由**

```bash
git rm apps/cloud/src/domains/agent/workflows/mod.rs \
  apps/cloud/src/domains/agent/workflows/templates.rs \
  apps/cloud/src/domains/agent/workflows/dry_run.rs \
  apps/cloud/src/domains/agent/workflows/runner.rs \
  apps/cloud/src/domains/agent/workflows/supervisor.rs \
  apps/cloud/templates/workflows/inspection.json \
  apps/cloud/templates/workflows/video.json \
  apps/cloud/templates/workflows/energy.json
```

在 `workflows/mod.rs` 删除后，新增 `workflows/mod.rs` 只保留：

```rust
pub mod compaction;
pub mod health;
pub mod projection_contract;

pub use health::{SupervisorHealth, SupervisorHealthSnapshot, SupervisorStatus};
```

`harness.rs` 删除 Workflow handler 与路由；`overview_payload` 的 `workflows` 数组改为恒为空数组 `[]`（保持响应形状一版），`attention`/`coordination` 不变。删除对 `WorkflowService` 的字段/构造/依赖。

- [ ] **Step 2: harness_stream 重命名**

```bash
git mv apps/cloud/src/domains/agent/runtime/harness_stream.rs \
  apps/cloud/src/domains/agent/runtime/session_stream.rs
rg -l 'harness_stream' apps/cloud/src | xargs perl -pi -e 's/harness_stream/session_stream/g'
rg -l 'HarnessStreamHub' apps/cloud/src | xargs perl -pi -e 's/HarnessStreamHub/SessionStreamHub/g'
```

`runtime/mod.rs` 的模块声明同步；`AgentState` 字段改为 `session_stream`；`state.rs`、`service_manager.rs`、`session/*`、`runtime/*` 的引用由编译器发现并修正。测试同步改名。

- [ ] **Step 3: 前端删除 Workflow 界面**

```bash
git rm web/src/ui/views/harness-workflow-edit.ts web/src/ui/views/harness-workflow-edit.test.ts web/src/styles/views/harness-workflow-edit.css \
  web/src/ui/views/harness-templates.ts web/src/ui/views/harness-templates.test.ts web/src/styles/views/harness-templates.css \
  web/src/ui/views/harness-dry-run.ts web/src/ui/views/harness-dry-run.test.ts web/src/styles/views/harness-dry-run.css
```

（CSS 真实路径以 `harness-*.ts` 顶部 import 为准；若在 `styles/views/` 之外，按实际路径删除。）

从 `harness-overview.ts` 删除指向 workflows/templates/dry-run 的链接与 import；从 `harness-url-state.ts` 删除对应路由解析；从 `app.ts` 删除 lazy loader/render 分支；`api/harness.ts`/`types/harness.ts` 删除仅被上述视图使用的函数/DTO；对应测试删除或收窄到 overview/supervisor/compaction。

- [ ] **Step 4: 门禁**

```bash
cargo fmt --all -- --check
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p tinyiothub-cloud
cargo test -p db
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npm test && npm run type-check && npm run lint && npm run build
```

若 `harness_core_e2e_tests.rs` 中 Workflow 用例失败：删除对应测试段落（它们是 Workflow 专属），Effect/Decision/Task/恢复用例必须保留并通过。

- [ ] **Step 5: 提交**

```bash
git add -A apps/cloud/src/domains/agent/workflows \
  apps/cloud/src/domains/agent/harness.rs \
  apps/cloud/src/domains/agent/runtime \
  apps/cloud/src/state.rs apps/cloud/src/shared/service_manager.rs \
  apps/cloud/templates/workflows web/src
git commit -m "refactor(agent): retire the workflow experiment"
```

---

### Task 6: Batch 4 — 兼容层与命名收敛

**Files:**

- Delete: `host/skill.rs`、`host/reflect.rs`；`host/types.rs` 中未使用 DTO
- Move: `host/chat/*` → `session/chat/*`；`host/service.rs` → `session/index.rs`；`host/types.rs` → `session/chat_types.rs`；`host/autonomous_factory.rs` → `session/factory.rs`；`host/pool_adapter.rs` → `session/pool_adapter.rs`；`host/thing_agent_host.rs` → `session/agent_host.rs`；`host/composite.rs` → `tool/composite.rs`；`host/tools/*` → `tool/*`；`host/scaffold.rs` → `prompt/scaffold.rs`；`host/ports.rs` → `session/ports.rs`；`host/config/*` → `session/config/*`；`host/persist/*` → `session/persist/*`
- Modify: `domains/agent/mod.rs`、`api/mod.rs`、`state.rs`、`shared/service_manager.rs`、全部 import
- Rename types via aliases（Task 3 已建）；`brain_events` 收窄；Task SSE 改发 `SessionEvent`

**Interfaces:**

- `host/` 目录仅作为一版 re-export 兼容层（`host/mod.rs` 只保留 `pub use` 与 `pub mod` 指向新路径），不再新增代码。
- `brain_events`：保留 `judgments` 源；`agent_actions`/`agent_runs` 源删除；新增 `source = "session"`，从 workspace DB `recent_session_events` 读取并在 handler 合并；旧 `tab`/`source` 参数一版兼容映射到 session。
- Task SSE/事件 API 返回 `HarnessEvent`（`SessionEvent`）字段，不再转 `RuntimeEvent`。

- [ ] **Step 1: host 拆分（机械移动）**

```bash
mkdir -p apps/cloud/src/domains/agent/session/chat apps/cloud/src/domains/agent/tool apps/cloud/src/domains/agent/session/persist
git mv apps/cloud/src/domains/agent/host/chat/* apps/cloud/src/domains/agent/session/chat/
git mv apps/cloud/src/domains/agent/host/service.rs apps/cloud/src/domains/agent/session/index.rs
git mv apps/cloud/src/domains/agent/host/types.rs apps/cloud/src/domains/agent/session/chat_types.rs
git mv apps/cloud/src/domains/agent/host/autonomous_factory.rs apps/cloud/src/domains/agent/session/factory.rs
git mv apps/cloud/src/domains/agent/host/pool_adapter.rs apps/cloud/src/domains/agent/session/pool_adapter.rs
git mv apps/cloud/src/domains/agent/host/thing_agent_host.rs apps/cloud/src/domains/agent/session/agent_host.rs
git mv apps/cloud/src/domains/agent/host/composite.rs apps/cloud/src/domains/agent/tool/composite.rs
git mv apps/cloud/src/domains/agent/host/tools/* apps/cloud/src/domains/agent/tool/
git mv apps/cloud/src/domains/agent/host/scaffold.rs apps/cloud/src/domains/agent/prompt/scaffold.rs
git mv apps/cloud/src/domains/agent/host/ports.rs apps/cloud/src/domains/agent/session/ports.rs
git mv apps/cloud/src/domains/agent/host/config apps/cloud/src/domains/agent/session/config
git mv apps/cloud/src/domains/agent/host/persist apps/cloud/src/domains/agent/session/persist
mkdir -p apps/cloud/src/domains/agent/session/handlers
git mv apps/cloud/src/domains/agent/host/handler/*.rs apps/cloud/src/domains/agent/session/handlers/
git mv apps/cloud/src/domains/agent/host/judge apps/cloud/src/domains/agent/session/judge
git mv apps/cloud/src/domains/agent/host/notify apps/cloud/src/domains/agent/session/notify
git mv apps/cloud/src/domains/agent/host/trigger apps/cloud/src/domains/agent/session/triggers
```

（`session/handler.rs` 与 `session/handlers/` 是两个不同模块，避免与 Plan 1 的 chat events handler 冲突；`host/handler/mod.rs` 删除，改用 `session/handlers/mod.rs` 逐个 `pub mod`。）

逐文件把 `mod.rs`/`pub use` 指向新位置；用：

```bash
rg -l 'crate::domains::agent::host::' apps/cloud/src | xargs perl -pi -e '
  s/crate::domains::agent::host::chat/crate::domains::agent::session::chat/g;
  s/crate::domains::agent::host::service/crate::domains::agent::session::index/g;
  s/crate::domains::agent::host::types/crate::domains::agent::session::chat_types/g;
  s/crate::domains::agent::host::autonomous_factory/crate::domains::agent::session::factory/g;
  s/crate::domains::agent::host::pool_adapter/crate::domains::agent::session::pool_adapter/g;
  s/crate::domains::agent::host::thing_agent_host/crate::domains::agent::session::agent_host/g;
  s/crate::domains::agent::host::composite/crate::domains::agent::tool::composite/g;
  s/crate::domains::agent::host::tools/crate::domains::agent::tool/g;
  s/crate::domains::agent::host::scaffold/crate::domains::agent::prompt::scaffold/g;
  s/crate::domains::agent::host::ports/crate::domains::agent::session::ports/g;
  s/crate::domains::agent::host::config/crate::domains::agent::session::config/g;
  s/crate::domains::agent::host::persist/crate::domains::agent::session::persist/g;
'
```

`host/mod.rs` 改为：

```rust
//! Deprecated compatibility shell (one version). Import the new paths instead.
pub use crate::domains::agent::prompt::scaffold;
pub use crate::domains::agent::session::{agent_host, chat, chat_types, config, factory, index, persist, pool_adapter, ports};
pub use crate::domains::agent::tool;
```

（按实际 re-export 需要保留的一版别名；不再 `pub mod` 实际文件。）

`cargo check` 驱动修正所有 `host::X` 内部引用；不允许 `allow(dead_code)` 掩盖。

- [ ] **Step 2: 删除 compat 文件与 deprecated DTO**

```bash
git rm apps/cloud/src/domains/agent/host/skill.rs apps/cloud/src/domains/agent/host/reflect.rs
```

`host/handler/skills.rs` 改用 `tinyiothub_skills::SkillDefinition::parse_frontmatter`；`host/types.rs`（已移动为 `session/chat_types.rs`）删除 `ParsedSessionKey` 与无消费者的 `ChatMessage/CompactedSession/ThingSnapshot/AgentMemoryItem/MemoryContext/MemoryError`（以 `cargo check` 确认零引用为准）。

- [ ] **Step 3: brain_events 收窄**

删除 `crates/db/src/brain_event.rs` 中 `agent_actions`/`agent_runs` 分支的 SQL（保留 judgments 分支）；handler 合并 `source="session"`：

```rust
let mut events = db.list_brain_events(workspace_id, filter, limit, offset).await?;
if filter.includes_session() {
    let session_events = db.recent_session_events(workspace_id, limit).await?;
    events.extend(session_events.into_iter().map(|event| BrainEvent::from_session(&workspace_id, event)));
}
```

`BrainEvent::from_session`（加在 `crates/db/src/brain_event.rs`，字段类型以现有结构为准——`created_at` 是 String）：

```rust
impl BrainEvent {
    pub fn from_session(workspace_id: &str, event: tinyiothub_core::harness::HarnessEvent) -> Self {
        Self {
            id: format!("session:{}", event.event_id),
            workspace_id: workspace_id.to_string(),
            source: "session".to_string(),
            alarm_id: None,
            thing_id: None,
            title: event.event_type.clone(),
            verdict: None,
            reason: String::new(),
            suggested_action: None,
            action_category: None,
            risk: None,
            run_id: event.run_id.clone(),
            tick_id: None,
            ticket_id: None,
            status: "info".to_string(),
            triage_mode: String::new(),
            created_at: event.occurred_at.to_rfc3339(),
            judged_at: None,
            state_entered_at: None,
            resolved_at: None,
        }
    }
}
```

旧 `tab=patrol|directive`/`source=patrol|patrol_tick|directive` 映射到 `session`（一版兼容）；`/{id}` 仍解析旧 `source:id` 前缀。前端 `dispositions.ts` 的 tab/source 文案更新；`run-detail.ts` 的 fallback 改读新 source。

- [ ] **Step 4: Task 事件改用 SessionEvent**

`crates/db/src/harness_runtime_api.rs` 删除 `harness_event_to_runtime` 转换；`harness_task_events_after/before` 返回 `HarnessEvent`；`runtime/session_stream.rs` 的 frame 直接序列化 `HarnessEvent`（字段 `seq/eventType/payloadJson`）。`web/src/api/runtime-stream.ts` 与 `runtime-projection.ts`/`runtime-flow.ts`/`runtime-task.ts` 的解析改为 `seq/eventType/payloadJson`（若 Batch 5 删除这些视图，则仅改 `runtime-stream.ts` 的消费者并在 Batch 5 删除前保持编译）。`RuntimeEvent` 仅保留给设备/watch/media recorder。

- [ ] **Step 5: 门禁**

```bash
cargo fmt --all -- --check
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p tinyiothub-cloud
cargo test -p db
cd web && npm test && npm run type-check && npm run build
```

- [ ] **Step 6: 提交**

```bash
git add -A apps/cloud/src/domains/agent crates web/src
git commit -m "refactor(agent): converge host modules and session event naming"
```

---

### Task 7: Batch 5 — 前端收敛

**Files:**

- Delete: `web/src/ui/views/runtime-*.ts`（保留 `runtime-url-state.ts`）
- Delete: `web/src/ui/views/harness-overview.ts`、`harness-supervisor.ts`、`harness-compaction.ts`、`harness-status.ts`（+tests/css）
- Delete: `web/src/ui/chat/grouped-render.ts`、`views/a2ui-fallback.ts`
- Delete: `web/src/styles/chat/{layout,text,grouped,tool-cards,sidebar}.css` 并更新 `chat.css`
- Create: `web/src/ui/views/assistant-url-state.ts`
- Modify: `app.ts`、`api/runtime-stream.ts`、`dispositions.ts`、`run-detail.ts`、`agents-*` tabs

**Interfaces:**

- `assistant-url-state.ts`：canonical `#/assistant`、`#/assistant/sessions/{enc}`、`#/assistant/sessions/{enc}?view=...&device=&at=&event=`；保留 `parseRuntimeHash`/`parseHarnessHash` 作为**只读重定向输入**（一版）。
- `app.ts`：旧 `#/runtime*`、`#/harness*` → `assistant` 路由的 `history.replaceState` 重定向（保留 `from` 返回路径语义）；导航移除 Harness/Runtime 项。

- [ ] **Step 1: 删除旧视图与其测试**

```bash
git rm 'web/src/ui/views/runtime-*.ts' \
  web/src/ui/views/harness-overview.ts web/src/ui/views/harness-overview.test.ts \
  web/src/ui/views/harness-supervisor.ts web/src/ui/views/harness-supervisor.test.ts \
  web/src/ui/views/harness-compaction.ts web/src/ui/views/harness-compaction.test.ts \
  web/src/ui/views/harness-status.ts \
  web/src/ui/views/harness-accessibility.test.ts \
  web/src/ui/chat/grouped-render.ts \
  web/src/ui/views/a2ui-fallback.ts \
  web/src/styles/chat/layout.css web/src/styles/chat/text.css \
  web/src/styles/chat/grouped.css web/src/styles/chat/tool-cards.css \
  web/src/styles/chat/sidebar.css
```

保留 `runtime-url-state.ts`、`harness-url-state.ts`（重定向输入）；删除其余 harness 视图文件与其 CSS（`styles/views/harness-*.css`）。`styles/chat.css` 只保留 `codex.css` import；`components/chat.css` 保留 composer 样式。

- [ ] **Step 2: 新增 assistant URL 状态**

`assistant-url-state.ts`：

```ts
export interface AssistantHashState {
  view: 'assistant';
  sessionKey?: string;
  taskId?: string;
  focus?: { view?: string; device?: string; at?: string; event?: string };
  anchor?: string;
}

export function parseAssistantHash(hash: string): AssistantHashState | null {
  const raw = hash.replace(/^#\/?/, '');
  if (!raw.startsWith('assistant')) return null;
  const [pathPart, queryPart = ''] = raw.split('?');
  const params = new URLSearchParams(queryPart);
  const segments = pathPart.split('/').filter(Boolean); // ['assistant', 'sessions', id]
  const state: AssistantHashState = { view: 'assistant' };
  if (segments[1] === 'sessions' && segments[2]) state.sessionKey = decodeURIComponent(segments[2]);
  const focusView = params.get('view');
  if (focusView) {
    state.focus = {
      view: focusView,
      device: params.get('device') ?? undefined,
      at: params.get('at') ?? undefined,
      event: params.get('event') ?? undefined,
    };
  }
  return state;
}

export function assistantUrl(state: AssistantHashState): string {
  const base = state.sessionKey
    ? `#/assistant/sessions/${encodeURIComponent(state.sessionKey)}`
    : '#/assistant';
  if (!state.focus) return base;
  const params = new URLSearchParams();
  if (state.focus.view) params.set('view', state.focus.view);
  if (state.focus.device) params.set('device', state.focus.device);
  if (state.focus.at) params.set('at', state.focus.at);
  if (state.focus.event) params.set('event', state.focus.event);
  const query = params.toString();
  return query ? `${base}?${query}` : base;
}
```

`app.ts` 路由：

```ts
const assistantState = assistantUrlState.parseAssistantHash(window.location.hash);
if (assistantState) {
  this.currentRoute = { view: 'chat', params: { sessionKey: assistantState.sessionKey, focus: assistantState.focus } } as never;
  return;
}
// 一版兼容：旧 hash 先映射再重定向
const harnessState = parseHarnessHash(window.location.hash);
if (harnessState) {
  const target = assistantUrl({ view: 'assistant', sessionKey: harnessState.taskId });
  window.history.replaceState({}, '', target);
  return this.handleRoute();
}
const runtimeState = parseRuntimeHash(window.location.hash);
if (runtimeState) {
  const target = assistantUrl({ view: 'assistant', sessionKey: runtimeState.taskId });
  window.history.replaceState({}, '', target);
  return this.handleRoute();
}
```

（字段名以 `harness-url-state.ts`/`runtime-url-state.ts` 的实际返回类型为准；编译通过为准。）

- [ ] **Step 3: 更新 hub/agents/dispositions**

- `views/agents.ts` 删除 heartbeat 与旧 skills/tools/workspace 标签页（改用 workspace 的 skills/prompt 编辑入口）；`controllers/agents.ts` 删除 heartbeat 段（191-293）。
- `views/heartbeat.ts` 删除（无导航入口）。
- `dispositions.ts`/`run-detail.ts` 适配 Batch 4 的 `session` source 与旧 tab 兼容。
- `controllers/chat.ts`/`views/tickets.ts`/`views/workspace.ts` 仍使用，保留。

- [ ] **Step 4: 门禁**

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npm test && npm run type-check && npm run lint && npm run build
npx playwright test e2e/assistant.spec.ts
```

- [ ] **Step 5: 提交**

```bash
git add -A web
git commit -m "refactor(web): converge the assistant-only frontend"
```

---

### Task 8: Batch 6 — 文档收敛

**Files:**

- Modify: `AGENTS.md:160,165-175,185,195-197`
- Modify: `README.md:469-473`
- Modify: `docs/runbooks/{harness-recovery,runtime-rollout,harness-soak}.md`
- Modify: `docs/designs/{agent-harness-core,heartbeat-trust-levels,ai-brain-p0-pr99-merge-gate}.md`（状态横幅）
- Move/Delete: `docs/superpowers/plans/2026-11-03-agent-harness-core.md`

**Interfaces:**

- 不重写历史设计正文；在文件顶部加 “SUPERSEDED / HISTORICAL” 横幅并指向 `2026-11-07-assistant-harness-design.md`。
- `AGENTS.md` 更新为当前架构：Session Loop、`session/`、`TriggerRouter`、`tool/`、`prompt/`；删除已退出的 Workflow/heartbeat 路径引用。

- [ ] **Step 1: 更新 AGENTS.md 与 README**

`AGENTS.md`：
- 160 行附近把 `crates/db/src/harness_workflow.rs` 的“Task/Run/Wake 所有者”改为 `session/` + Task runtime（`thing_agent/task/coordinator.rs`）。
- 删除 Workflow/Heartbeat 描述；把 `web/src/ui/views/harness-status.ts` 引用改为 `timeline-render.ts`/`global-hud.ts`。

`README.md` 删除 `/agents/{id}/heartbeat/*` 端点段，改为 `/agent/schedule`、`/agent/hud`、`/chat/sessions/*`。

- [ ] **Step 2: runbooks**

`harness-recovery.md`：Workflow 恢复段落改为 Task/Run 恢复（Effect/Decision/unknown fence 保留）；`#/harness/supervisor` 改为 `#/assistant`（或一版兼容路由）；删除 `agent_workflows`/`workflow_cron_cursors` 步骤。
`runtime-rollout.md`：`#/runtime` → `#/assistant`；`heartbeat_config` → `assistant_schedules`。
`harness-soak.md`：Workflow revision → Session/Task id。

- [ ] **Step 3: 历史文档横幅**

在 `docs/designs/agent-harness-core.md`、`docs/designs/heartbeat-trust-levels.md`、`docs/designs/ai-brain-p0-pr99-merge-gate.md` 顶部插入：

```markdown
> **HISTORICAL / SUPERSEDED (2026-11-08).** 当前设计见 `docs/superpowers/specs/2026-11-07-assistant-harness-design.md`。
> 本文仅作历史记录，路径与流程可能已退役。
```

- [ ] **Step 4: 2026-11-03 plan**

该文件未被跟踪；按 spec “删除或移入 archive”，保留历史、不删除用户内容：

```bash
mkdir -p docs/superpowers/archive
git add docs/superpowers/plans/2026-11-03-agent-harness-core.md
git mv docs/superpowers/plans/2026-11-03-agent-harness-core.md docs/superpowers/archive/2026-11-03-agent-harness-core.md
```

并在归档文件顶部加同一条 FROZEN 横幅。

- [ ] **Step 5: 全量门禁**

```bash
cargo fmt --all -- --check
cargo clippy -p tinyiothub-cloud --all-targets -- -D warnings
cargo test -p tinyiothub-cloud
cargo test -p db
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd web && npm test && npm run type-check && npm run lint && npm run build
git grep -nE 'ThingAgent|AutonomousAgent|dispatch_thing_task|subagent\.done' || true
git grep -nE 'runtime-url-state|harness-url-state' | head
```

Expected: 禁用词 grep 零命中（兼容层/历史说明除外）；url-state 仅命中重定向兼容层；全部门禁绿。

- [ ] **Step 6: 提交**

```bash
git add -A AGENTS.md README.md docs
git commit -m "docs: converge assistant-only architecture docs"
```

---

## 验证清单

- [ ] Batch 0：旧文档有 FROZEN 横幅；空 stub/死模块删除后 `cargo check`/clippy 绿。
- [ ] Batch 1：device-event/watch 唤醒经 `TriggerRouter` 进入 `kind="event"` Session（含 dedup key、Task-owned watch 不双跑）；legacy `run_pipeline`/Timer/Heartbeat/ThingEvent/Watch 触发器与 `pushback` 已删除；`dispatch_thing_task` 不存在；Task runtime 仍工作。
- [ ] Batch 2：`AutonomyMode` 零引用；`assistant_schedules.permission_mode` 生效；旧 policy 数据（off/diagnose/act）迁移正确；`HEARTBEAT.md`/`.migrated` 代码删除；`heartbeat_tasks` 保留只读。
- [ ] Batch 3：Workflow CRUD/templates/dry-run/simulations 路由与前端视图不存在；overview/subagents/supervisor/compaction 仍工作；`session_stream` 命名生效；表保留只读。
- [ ] Batch 4：`host/` 仅一版 re-export；`brain_events` 仅 judgments + session 源；Task SSE 返回 `SessionEvent`；`AgentTask` 通过 `Task` 别名可用。
- [ ] Batch 5：前端只保留 assistant 视图族；旧 hash 重定向可用；无死 CSS/视图；web 门禁绿。
- [ ] Batch 6：文档指向新路径；旧设计有 HISTORICAL 横幅；禁用词 grep 零命中。
- [ ] 全程：无表被 drop；Effect/Decision/unknown fence/对账/恢复测试一条不少；设备写审批路径未放松。

---

## Self-Review

**Spec coverage（§12）：**

- Batch 0：Task 0。
- Batch 1：Task 1（event/watch 迁移）+ Task 2（legacy loop 退役）+ Task 3（dispatch/命名别名）。
- Batch 2：Task 4。
- Batch 3：Task 5。
- Batch 4：Task 6。
- Batch 5：Task 7。
- Batch 6：Task 8。

**Placeholder scan：** 删除/移动/重命名步骤给出精确 `git rm`/`git mv`/`rg+perl` 命令与目标路径；需要新代码的步骤给出完整代码。批量重命名以 `cargo check` 错误清单为完成标准（这是机械操作，不是占位）。

**Type consistency：** `EventWake.dedup_key` 在 Task 1 引入并被 Task 2 的 watch/thing 映射使用；`AssistantSchedule.permission_mode` 在 Task 4 引入并被 wake/HUD 使用；`SessionStreamHub` 在 Task 5 统一替换；`Task/Run` 别名在 Task 3 引入，后续任务使用别名。

**风险与回滚：**

1. Batch 1/2 删除量大：每批独立提交 + 全量门禁；若 Task runtime 出现隐藏耦合，按编译错误逐项保留（Task runtime 优先）。
2. AutonomyMode 映射改变了 `/agent/policy` 语义：保留一版旧字段读取与 UI 提示；不得把 `Act` 映射为 `full_access`。
3. Workflow 表与 `brain_events` 视图保留只读；不可在本计划中 drop。
4. 前端旧 hash 重定向保留一版；删除视图前先确认无新 UI 依赖（当前已核实新 assistant UI 零依赖）。
5. 每批后运行 `git diff` 审查，禁止 `#[allow(dead_code)]` 掩盖残留引用。

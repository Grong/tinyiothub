# TinyIoTHub Harness Core Implementation Plan

> **FROZEN 2026-11-08 — implementation plan frozen；未实现的步骤作废.** 产品方向已被 `2026-11-07-assistant-harness-design.md` 取代；
> 安全、Effect/Decision、恢复合同仍为规范。请勿继续在此文件追加设计。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 TinyIoTHub 中交付配置驱动的 24x7 Main/Subagent Harness、独立 workspace 事实库、统一 Effect/Decision/Command、可恢复投影和独立 Harness 操作界面。

**Architecture:** `apps/cloud` 负责 Host/Supervisor 与组合根，`crates/agent` 提供通用纯 Loop/reducer，`crates/db` 拥有全部 SQL 和 workspace DB 生命周期，`crates/runtime` 保留 Device Hub。`session_events` 是执行事实，`effect_intents`/`decisions`/`commands` 是权威状态机，快照与 `device_jobs` 是可重建投影；前端只投影后端事实。

**Tech Stack:** Rust 2024、Axum、Tokio、SQLx/SQLite WAL、rig-core/rig-agent 0.42.0、Lit 3、Vite、TypeScript、Vitest、Playwright。

**Spec:** `docs/superpowers/specs/2026-11-02-agent-harness-core-design.md`

## Global Constraints

- 不新建 `crates/harness`/`harness-rig`，不递归 Subagent，不做通用插件 DAG 或 UI 面板插件系统。
- 不做历史数据迁移或旧 schema 兼容；但旧 `#/runtime*` 深链接必须按 R31 保留状态并重定向。
- 所有 SQL 只在 `crates/db`；`crates/agent` 无 Axum、SQLx、Device Hub 或设备实现依赖。
- 所有新写面要求 Agent 管理权限，并在打开 workspace DB 前完成 tenant/workspace/角色校验。
- `dispatching` 提交是唯一物理发送许可；SQLite 事务内不得调用设备或 provider，未知结果绝不盲重试。
- 设备联锁、资源仲裁和 e-stop 留在设备/驱动侧；模型、Workflow 和 UI 不得绕过。
- 不新增固定 Task/媒体/事件配额；使用自然背压、指标、告警、分页和增量渲染。
- 用户可见文案中文；固定 `scheduled=计划`、`pending=等待调度`、`unknown=结果未知`、`cancelled_unknown=停止结果未知`、`simulation=模拟运行`。
- Web 命令先使用 Node 24：`export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`。
- 所有任务在同一 PR #101 范围，但每个任务独立提交；不 push、不创建 PR，除非用户另行授权。
- 本期只交付 24h soak runbook，不实际执行 24h，不宣称长时可靠性已通过。
- 每个提交信息末尾保留 `Co-Authored-By: Claude Code <noreply@anthropic.com>`。

## Review Focus

1. **编辑 enabled Workflow**：普通编辑不得隐式停止未来 wake；Task 7 的 R30 矩阵固定新建/编辑/模板三种路径。
2. **恢复旧备份后外部动作已完成**：所有非终态 Effect 先 fencing，禁止重复设备动作；Task 5 的恢复 E2E 固定。
3. **simulation 误入生产**：dry-run 不得产生真实 Effect、Decision、wake、恢复项或 Main 协调；Task 13 固定组合根与 UI 双重隔离。
4. **旧 Runtime 深链接**：编码 ID、`view/device/at/event` 不得丢失；Task 11 固定 canonical/legacy 映射和真实路由回归。
5. **取消/停止失败**：停止请求失败不是物理停止证据，必须进入 `cancelled_unknown`；Task 5 与 Task 8 固定状态机和可见文案。

## File Structure

| Area | Files and responsibility |
|---|---|
| Shared values | `crates/core/src/harness.rs`：Workflow/Event/Effect/Decision/Command 的跨 crate 值类型 |
| Harness Core | `crates/agent/src/harness/{mod.rs,types.rs,reducer.rs,projection.rs}`：纯输入输出、状态迁移和投影，无 I/O |
| Workspace storage | `crates/db/src/harness_{workspace,event,workflow,effect,command,snapshot}.rs`：SQL、迁移、恢复与 CAS |
| Host | `apps/cloud/src/domains/agent/workflows/{mod.rs,supervisor.rs,runner.rs,dry_run.rs}`：调度、Main/Subagent、模拟组合根 |
| Runtime adapters | `apps/cloud/src/domains/agent/runtime/`：现有 Task/Effect/Job/SSE 适配与恢复接线 |
| Web API | `web/src/api/harness.ts`：唯一 Harness 请求入口，不在组件直接 fetch |
| Web routes | `web/src/ui/views/harness-url-state.ts`：canonical 与 legacy URL 映射 |
| Web UI | `web/src/ui/views/harness-{overview,workflow-edit,templates,dry-run,supervisor,compaction}.ts` 及同目录 CSS：单一组件集；Task 详情复用现有 Runtime 组件 |
| Templates | `apps/cloud/templates/workflows/{inspection,video,energy}.json`：版本化声明模板 |
| Verification | `apps/cloud/tests/harness_core_e2e_tests.rs`、`apps/cloud/tests/runtime_eval.rs`、`apps/cloud/benches/harness.rs`、`docs/runbooks/harness-soak.md` |

## Execution Order

按下列顺序执行，括号保留规格任务 ID：

1. T2 Core 边界 → T3 workspace DB → T4 事件 → T5 原子迁移 → T6 Effect/Decision → T20 Command。
2. T1 持续 Workflow/Main-Subagent → T7 控制与父子 drain → T8 Stream/投影 → T9 ToolDescriptor。
3. T11 Harness Workbench → T15 模板 → T16 dry-run → T17 Supervisor 健康 → T18 压缩检查器。
4. T10 全量恢复/权限回归 → T12 eval → T13 性能对照 → T14 旧路径清理 → T19 soak runbook。

每个任务必须先写失败测试、确认失败、最小实现、确认通过，再提交。不得把后续任务的接口提前实现成空壳。

---

### Task 1 (Spec T2): 收清 Agent Core 边界

**Files:**
- Move: `crates/agent/src/runtime/thing_agent/` → `apps/cloud/src/domains/agent/thing_agent/`
- Modify: `crates/agent/src/runtime/mod.rs`、`crates/agent/src/lib.rs`、`crates/agent/Cargo.toml`
- Modify: `apps/cloud/src/domains/agent/mod.rs`
- Test: `scripts/guards/selftest.sh`、`scripts/guards/agent-purity.sh`

**Interfaces:**
- Consumes: 现有 `AgentLoop`、`Tool`、`ModelProvider` 和 `TurnEvent` 端口。
- Produces: `tinyiothub_agent` 只暴露通用 runtime/port；thing-agent 应用类型由 `apps/cloud` 拥有，后续任务不得从 Core import 设备实现。

- [ ] **Step 1: 先写依赖与 import 失败测试**

在 `scripts/guards/selftest.sh` 增加一个临时 fixture，断言 agent crate 不含 `tinyiothub_runtime` 依赖，且 `crates/agent/src/runtime/thing_agent` 不再存在；现有 guard 仍必须通过自身正/反例。

- [ ] **Step 2: 运行 guard，确认当前边界失败**

Run: `scripts/guards/selftest.sh && scripts/guards/agent-purity.sh`
Expected: 新增 fixture FAIL，明确指出残留 thing-agent 或 runtime 依赖。

- [ ] **Step 3: 移动应用实现并保持现有行为**

只移动 thing-agent 模块、更新调用点和依赖，不改 Task/审批/设备语义；从 `crates/agent/Cargo.toml` 移除 `runtime` workspace 依赖，保留 rig `=0.42.0`。

- [ ] **Step 4: 运行边界与现有回归**

Run: `scripts/guards/selftest.sh && scripts/guards/agent-purity.sh && cargo test -p agent --lib && cargo test -p tinyiothub-cloud --lib`
Expected: 全部 PASS；无设备实现依赖残留。

- [ ] **Step 5: Commit**

```bash
git add crates/agent apps/cloud/src/domains/agent scripts/guards
git commit -m "refactor(agent): move thing application out of harness core" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 2 (Spec T3): 每 workspace 独立 Harness DB

**Files:**
- Create: `crates/db/src/harness_workspace.rs`
- Create: `crates/db/migrations/20261103000001_harness_workspace.sql`
- Modify: `crates/db/src/database.rs`、`crates/db/src/lib.rs`、`crates/db/src/pool.rs`
- Test: `crates/db/src/harness_workspace.rs` 内真实 SQLite 测试

**Interfaces:**
- Consumes: 平台 `workspaces` 注册表、现有 WAL/pool/migration 配置。
- Produces: `pub(crate) Db::workspace_pool(&self, workspace_id: &str) -> Result<SqlitePool>`、`Db::create_workspace_store(&self, workspace_id: &str) -> Result<()>`、`Db::close_workspace_store(&self, workspace_id: &str) -> Result<()>`；路径只能由注册表 ID 派生，raw pool 不暴露给 cloud。

- [ ] **Step 1: 写真实 SQLite 生命周期失败测试**

覆盖创建/重开、未知或路径穿越 ID 拒绝、单区迁移失败不阻塞另一区、关闭后访问按合同拒绝；断言文件位于 `data/agents/<workspace_id>/harness/harness.db`，WAL 和 foreign keys 开启。cross-tenant/non-manager 的 DB-open 前拒绝由 Task 16 的 HTTP 组合根测试覆盖。

- [ ] **Step 2: 确认测试失败**

Run: `cargo test -p db harness_workspace -- --nocapture`
Expected: FAIL，因为 `workspace_pool` 和独立 migrations 尚不存在。

- [ ] **Step 3: 实现最小 Db 内部门面**

在 `Db` 内缓存 workspace pools；创建时建目录/建库/迁移，迁移失败标该 workspace unavailable；pool 可缓存，`min_connections=0` 和 idle timeout 复用现有配置。不得在 cloud 打开 SQLx pool。

- [ ] **Step 4: 验证生命周期与故障隔离**

Run: `cargo test -p db harness_workspace -- --nocapture`
Expected: 全部 PASS，另一 workspace 在单区失败后仍可读写。

- [ ] **Step 5: Commit**

```bash
git add crates/db/src crates/db/migrations/20261103000001_harness_workspace.sql
git commit -m "feat(db): add isolated workspace harness stores" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 3 (Spec T4): 自包含 session_events 与 stamped facts

**Files:**
- Create: `crates/core/src/harness.rs`
- Create: `crates/db/src/harness_event.rs`
- Create: `crates/db/migrations/20261103000002_harness_events.sql`
- Modify: `crates/core/src/lib.rs`、`crates/db/src/lib.rs`
- Test: `crates/db/src/harness_event.rs` 内真实 SQLite 测试

**Interfaces:**
- Produces: `ExecutionMode::{Production,Simulation}`、`HarnessEvent`、`HarnessEventInput{event_id,workspace_id,session_id,execution_mode,run_id,task_id,event_type,occurred_at,cause_id,schema_v,payload_digest,payload_json}`、`DeviceFactsSnapshot{observed_at,declaration_hash,driver_version,facts}`。
- Produces: `EventFilter{session_id: Option<String>,run_id: Option<String>,task_id: Option<String>,execution_mode: ExecutionMode}`。
- Produces: `Db::append_session_event(&self, input: &HarnessEventInput) -> Result<HarnessEvent>` 和 `Db::session_event_page(&self, workspace_id: &str, filter: &EventFilter, after: i64, limit: i64) -> Result<EventPage>`，默认 256、最多预取一页由调用方保证。

- [ ] **Step 1: 写事件身份、隔离与快照失败测试**

断言相同 event_id+digest 返回原事件，相同 ID 不同 digest 返回 Conflict；生产查询不含 simulation；索引覆盖 `(execution_mode,session_id/run_id/task_id,seq)`；回放只使用事件内 stamped facts，不回查平台目录。

- [ ] **Step 2: 确认事件测试失败**

Run: `cargo test -p db harness_event -- --nocapture`
Expected: FAIL，因为 session_events/schema 和 append/page 接口未实现。

- [ ] **Step 3: 实现事件表、信封与分页**

写入字段严格使用 spec 的 `seq/event_id/session_id/execution_mode/run_id/task_id/type/occurred_at/cause_id/schema_v/payload_digest/payload_json`；空 thinking/text 不写，未知 schema 拒绝，媒体只存元数据/hash/evidence refs。

- [ ] **Step 4: 验证去重、索引与无目录回放**

Run: `cargo test -p db harness_event -- --nocapture`
Expected: 全部 PASS；`EXPLAIN QUERY PLAN` 使用复合索引，过滤空档返回 high-watermark。

- [ ] **Step 5: Commit**

```bash
git add crates/core/src crates/db/src crates/db/migrations/20261103000002_harness_events.sql
git commit -m "feat(harness): persist self-contained session events" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 4 (Spec T5): 纯 reducer 与统一事务入口

**Files:**
- Create: `crates/agent/src/harness/mod.rs`、`types.rs`、`reducer.rs`
- Modify: `crates/agent/src/lib.rs`
- Create: `crates/db/src/harness_effect.rs`
- Modify: `crates/db/src/database.rs`
- Test: `crates/agent/src/harness/reducer.rs`、`crates/db/src/harness_effect.rs`

**Interfaces:**
- Produces: `HarnessState`、`HarnessFact`、`Transition`、`TransitionPrecondition`。
- Produces: `reduce(state: &HarnessState, fact: &HarnessFact) -> Result<Transition, HarnessError>`，纯函数、无 I/O。
- Produces: `Db::apply_workspace_transition(&self, workspace_id: &str, expected: &TransitionPrecondition, transition: &Transition) -> Result<ApplyOutcome>`；`ApplyOutcome{transition_id,deduplicated,state_version}`；Effect/事件/Job 投影/wake 同事务。

- [ ] **Step 1: 写 reducer 矩阵与事务失败测试**

覆盖合法迁移、非法迁移、expected_version 冲突、迟到终态不覆盖、SQLite code 5/6 整事务重试、COMMIT 未知先按稳定 ID 去重。断言 reducer 不持有 Tokio/SQLx/provider。

- [ ] **Step 2: 确认矩阵失败**

Run: `cargo test -p agent harness::reducer && cargo test -p db harness_effect -- --nocapture`
Expected: FAIL，因为 reducer/apply 接口尚不存在。

- [ ] **Step 3: 实现 load→reduce→apply 所需纯类型与原子写入**

cloud 负责 load/reduce/apply 编排；db 在事务内重验前置状态。瞬态锁重试整个事务，不只重试 `BEGIN IMMEDIATE`；任何物理 dispatch 都在提交后由 Host 执行。

- [ ] **Step 4: 验证矩阵与提交边界**

Run: `cargo test -p agent harness::reducer && cargo test -p db harness_effect -- --nocapture`
Expected: 全部 PASS；事务内设备/provider 调用计数为零。

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src crates/db/src
git commit -m "feat(harness): add pure reducers and atomic transitions" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 5 (Spec T6): 统一 Effect、Decision、Job 投影与恢复

**Files:**
- Create: `crates/db/migrations/20261103000003_harness_effects.sql`
- Modify: `crates/db/src/harness_effect.rs`、`crates/db/src/device_job.rs`、`crates/db/src/task_approval.rs`
- Modify: `crates/agent/src/harness/{types.rs,reducer.rs}`
- Modify: `apps/cloud/src/domains/agent/runtime/{mod.rs,approvals.rs,controls.rs,admin.rs}`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`

**Interfaces:**
- Consumes: Task 4 的 `reduce`、`Transition`、`Db::apply_workspace_transition`。
- Produces: `EffectState` 九态、`DecisionKind::{ToolApproval,EffectReconciliation,Other}`、`EffectReconciler::settle(&self, intent: &EffectIntent) -> ReconcileOutcome`。
- Produces: 统一 recovery，覆盖 prepared/dispatching/cancelling/unknown/终态与 Decision 组合；`device_jobs` 不再独立迁移状态。Command 恢复由下一任务定义，本任务不提前依赖它。

- [ ] **Step 1: 写崩溃、竞态与旧备份恢复失败测试**

覆盖 dispatching 提交前/后、物理发送后、终态 wake 前 kill/restart；自动/人工对账两顺序；取消/发送两顺序；恢复旧备份后 prepared/approved 也进入 `unknown/recovery_required`。断言 `Failed` 只用于外部明确终止，停止失败映射 `cancelled_unknown`。

- [ ] **Step 2: 确认恢复矩阵失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests effect_recovery -- --nocapture`
Expected: FAIL，至少暴露 dispatching 盲重发或独立 Job/审批状态所有权。

- [ ] **Step 3: 实现唯一 Effect/Decision 状态机与对账**

字段严格使用 spec 的 effect identity/request_digest/idempotency_key/external_ref/state_version；公开事件只含脱敏摘要。`effect_reconciliation` 永不过期，人工 answered 先提交即获胜；错误自动对账只能启动带新 ID/key 的补偿 Workflow。

- [ ] **Step 4: 验证原子终态、恢复和权限**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests effect_recovery -- --nocapture && cargo test -p db harness_effect -- --nocapture`
Expected: 全部 PASS；重复物理动作计数为零，非终态 Effect 在对账前无 dispatch/consume。

- [ ] **Step 5: Commit**

```bash
git add crates/db crates/agent/src/harness apps/cloud/src/domains/agent/runtime apps/cloud/tests/harness_core_e2e_tests.rs
git commit -m "feat(harness): unify effects decisions and recovery" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 6 (Spec T20): 持久 Command 与逐类恢复

**Files:**
- Create: `crates/db/src/harness_command.rs`
- Create: `crates/db/migrations/20261103000004_harness_commands.sql`
- Modify: `crates/core/src/harness.rs`、`crates/agent/src/harness/reducer.rs`
- Modify: `apps/cloud/src/domains/agent/runtime/controls.rs`
- Test: `crates/db/src/harness_command.rs`、`apps/cloud/tests/harness_core_e2e_tests.rs`

**Interfaces:**
- Produces: `HarnessCommand{command_id,kind,target_task_id,target_run_id,actor,payload,created_at,expires_at,state}` 与 `CommandKind::{Steer,Interrupt,Pause,Cancel,Queue}`。
- Produces: `Db::consume_next_command(&self, target_task_id: &str, now: DateTime<Utc>) -> Result<Option<HarnessCommand>>`，按 `target_task_id+seq` 单头 FIFO，并与目标 Run/Task/wake/event 同事务。

- [ ] **Step 1: 写命令顺序、过期与恢复失败测试**

覆盖重复 command_id、过期、无效/终态目标、乱序 queue、重复 consume、重启后 requested command；interrupt 只 abort 当前模型调用，steer 仅在下一次模型调用前消费，cancel 将未消费命令标 superseded。

- [ ] **Step 2: 确认命令测试失败**

Run: `cargo test -p db harness_command -- --nocapture && cargo test -p tinyiothub-cloud --test harness_core_e2e_tests command_recovery -- --nocapture`
Expected: FAIL，因为 commands 表和原子 consume 尚不存在。

- [ ] **Step 3: 实现命令表与每类 consume**

索引使用 `(target_task_id,state,seq)`；每个目标每次事务只消费最早有效 Command。pause 保持 pending，恢复后只消费仍有效命令；不得在重启后重放陈旧 steer/interrupt。

- [ ] **Step 4: 验证 FIFO、幂等与恢复**

Run: `cargo test -p db harness_command -- --nocapture && cargo test -p tinyiothub-cloud --test harness_core_e2e_tests command_recovery -- --nocapture`
Expected: 全部 PASS；重复恢复不重复 abort、指导或入队。

- [ ] **Step 5: Commit**

```bash
git add crates/core/src crates/db crates/agent/src/harness apps/cloud/src/domains/agent/runtime/controls.rs apps/cloud/tests/harness_core_e2e_tests.rs
git commit -m "feat(harness): add durable ordered commands" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 7 (Spec T1): 持续 Workflow、Supervisor 与两层 Agent

**Files:**
- Create: `crates/db/src/harness_workflow.rs`
- Create: `crates/db/migrations/20261103000005_harness_workflows.sql`
- Create: `apps/cloud/src/domains/agent/workflows/mod.rs`、`supervisor.rs`、`runner.rs`
- Modify: `crates/core/src/harness.rs`、`crates/agent/src/harness/types.rs`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`

**Interfaces:**
- Produces: `WorkflowDefinition{objective,trigger,device_selector,source_selector,tool_ids,skill_refs,required_capabilities,approval_policy,evidence_policy}` 与不可变 `WorkflowRevision`。
- Produces: `WorkflowScheduler::run(self, shutdown: CancellationToken)`、`MainCoordinator::on_fact(&self, fact: HarnessFact) -> Result<()>`（async）、`SubagentRunner::run(&self, input: SubagentInput) -> Result<TaskOutcome>`（async）。
- Produces: Host 侧 `WorkflowService`，提供 `create(&self, input: WorkflowDefinition) -> Result<WorkflowRevision>`、`edit(&self, workflow_id: &str, expected_version: i64, input: WorkflowDefinition) -> Result<WorkflowRevision>`、`set_enabled(&self, workflow_id: &str, enabled: bool) -> Result<WorkflowRevision>`；后续模板和 UI 只使用这些入口。
- Produces: `supervisor:<workspace_id>` 与 `task:<task_id>/run:<run_id>` session 身份；Subagent 不共享 Main 或其他 Subagent 对话上下文。

- [ ] **Step 1: 写启动、幂等、misfire 与主子隔离失败测试**

启动自动加载 enabled workflows；来源键严格为 `workflow_id+trigger_id+slot/event_id`，不含 version；过期 cron 全跳过并写 `trigger.missed`，事件缺口写 `trigger.unavailable`；Main 每轮调用有界，Subagent 全新上下文并返回 `{status,summary,observations,evidence_refs,recommended_actions}`。

- [ ] **Step 2: 确认持续调度测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests workflow -- --nocapture`
Expected: FAIL，因为 workflow revisions、Supervisor 和 Main/Subagent runner 未实现。

- [ ] **Step 3: 实现声明调度与有限两层执行**

编辑追加 revision 并移动 current，不覆盖历史；首次 wake 固定 revision。R30=A：空白新建 disabled，普通编辑保留 enabled/disabled，模板直接 enabled；禁用只停止未来 wake。Host 空闲不调用 LLM，调度背压沿用现有 active-run/outbox 机制。

- [ ] **Step 4: 验证启动、重启、编辑和上下文隔离**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests workflow -- --nocapture`
Expected: 全部 PASS；编辑/回滚/重启不重复同一外部发生，enabled 编辑后仍产生后续计划 wake。

- [ ] **Step 5: Commit**

```bash
git add crates/core/src crates/db crates/agent/src/harness apps/cloud/src/domains/agent/workflows apps/cloud/tests/harness_core_e2e_tests.rs
git commit -m "feat(harness): run durable workflows with isolated subagents" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 8 (Spec T7): 版本固定、暂停/取消与父子 drain

**Files:**
- Modify: `apps/cloud/src/domains/agent/workflows/{mod.rs,runner.rs}`
- Modify: `apps/cloud/src/domains/agent/runtime/controls.rs`
- Modify: `crates/db/src/runtime_task.rs`、`crates/db/src/harness_effect.rs`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`

**Interfaces:**
- Consumes: Task 5 Effect/Decision 和 Task 6 Command。
- Produces: 父任务取消进入 `cancelling`，级联子任务并等待全部子任务及 owned effects 明确终态；禁止子任务再创建子任务。
- Produces: pause 只停止后续 Run/新 dispatch，不 abort 当前 token 或取消在途 Effect；resume 回到原等待关系。

- [ ] **Step 1: 写父子取消、暂停恢复与取消失败测试**

覆盖多子任务、子任务等待 approval/unknown、取消与 dispatch 两顺序、迟到 worker 结果、停止失败。断言任一 `cancelled_unknown` 使父 Task 以 `failed(cancel_unknown)` 结束，不显示「全部停止」。

- [ ] **Step 2: 确认控制测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests workflow_controls -- --nocapture`
Expected: FAIL，暴露父级提前终态或在途 Effect 被隐式取消。

- [ ] **Step 3: 实现最小父子 drain 与控制恢复**

Subagent 的 TaskOutcome、结果事件和 parent wake 同事务；取消只撤回 prepared 未发送 tool approval，reconciliation 保留到明确结果；dispatcher 提交后、发送前再次检查取消 token。

- [ ] **Step 4: 验证父子与控制矩阵**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests workflow_controls -- --nocapture`
Expected: 全部 PASS；终态不可被迟到结果覆盖或再次 wake。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent apps/cloud/tests/harness_core_e2e_tests.rs crates/db/src
git commit -m "feat(harness): preserve controls and drain child work" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 9 (Spec T8): Snapshot+tail 与有界 SSE

**Files:**
- Modify: `apps/cloud/src/domains/agent/runtime/stream.rs`、`projection.rs`
- Modify: `crates/db/src/harness_event.rs`
- Create: `crates/db/src/harness_snapshot.rs`
- Modify: `web/src/api/runtime-stream.ts`、`web/src/ui/views/runtime-projection.ts`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`、`web/src/api/runtime-stream.test.ts`、`web/src/ui/views/runtime-projection.test.ts`

**Interfaces:**
- Produces: `ProjectionSnapshot{projection_name,subject_id,last_seq,projection_version,state_version,state_json,updated_at}` 与 `ProjectionSubject{session_id,run_id,task_id,execution_mode}`。
- Produces: `Db::projection_tail(&self, subject: &ProjectionSubject, after: i64, limit: i64) -> Result<EventPage>`；Snapshot/Event/Resync 都返回 high-watermark。
- Produces: 实时与回放按 seq/event_id 去重，未知/未来/过期游标显式 Resync。

- [ ] **Step 1: 写快照尾部、游标和内存失败测试**

覆盖空 replay 页、过滤长空档、after=0、未知/未来游标、实时/回放竞争；crash 未闭合 text 标 `incomplete_tail=true` 和 `last_persisted_seq`。使用百万事件 fixture 的受限测试断言 replay 最多预取一页，不先把全部事件放进 VecDeque。

- [ ] **Step 2: 确认流与投影测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests stream -- --nocapture && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/api/runtime-stream.test.ts src/ui/views/runtime-projection.test.ts`
Expected: 至少现有 preload-all 或游标分支 FAIL。

- [ ] **Step 3: 实现增量投影与分页流出**

日常读取快照+尾部，不默认全历史扫描；SSE 每页 256 条，最多预取一页，实时使用有界广播和 lag Resync。旧 generation 响应/事件继续丢弃。

- [ ] **Step 4: 验证实时=回放与有界行为**

Run: 同 Step 2。
Expected: 全部 PASS；已知游标重连投影等于全量回放，无重复步骤。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/runtime crates/db/src web/src/api web/src/ui/views/runtime-projection.ts web/src/ui/views/runtime-projection.test.ts apps/cloud/tests/harness_core_e2e_tests.rs
git commit -m "feat(harness): stream bounded snapshot tails" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 10 (Spec T9): ToolDescriptor 单一事实来源

**Files:**
- Modify: `crates/skills/src/registry.rs`
- Modify: `crates/agent/src/tools/`、`crates/agent/src/prompt/constitution.rs`
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`
- Create: `web/src/types/harness.ts`
- Test: `apps/cloud/tests/runtime_eval.rs`、相关 agent/tools 单测

**Interfaces:**
- Produces: 扩展现有 `skills::registry::ToolDescriptor`，增加 `prompt_summary: String`、`effect_kind: ToolEffectKind`、`required_capabilities: Vec<Capability>`、`approval_policy: ApprovalPolicy`、`redaction: RedactionPolicy`、`presentation: PresentationDescriptor`；不新建第二份 descriptor。
- Produces: 注册表统一生成 LLM 工具子集、catalog API、策略门、脱敏参数摘要和通用前端展示；Workflow policy 只能收紧。

- [ ] **Step 1: 写声明驱动、能力与脱敏失败测试**

注册一个新测试工具，只改 descriptor，不断言还要修改 Core/前端事件名单；覆盖嵌套 password/token/secret、未知工具、非法参数、非视觉 provider、模板试图关闭强制审批。当前强制策略与固定 revision 不一致时失败关闭。

- [ ] **Step 2: 确认声明测试失败**

Run: `cargo test -p agent tools && cargo test -p tinyiothub-cloud --test runtime_eval tool_descriptor -- --nocapture`
Expected: FAIL，暴露硬编码清单、能力或嵌套脱敏缺口。

- [ ] **Step 3: 实现声明合成与统一暴露**

组合根从 provider/model 配置解析实际能力；视频缺失在模型调用前写 `capability.unavailable` 并零 Effect。request_json 仅供内部 dispatch/reconcile，公开 DTO/SSE 不包含它，UI 不使用 unsafeHTML。

- [ ] **Step 4: 验证声明、权限和 eval**

Run: 同 Step 2，并运行 `cargo test -p db task_approval`。
Expected: 全部 PASS；mock 结果不宣称 live 模型或真实设备质量。

- [ ] **Step 5: Commit**

```bash
git add crates/skills/src/registry.rs crates/agent/src apps/cloud/src/domains/agent/host apps/cloud/tests/runtime_eval.rs web/src/types/harness.ts
git commit -m "feat(harness): drive tools from declarations" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 11A (Spec T11): Canonical Harness 路由与异常优先首页

**Files:**
- Create: `web/src/api/harness.ts`、`web/src/types/harness.ts`
- Create: `web/src/ui/views/harness-url-state.ts`、`harness-overview.ts`、`harness-overview.css`
- Modify: `web/src/ui/app.ts`、`web/src/ui/views/runtime-url-state.ts`
- Test: `web/src/ui/views/harness-url-state.test.ts`、`harness-overview.test.ts`、`web/src/ui/app.test.ts`

**Interfaces:**
- Produces: `parseHarnessHash(hash: string) -> HarnessRoute | null`、`toHarnessTaskUrl(taskId: string, state: RuntimeTaskUrlState) -> string`、`runtimeToHarnessUrl(hash: string) -> string | null`。
- Produces: canonical `#/harness`、`#/harness/workflows/*`、`#/harness/tasks`、`#/harness/tasks/new`、`#/harness/tasks/{task_id}`；旧 list/new/task 重定向并保留编码 ID 与 `view/device/at/event`。
- Produces: 首页区域顺序「需要处理 → Workflow 与调度 → 当前 Main/Subagent」，唯一主行动「新建工作流」；同一组件/API，不复制 Task 状态。

- [ ] **Step 1: 写路由和首页层级失败测试**

```ts
expect(runtimeToHarnessUrl('#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z'))
  .toBe('#/harness/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z');
expect(runtimeToHarnessUrl('#/runtime')).toBe('#/harness/tasks');
expect(runtimeToHarnessUrl('#/runtime/new')).toBe('#/harness/tasks/new');
```

同时断言 list/new 映射、无效路由返回 null、异常先于配置和协调、异常分页/折叠，以及 Main 展开后子项可进入 Task 并返回父级。

- [ ] **Step 2: 确认路由与组件测试失败**

Run: `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/ui/views/harness-url-state.test.ts src/ui/views/harness-overview.test.ts src/ui/app.test.ts`
Expected: FAIL，因为 Harness parser、首页和路由尚未注册。

- [ ] **Step 3: 实现 canonical/legacy 映射与首页**

复用现有 Runtime Task 组件、SSE 和 API 写路径；旧 Runtime 导航项成为重定向入口。Main/Subagent 使用两层可展开列表，不画 DAG；子项显示独立 Task/Run、等待原因和最近结果，大量子项分页/增量加载。

- [ ] **Step 4: 验证深链接与异常闭环入口**

Run: 同 Step 2。
Expected: 全部 PASS；编码 ID、历史游标、设备选择和返回路径无丢失，旧 Runtime 不出现第二套写实现。

- [ ] **Step 5: Commit**

```bash
git add web/src/api/harness.ts web/src/types/harness.ts web/src/ui/app.ts web/src/ui/app.test.ts web/src/ui/views
git commit -m "feat(harness): add canonical navigation and exception overview" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 11B (Spec T11): Workflow 编辑与通用交互合同

**Files:**
- Create: `web/src/ui/views/harness-workflow-edit.ts`、`harness-workflow-edit.css`、`harness-status.ts`
- Modify: `web/src/ui/views/harness-overview.ts`、`harness-overview.css`
- Modify: `web/src/styles/` 中现有共享样式入口
- Test: `web/src/ui/views/harness-workflow-edit.test.ts`、`harness-accessibility.test.ts`

**Interfaces:**
- Consumes: Task 11A 路由和 Task 7 Workflow API。
- Produces: `HarnessStatusCopy` 固定中文映射、`workflowFieldErrors(definition: WorkflowDefinition, error: HarnessApiError): Record<string, string>`、统一就地 pending/success/error 区域。
- Produces: 单页分组「目标与设备 / 触发 / 工具与 Skills / 策略与证据 / 配置摘要」；空白新建 disabled，编辑保持原 enabled/disabled，模板例外由 Task 12 实现。

- [ ] **Step 1: 写编辑、冲突、反馈和可访问性失败测试**

覆盖可见 label、顶部摘要定位、未保存离开、stale-submit 保留本地编辑与手动重载、R30 enabled/disabled 保持、按钮 pending、持久结果、中文状态、双主题 token。另测三档 viewport、200%/400% zoom、44px、H1 焦点、tabs 方向键、dialog/菜单焦点归还和去重公告。

- [ ] **Step 2: 确认编辑与交互测试失败**

Run: `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/ui/views/harness-workflow-edit.test.ts src/ui/views/harness-accessibility.test.ts`
Expected: FAIL，因为编辑器和共享状态组件未实现。

- [ ] **Step 3: 实现最小编辑器与共享状态组件**

复用现有 design tokens、dialog/focus、Runtime 状态组件；冲突文案固定「配置已更新；重新加载后再试」，不提供 diff。<600px 使用带字段名记录列表和 key-value 元数据，无页面级横滚；不以 toast、颜色或假进度替代事实。

- [ ] **Step 4: 验证编辑、状态、响应式和读屏**

Run: 同 Step 2，再运行 `pnpm --dir web type-check`。
Expected: 全部 PASS；编辑 enabled Workflow 后仍 enabled，所有动态结果保留可读文本。

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views web/src/styles
git commit -m "feat(harness): add workflow editor and accessible states" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 12 (Spec T15): 三份模板与直接启用实例化

**Files:**
- Create: `apps/cloud/templates/workflows/inspection.json`、`video.json`、`energy.json`
- Modify: `crates/db/src/seed.rs`、`crates/db/src/harness_workflow.rs`
- Modify: `apps/cloud/src/domains/agent/workflows/mod.rs`
- Create: `web/src/ui/views/harness-templates.ts`、`harness-templates.css`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`、`web/src/ui/views/harness-templates.test.ts`

**Interfaces:**
- Consumes: `WorkflowDefinition`、统一引用/能力/策略校验和 Task 11 编辑器。
- Produces: `WorkflowService::instantiate_template(&self, template_id: &str, input: TemplateInput) -> Result<WorkflowRevision>`；`TemplateInput{workflow_id,device_selector,source_selector,approval_policy}`。有效输入创建 enabled，无效输入零创建。
- Produces: 空态主行动「从模板新建」；dry-run 为可选旁路，不是启用前置步骤。

- [ ] **Step 1: 写三模板实例化与首次旅程失败测试**

对 inspection/video/energy 分别校验设备、数据源、工具、Skills、审批和 evidence；视频模板必须要求 vision。断言成功后 enabled/version/下一计划 wake 可见，失败零配置/零调度，过期 slot 不补跑，策略不得放宽。

- [ ] **Step 2: 确认模板测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests workflow_templates -- --nocapture && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/ui/views/harness-templates.test.ts`
Expected: FAIL，因为模板文件和实例化入口不存在。

- [ ] **Step 3: 实现版本化 JSON 模板与实例化**

模板只包含统一 WorkflowDefinition 数据，不新增执行引擎。前端路径为模板选择 → 参数校验 → 可选 dry-run → 创建 enabled → 详情；未来触发只显示计划时间，通用导入仍不做。

- [ ] **Step 4: 验证模板、权限和首次配置**

Run: 同 Step 2。
Expected: 全部 PASS；直接启用是 DR4=B 的明确模板路径，不改变普通编辑 R30 合同。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/templates apps/cloud/src/domains/agent/workflows apps/cloud/tests/harness_core_e2e_tests.rs crates/db/src web/src/ui/views
git commit -m "feat(harness): add direct-enabled workflow templates" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 13 (Spec T16): 隔离 dry-run 与只读 simulation Run

**Files:**
- Create: `apps/cloud/src/domains/agent/workflows/dry_run.rs`
- Modify: `apps/cloud/src/domains/agent/workflows/{mod.rs,runner.rs}`
- Create: `web/src/ui/views/harness-dry-run.ts`、`harness-dry-run.css`
- Modify: `web/src/api/harness.ts`、`web/src/ui/views/harness-url-state.ts`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`、`web/src/ui/views/harness-dry-run.test.ts`

**Interfaces:**
- Produces: `DryRunInput{workflow_id,workflow_version,snapshot_seq}` 与 `DryRunResult{task_id,run_id}`；组合根不注册设备 worker/driver dispatcher。
- Produces: 独立 simulation Run 页面，持续 banner、只读通用 Run 投影、「返回此版本」；无真实执行/生产审批入口。

- [ ] **Step 1: 写 simulation 硬隔离与页面失败测试**

固定 revision 和调用方选择的 snapshot；snapshot 缺失/不可读响亮失败。断言 simulation 在创建 prepared Effect 前被拒绝，生产列表/恢复/Main 投影默认不含它，模拟 TaskOutcome 不触发后续生产 Task/Effect；能力错误与生产一致。

- [ ] **Step 2: 确认 dry-run 隔离测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests dry_run -- --nocapture && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/ui/views/harness-dry-run.test.ts`
Expected: FAIL，因为 simulation 组合根和只读页面未实现。

- [ ] **Step 3: 实现模拟组合根与独立结果页**

snapshot picker 显示 `seq/observed_at`；复用生产 reducer、ToolDescriptor 和通用投影，但 execution_mode 始终为 simulation。模拟历史不可变保留并默认隔离，不在本期删除。

- [ ] **Step 4: 验证零生产副作用与回程**

Run: 同 Step 2。
Expected: 全部 PASS；生产 wake/outbox/Effect/Decision/恢复任务计数不增加，页面持续标明「模拟运行」。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent/workflows apps/cloud/tests/harness_core_e2e_tests.rs web/src
git commit -m "feat(harness): add isolated workflow dry runs" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 14 (Spec T17): Supervisor 健康与就地恢复

**Files:**
- Create: `apps/cloud/src/domains/agent/workflows/health.rs`
- Create: `web/src/ui/views/harness-supervisor.ts`、`harness-supervisor.css`
- Modify: `apps/cloud/src/domains/agent/runtime/admin.rs`、`web/src/api/harness.ts`
- Create: `docs/runbooks/harness-recovery.md`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`、`web/src/ui/views/harness-supervisor.test.ts`

**Interfaces:**
- Produces: 单条可变 `supervisor_state{tick_seq,last_started_at,last_completed_at,last_error}`；正常 tick 不追加永久事件，仅 `stalled/recovered` 状态变化写事件。
- Produces: `SupervisorHealth::retry_dead_wake(&self, workspace_id: &str, event_id: &str, actor: &str) -> Result<bool>`，沿用原 event_id 幂等；逐条确认、行 pending/持久结果、无权限只读，无批量重试。

- [ ] **Step 1: 写健康判定和恢复失败测试**

覆盖无 enabled workflow 的正常空闲、下一 wake 很远、tick deadline、到期未开始、UTC 回拨、恢复清除；区分 scheduled 与实际执行、AI heartbeat 与进程健康。dead wake 显示 event_id/目标/原因，非管理角色无写动作。

- [ ] **Step 2: 确认 Supervisor 测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests supervisor_health -- --nocapture && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/ui/views/harness-supervisor.test.ts`
Expected: FAIL，因为持久健康状态和就地恢复未实现。

- [ ] **Step 3: 实现持久健康状态与逐条恢复**

进程内 monotonic clock 防瞬时回拨，持久 UTC 用于重启判定；显示下一计划 wake、oldest pending、调度延迟、活动 Main/Subagent 和最近错误。恢复反馈遵循 DR9，不使用假在线或假进度。

- [ ] **Step 4: 验证健康、权限与恢复幂等**

Run: 同 Step 2。
Expected: 全部 PASS；重复 retry 返回同一事实，其他 workspace 故障不被掩盖。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/src/domains/agent apps/cloud/tests/harness_core_e2e_tests.rs web/src docs/runbooks/harness-recovery.md
git commit -m "feat(harness): add supervisor health and recovery" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 15 (Spec T18): 固定边界压缩检查器

**Files:**
- Modify: `crates/db/src/harness_snapshot.rs`
- Create: `apps/cloud/src/domains/agent/workflows/compaction.rs`
- Create: `web/src/ui/views/harness-compaction.ts`、`harness-compaction.css`
- Modify: `web/src/api/harness.ts`
- Test: `apps/cloud/tests/harness_core_e2e_tests.rs`、`web/src/ui/views/harness-compaction.test.ts`

**Interfaces:**
- Produces: `ProjectionInspector::compare(&self, workspace_id: &str, projection_name: &str, subject_id: &str, last_seq: i64, projection_version: i64) -> Result<ProjectionComparison>`。
- Produces: `ProjectionInspector::replace_snapshot(&self, comparison: &ProjectionComparison, expected_last_seq: i64, expected_state_version: i64) -> Result<ProjectionSnapshot>`；只对同边界匹配开放 CAS 替换，零 dispatch。

- [ ] **Step 1: 写比较、并发 append 与 CAS 失败测试**

显示 seq 范围、projection_version、证据引用、canonical hash 和最后完整点；重建期间 append、hash mismatch、比较后 state_version 变化都保留最新快照。只有匹配时可点「替换投影快照」，冲突显示差异和「重新比较」，不自动重试。

- [ ] **Step 2: 确认压缩检查测试失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests compaction -- --nocapture && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web vitest run src/ui/views/harness-compaction.test.ts`
Expected: FAIL，因为固定边界比较和显式替换未实现。

- [ ] **Step 3: 实现临时投影比较与 CAS 替换**

canonical state/hash 使用同一 `projection_version+last_seq`；对 session facts、Task/Run、Effect/Decision 和外部系统只读。替换按钮沿用管理权限和 DR9 就地持久反馈。

- [ ] **Step 4: 验证快照新鲜度与零副作用**

Run: 同 Step 2。
Expected: 全部 PASS；旧快照不能覆盖新快照，Effect dispatch 计数为零。

- [ ] **Step 5: Commit**

```bash
git add crates/db/src/harness_snapshot.rs apps/cloud/src/domains/agent/workflows apps/cloud/tests/harness_core_e2e_tests.rs web/src
git commit -m "feat(harness): add verified compaction inspection" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 16 (Spec T10): 完整崩溃、竞态、权限与恢复回归

**Files:**
- Modify: `apps/cloud/tests/harness_core_e2e_tests.rs`
- Modify: `crates/db/src/harness_workspace.rs`、`harness_event.rs`、`harness_workflow.rs`、`harness_effect.rs`、`harness_command.rs`、`harness_snapshot.rs` 中测试模块
- Modify: `web/src/ui/views/harness-url-state.test.ts`、`harness-overview.test.ts`、`harness-workflow-edit.test.ts`、`harness-accessibility.test.ts`、`harness-templates.test.ts`、`harness-dry-run.test.ts`、`harness-supervisor.test.ts`、`harness-compaction.test.ts`、`web/src/api/runtime-stream.test.ts`
- Modify: 仅用于修复测试暴露问题的既有实现文件
- Test: 上述真实 SQLite、组合根 E2E 和前端测试

**Interfaces:**
- Consumes: Task 1-15 的全部公开合同。
- Produces: 完整恢复矩阵、实时=回放、嵌套脱敏、cross-tenant/non-manager 在 DB open 前拒绝、单 workspace restore 与其他区隔离的证据。

- [ ] **Step 1: 把 spec 测试计划转为逐项失败回归**

覆盖全部 Effect×Decision×Command 组合，未列组合写 `recovery.invalid_state` 并禁止 dispatch/consume；restore 使用 restore_journal、完整目录原子改名和完整性选择，覆盖 rename 前/中/后、journal 损坏、文件缺失。保留旧断言的行为边界，不以删除测试换绿。

- [ ] **Step 2: 运行新增回归并记录首个真实失败**

Run: `cargo test -p tinyiothub-cloud --test harness_core_e2e_tests -- --nocapture && cargo test -p db harness_ -- --nocapture`
Expected: 新矩阵中至少一个未覆盖分支 FAIL；记录具体状态组合，不修改断言来适配错误实现。

- [ ] **Step 3: 修复测试暴露的根因**

优先修统一 reducer/Db transition/组合根，不在每个 handler 加特例。修复涉及 JS 行为时添加精确回归；CSS-only 不新增无价值测试。

- [ ] **Step 4: 运行完整迁移后回归**

Run: `cargo test --workspace --lib --bins --tests && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web test`
Expected: 全部 PASS；新路径没有权限旁路。旧写路径与双审批清理由 Task 19 完成，本任务不提前宣称它们已删除。若范围超过本地验证选择器，记录 `/ship` pregate 仍需全量运行。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/tests crates/db crates/agent apps/cloud/src web/src
git commit -m "test(harness): verify crash race auth and restore matrix" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 17 (Spec T12): Scripted 与 opt-in live-provider eval

**Files:**
- Modify: `apps/cloud/tests/runtime_eval.rs`
- Modify: `evals/scenarios.json`、`evals/README.md`
- Test: `apps/cloud/tests/runtime_eval.rs`

**Interfaces:**
- Consumes: ToolDescriptor、Workflow runner、Effect/Decision 门。
- Produces: 同源 scripted 场景覆盖新工具、denied/unknown 不重复副作用、ACK 不代表物理完成、非视觉能力失败、TaskOutcome schema 和证据引用；live 使用相同场景并记录 provider/model/version。

- [ ] **Step 1: 写确定性 eval 失败场景**

为每个场景定义输入、预期工具/事件、禁止副作用和证据断言；mock 只验证编排，不宣称 live 模型质量。live 测试保持 ignored/opt-in，无授权配置时不运行。

- [ ] **Step 2: 确认 scripted 场景失败**

Run: `cargo test -p tinyiothub-cloud --test runtime_eval -- --nocapture`
Expected: 至少一个新场景 FAIL，并指出缺失的 provider/工具/效果边界。

- [ ] **Step 3: 修复 eval 暴露的根因并记录基线**

保持 prompt 确定性，记录 provider、模型、版本和 fixture；不得为通过 mock 而绕过 Effect/审批门。

- [ ] **Step 4: 验证 scripted，按授权选择 live**

Run: `cargo test -p tinyiothub-cloud --test runtime_eval -- --nocapture`
Expected: scripted PASS。只有用户已有授权配置时才运行 live ignored eval，并单独记录结果；否则明确写 `live=not run`。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/tests/runtime_eval.rs evals
git commit -m "test(harness): add workflow and effect provider evals" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 18 (Spec T13): main/branch 性能与规模对照

**Files:**
- Create: `apps/cloud/benches/harness.rs`
- Modify: `docs/runbooks/runtime-rollout.md`
- Create: `docs/runbooks/harness-performance.md`
- Test: `apps/cloud/benches/harness.rs` 的可复现测量脚本/入口

**Interfaces:**
- Consumes: workspace pools、session events、snapshot/rebuild、backup、SSE 实现。
- Produces: 原始结果包含机器/OS/SQLite/WAL、二进制与配置版本、数据规模、并发、样本分布、main/branch 交错 p50/p95；规模桶只是输入，不是配额或 SLA。

- [ ] **Step 1: 写性能测量与完整性失败检查**

固定 workspace 1/10/100/1000、事件 1k/100k/1M、非终态 Effect 空/大批量、最新/长空档/after=0 游标；断言输出原始样本和 main/branch 标签，缺任一元数据即报告不完整而非给一个总分。

- [ ] **Step 2: 确认当前无法产生所需证据**

Run: `cargo test -p tinyiothub-cloud --bench harness -- --nocapture`
Expected: FAIL 或明确 `missing benchmark`，不能复用一次热缓存耗时作为结论。

- [ ] **Step 3: 实现可复现 benchmark 与原始记录**

测量启动扫描、迁移/恢复、idle pools、replay 内存、日常 snapshot、离线 rebuild、备份写入延迟及其他 workspace 隔离；main 与分支使用同一 fixture 交错运行。

- [ ] **Step 4: 运行并保存对照**

Run: `cargo test -p tinyiothub-cloud --bench harness -- --nocapture`
Expected: 命令 PASS 且原始报告完整；报告实际 p50/p95/内存，不把本地结果称为生产 p99。

- [ ] **Step 5: Commit**

```bash
git add apps/cloud/benches docs/runbooks/harness-performance.md docs/runbooks/runtime-rollout.md
git commit -m "perf(harness): measure storage replay and recovery" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 19 (Spec T14): 清理旧写路径与硬编码注册

**Files:**
- Modify: `crates/db/src/runtime_event.rs`、`runtime_task.rs`、`task_approval.rs`、`device_job.rs`
- Modify: `apps/cloud/src/domains/agent/` 中旧 writer/审批/注册调用点
- Modify: `web/src/` 中旧事件名单与重复状态映射
- Modify: `AGENTS.md` 只更新实际改变的架构说明
- Modify: `scripts/guards/selftest.sh`，只添加旧 writer/注册点替代检查
- Test: 全量调用点搜索与现有回归

**Interfaces:**
- Consumes: 全部新 Harness 路径已经通过 Task 16。
- Produces: 每类事实只有一个 writer；旧 runtime.v1/hub_ledger 运行事件、独立 Job 状态迁移、双审批和手工工具/事件名单退出。既有无关 dead code 不删除。

- [ ] **Step 1: 写旧路径静态失败检查**

列出每个旧 writer/注册点及替代接口，断言运行时调用点不再命中；只移除本改造产生的孤儿 import/变量，不删除既有无关 dead code。

- [ ] **Step 2: 确认清理检查在当前调用点失败**

Run: `scripts/guards/selftest.sh && scripts/guards/sql-residence.sh && rg -n "runtime\.v1|task_approvals|autonomous_factory" crates apps/cloud web/src`
Expected: 新检查 FAIL 或列出尚未迁移的真实调用点；人工核对每个命中，不把注释/文档误报当代码。

- [ ] **Step 3: 删除或迁移已替代旧路径**

先迁移调用方和测试断言，再删除旧实现；`hub_ledger` 遥测本身保留，Chat 历史边界不变，X5 与 deferred 项不借机实现。

- [ ] **Step 4: 运行完整门禁**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --lib --bins --tests && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && pnpm --dir web lint && pnpm --dir web type-check && pnpm --dir web test && pnpm --dir web build`
Expected: 全部 PASS；若测试选择过宽，明确留给 `/ship` pregate，不声称已全绿。

- [ ] **Step 5: Commit**

```bash
git add crates/db apps/cloud/src web/src AGENTS.md scripts/guards
git commit -m "refactor(harness): retire superseded runtime write paths" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

### Task 20 (Spec T19): 24h soak 与故障演练 runbook

**Files:**
- Create: `docs/runbooks/harness-soak.md`
- Modify: `docs/runbooks/runtime-rollout.md`
- Test: 文档要求 marker 与原始证据模板

**Interfaces:**
- Consumes: 已实现指标、恢复、故障注入和性能报告格式。
- Produces: 可执行但本期不执行的 24h 方法：二进制/配置版本、完整起止时间、模拟设备/受控 provider、原始指标、故障时间线、期望/实际结果和失败判据。

- [ ] **Step 1: 写 runbook 完整性失败检查**

要求 scheduled/actual wake、Main/Subagent、LLM 错误/token/cost、Effect/unknown/reconcile、重复物理动作、恢复、压缩、SSE resync、DB/WAL、内存和 idle pools；包含锁、慢盘、断连、重启、重复结果、压缩和重建故障。

- [ ] **Step 2: 确认当前文档不满足检查**

Run: `rg -n "24h|duplicate|restore_journal|cancelled_unknown|actual|expected|not executed" docs/runbooks/harness-soak.md`
Expected: 文件缺失或检查 FAIL；不能把未来执行写成已完成。

- [ ] **Step 3: 编写 runbook 与原始证据模板**

明确任何重复设备动作、静默丢失、错误终态或无法重建都使 soak 失败；模拟结果不替代生产证据，本期只交方法，不设 merge/production soak 门。

- [ ] **Step 4: 验证文档合同且不执行 soak**

Run: 同 Step 2。
Expected: 所有必需 marker 存在，文档显式 `not executed in this change`；没有启动 24h 进程。

- [ ] **Step 5: Commit**

```bash
git add docs/runbooks/harness-soak.md docs/runbooks/runtime-rollout.md
git commit -m "docs(harness): add soak and fault injection runbook" -m "Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

## Self-Review Results

1. **Spec coverage:** T1–T20 全部有任务；T11 拆为 11A/11B，共 21 个任务块。自动检查确认 ID 无遗漏。
2. **Step scan:** 每个任务恰有 5 个 checkbox 步骤，共 105 步和 21 个独立提交；无 TBD、空泛“适当校验”或提前实现后续接口的步骤。
3. **Type consistency:** `WorkflowService`、`ProjectionInspector`、`HarnessEventInput`、`EventFilter`、`ProjectionSubject` 与 Web 函数签名已对齐；Effect 不提前依赖后续 Command，旧 Runtime list/new/task 均有 canonical 目标。
4. **Review Focus:** 五项风险分别由 Task 5、7、8、11、13 的测试覆盖；Task 16 只做全矩阵证据，不提前宣称 Task 19 的旧路径清理完成。
5. **Proportion:** 计划约 875 行，短于 2700 行规格；代码块仅用于精确测试断言和命令，没有粘贴实现主体。

## Final Acceptance

- 按顺序完成全部任务，不能只做 UI 或只让 mock 变绿。
- `cargo fmt --all -- --check`、Clippy、workspace tests、Web lint/type-check/test/build 均有实际结果。
- 真实 SQLite 和 kill/restart 证明不重复物理动作；前端实时=回放；17 条设计/路由路径均有通过的测试。
- 实现后另行运行真实浏览器 `/design-review`，验证独立 Harness、双主题、三档 viewport、键盘/读屏和 unknown/simulation 诚实状态。
- 只有用户另行授权后才 push、创建 PR 或部署；24h soak 仍是未执行的未来验证。

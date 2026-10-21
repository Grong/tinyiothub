# E2 Brain 接缝层 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地 E2——五接缝封闭契约与替换实现、固定设备工具集（7 工具 + 旧名隐藏别名）、turn 开头 6KB 快照注入、审批原因六分类、composite 编排原语框架。

**Architecture:** 接缝契约与决策类型住 `crates/agent`（纯逻辑、零 sqlx）；数据访问经 port/组合层注入。默认实现逐项保持 E2 前行为（回归锁）；替换证明用走真实 Runner/工具路径的测试实现。详细设计见 `docs/superpowers/specs/2026-10-21-e2-brain-seams-design.md`。

**Tech Stack:** Rust + tokio + sqlx(SQLite) + axum + rig；测试 `cargo test -p <crate>`。

**前置条件（硬阻塞）：** E1（watch + LiveState）已合入本分支（commit 52ad0d01 起）。

## Global Constraints

- agent crate 禁止 axum/sqlx/storage——接缝与快照类型零存储依赖
- 默认接缝实现**零行为变更**：25 次工具调用 / 300s / 单物 3 次 / gate_check 六步顺序 / 确认门 fail-closed
- 替换实现只能收窄：预算更低、策略更多拒绝、审批更多人工、ToolExec 恰好调用内层一次
- 快照硬上限 **6144 UTF-8 字节**；装配失败/超时→空快照，run 照常
- 模型可见设备工具恒为 7 个固定名；旧 9 工具保留为 `exposed()==false` 的隐藏别名
- composite 只支持顺序链（1..=10 步）；跨工作区引用一律拒绝
- 多租户：所有新表/工具/接口按 workspace_id 隔离
- 迁移文件名 `20261021000001_*.sql`，纯 SQL（ddl-only 守卫）
- 命令：`cargo test -p tinyiothub_agent` / `-p tinyiothub_runtime` / `-p tinyiothub_db` / `-p tinyiothub-policy` / `-p tinyiothub-cloud`

---

## Phase E2a — 五接缝

### Task 1: 接缝骨架 + 一致性测试辅助

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/seams/mod.rs`
- Test: 同文件 `#[cfg(test)]`

**Interfaces:**
- Produces:

```rust
pub mod approval;
pub mod budget;
pub mod policy;
pub mod tool_exec;

/// 链尾返回 None；替换实现返回被包装的内建。
pub trait Seam: Send + Sync {
    fn next(&self) -> Option<&dyn Seam> { None }
}

/// 组合根注入点；未替换 = 全部 Default。
#[derive(Clone)]
pub struct SeamRegistry {
    pub budget: Arc<dyn budget::BudgetSeam>,
    pub approval: Arc<dyn approval::ApprovalSeam>,
    pub policy: Arc<dyn policy::PolicySeam>,
    pub tool_exec: Arc<dyn tool_exec::ToolExecSeam>,
}
impl SeamRegistry { pub fn defaults() -> Self; }
```

- [ ] **Step 1: 写失败测试（契约一致性辅助）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_no_next_links() {
        let r = SeamRegistry::defaults();
        assert!(r.budget.next().is_none());
        assert!(r.approval.next().is_none());
        assert!(r.policy.next().is_none());
        assert!(r.tool_exec.next().is_none());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p tinyiothub_agent seams::tests` → 编译失败（模块不存在）

- [ ] **Step 3: 创建模块骨架**

`seams/mod.rs` 声明四个子模块与 `Seam`/`SeamRegistry`；四个子模块先各放类型占位（Task 2-5 填充）。每个 `*Seam` trait 继承 `Seam`。

- [ ] **Step 4: 运行测试通过**

Run: `cargo test -p tinyiothub_agent seams::tests`

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/runtime/thing_agent/seams crates/agent/src/runtime/thing_agent/mod.rs
git commit -m "feat(agent): E2 接缝骨架+组合根注入点（Seam/SeamRegistry）"
```

---

### Task 2: BudgetSeam —— 默认预算原样搬入 + 半预算替换证明

**Files:**
- Modify: `crates/agent/src/runtime/thing_agent/seams/budget.rs`
- Modify: `crates/agent/src/runtime/thing_agent/runner.rs`（常量 → DefaultBudget；Runner 持接缝）
- Modify: `crates/agent/src/runtime/thing_agent/manager.rs`（PipelineDeps.runner 构造点，如涉及）
- Test: `runner.rs` 既有测试 + `budget.rs` 新测试

**Interfaces:**
- Produces:

```rust
pub struct BudgetInput<'a> {
    pub workspace_id: &'a str,
    pub run_id: &'a str,
    pub tool_calls: u32,
    pub elapsed: Duration,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub per_thing_actions: &'a HashMap<String, u32>,
}
pub enum BudgetReason { ToolCalls, Duration, Tokens, PerThingActions }
pub enum BudgetDecision { Continue, Truncate(BudgetReason) }
pub trait BudgetSeam: Seam {
    fn check(&self, input: &BudgetInput<'_>) -> BudgetDecision;
}
pub struct DefaultBudget { pub max_tool_calls: u32, pub max_duration: Duration, pub max_tokens: Option<u64>, pub max_actions_per_thing: u32 }
impl Default for DefaultBudget { /* 25 / 300s / None / 3 */ }
```

- Runner 改动：`Runner { budget: Arc<dyn BudgetSeam> }`；`Runner::with_budget(n, d)` 构造 `DefaultBudget`；`consume_events` 在 ToolCall 事件与 deadline 处调用 `budget.check(...)`，映射到既有 `TruncationReason`（报告字段不变）。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn default_budget_matches_legacy_constants() {
    let b = DefaultBudget::default();
    assert_eq!(b.max_tool_calls, 25);
    assert_eq!(b.max_duration, Duration::from_secs(300));
    assert_eq!(b.max_actions_per_thing, 3);
    assert_eq!(b.max_tokens, None);
}

#[test]
fn half_budget_truncates_at_12_calls() { /* HalfBudget { inner: DefaultBudget, multiplier: 2 } 上限减半 */ }
```

Runner 回归测试：既有 `runner` 截断用例全绿，断言仍在第 26 次调用触发。

- [ ] **Step 2: 运行确认失败**（`cargo test -p tinyiothub_agent budget`）

- [ ] **Step 3: 实现 DefaultBudget + Runner 接线**

关键：Runner 不再直接读常量；`MAX_TOOL_CALLS_PER_RUN`/`MAX_RUN_DURATION` 保留为 `DefaultBudget::default()` 的取值来源（prompt 边界段仍引用常量，避免全仓改字面量）。

- [ ] **Step 4: 加替换证明测试**

`HalfBudget` 实现 `BudgetSeam`，`next()` 返回内建；真实 `Runner::execute` + ScriptedModel（复用 runner 既有桩）产出 13 次 ToolCall → 报告 `truncated == ToolCallBudget` 且 `tool_call_budget == 12`。

- [ ] **Step 5: 全量跑 agent 测试**

Run: `cargo test -p tinyiothub_agent`
Expected: 全绿（默认预算回归锁成立）

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(agent): E2 BudgetSeam——默认预算原样搬入+链式替换"
```

---

### Task 3: PolicySeam —— gate_check 原样包装

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/seams/policy.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/autonomous_invoke.rs:342`（改走接缝）
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`（构造接缝/注入工具）
- Test: `policy.rs` + `autonomous_invoke.rs`

**Interfaces:**
- Produces:

```rust
pub struct PolicyInput<'a> {
    pub workspace_id: &'a str,
    pub action_name: &'a str,
    pub actions_this_run: u32,
    pub actions_last_hour: u32,
    pub policy: Option<&'a AutonomyPolicy>,
}
pub enum PolicyDecision { Allow, Deny { code: String, message: String } }
pub trait PolicySeam: Seam { fn evaluate(&self, input: &PolicyInput<'_>) -> PolicyDecision; }
pub struct DefaultPolicy;
```

`DefaultPolicy::evaluate` = `gate_check` 结果映射：`Allow→Allow`，`Deny{reason}→Deny{code: reason, message: reason}`。

- [ ] **Step 1: 失败测试**：默认实现 6 个 gate_check 用例（no policy / mode≠Act / denied / not allowed / run cap / hourly fuse）逐项与 `gate_check` 结果相等；`next()` None。
- [ ] **Step 2: 实现 DefaultPolicy**
- [ ] **Step 3: 替换证明**：`DenyExtraAction` 包装 DefaultPolicy，额外拒绝 `open_valve`；断言真实工具路径（autonomous_invoke 测试）返回 deny 且 `code=="seam_extra_deny"`。
- [ ] **Step 4: 接线**：`AutonomousInvokeActionTool` 增加 `policy_seam: Arc<dyn PolicySeam>` 字段；第 342 行改 `policy_seam.evaluate(&PolicyInput{...})`；default 构造点注入 `DefaultPolicy`。
- [ ] **Step 5: 验证**：`cargo test -p tinyiothub_agent policy` + `cargo test -p tinyiothub-cloud autonomous_invoke`
- [ ] **Step 6: Commit**

```bash
git commit -am "feat(agent): E2 PolicySeam——gate_check 封闭契约+替换证明"
```

---

### Task 4: ApprovalSeam + 审批六分类类型

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/seams/approval.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/invoke_action.rs`（确认门改走接缝）
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`（工具上下文接缝注入）
- Test: `approval.rs` + `invoke_action.rs`

**Interfaces:**
- Produces:

```rust
pub enum ApprovalReason { AutoApproved, HumanApproved, HumanRejected, Unavailable, Expired, Cancelled }
impl ApprovalReason { pub fn executes(&self)->bool; pub fn is_denial(&self)->bool; pub fn code(&self)->&'static str; }

pub enum ApprovalDecision {
    Pending { token: String, message: String },
    Decided(ApprovalReason),
}
pub struct ApprovalInput<'a> {
    pub workspace_id: &'a str,
    pub tool_name: &'a str,
    pub args: &'a serde_json::Value,
    pub thing_id: Option<&'a str>,
    pub trigger: &'a str,
    pub require_confirm: bool,
    pub safety: ToolSafety,
}
pub trait ApprovalSeam: Seam { fn decide(&self, input: &ApprovalInput<'_>) -> ApprovalDecision; }
pub struct DefaultApproval;
```

- `DefaultApproval`：`require_confirm && (safety==Destructive||Write)` → `Pending`（token 由工具 mint，接缝只裁决）；否则 `AutoApproved`。
- `invoke_action`：`require_confirm` 读取保留（DB fail-closed 语义不变），把决策交给接缝；`Pending` 分支行为与现状完全一致（同 payload）。

- [ ] **Step 1: 失败测试**
  - `executes()` 仅 AutoApproved/HumanApproved；`is_denial()` 仅 HumanRejected。
  - DefaultApproval：只读→AutoApproved；Write+confirm→Pending；Write+无 confirm→AutoApproved；DB 读失败（require_confirm=true 兜底）→Pending。
- [ ] **Step 2: 实现类型 + Default**
- [ ] **Step 3: 替换证明**：`StrictApproval`（Write 一律 Pending，无视 require_confirm）；真实 `InvokeActionTool` 在 confirm 关闭的工作区仍返回 `confirmation_required`。
- [ ] **Step 4: 接线 invoke_action**
- [ ] **Step 5: 验证**：`cargo test -p tinyiothub_agent approval` + `cargo test -p tinyiothub-cloud invoke_action`
- [ ] **Step 6: Commit**

```bash
git commit -am "feat(agent): E2 ApprovalSeam+审批原因六分类类型"
```

---

### Task 5: ToolExecSeam + SeamTool 包装

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/seams/tool_exec.rs`
- Modify: `crates/agent/src/runtime/thing_agent/seams/mod.rs`（SeamRegistry.defaults）
- Modify: `crates/agent/src/tools/registry.rs`（`resolve_tools_for_agent` 末尾包装）
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`（`build_autonomous_tools` 末尾包装）
- Modify: `crates/agent/src/port/tool.rs`（`exposed()` 默认方法，Task 7 使用）
- Test: `tool_exec.rs` + `registry.rs`

**Interfaces:**
- Produces:

```rust
pub struct ToolInvocation { pub workspace_id: String, pub tool: Arc<dyn Tool>, pub args: serde_json::Value }

#[async_trait::async_trait]
pub trait ToolExecSeam: Seam {
    async fn execute(&self, inv: ToolInvocation) -> anyhow::Result<ToolResult>;
}
pub struct DefaultToolExec;      // inv.tool.execute(inv.args).await
pub struct SeamTool { inner: Arc<dyn Tool>, seam: Arc<dyn ToolExecSeam>, workspace_id: String }
impl Tool for SeamTool { /* execute → seam.execute(ToolInvocation{..}) */ }
```

- 顺序：SeamTool → 内层工具（policy/confirm/arbitration 不变）。
- 单物动作上限的**执行前检查**也放这里（Task 2 的 `max_actions_per_thing`）：`SeamTool` 持 `Arc<dyn BudgetSeam>` + 共享 action counts 读取器；超限返回 `ToolResult{success:false, error:"budget: per-thing action cap"}`，不调用内层。**注意**：runner 现有 action_counts 在 ToolCall 事件时累加（工具已派发），所以执行前检查读的是"本次调用前"的计数——测试需覆盖。

- [ ] **Step 1: 失败测试**：`AuditToolExec`（记录调用后委托）在 `resolve_tools_for_agent` 返回的工具上被执行一次，内层 execute 恰好一次；错误透传不变。
- [ ] **Step 2: 实现 trait + DefaultToolExec + SeamTool**
- [ ] **Step 3: 接线两处工具装配点**（chat registry / autonomous factory）
- [ ] **Step 4: 回归**：`cargo test -p tinyiothub_agent` + `cargo test -p tinyiothub-cloud tools`
- [ ] **Step 5: Commit**

```bash
git commit -am "feat(agent): E2 ToolExecSeam——统一工具执行外壳+单物动作预算"
```

---

### Task 6: Trigger 接缝契约固化 + 合成触发替换证明

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/seams/trigger.rs`（契约文档 + 常量）
- Modify: `docs/superpowers/specs/2026-10-21-e2-brain-seams-design.md`（如发现偏差，回写）
- Test: `crates/agent/src/runtime/thing_agent/manager.rs` 测试模块

**Interfaces:**
- Produces: `pub const DEDUP_PREFIXES: &[&str]`（thing/timer/heartbeat/watch 文档常量）；不新增 trait（复用 `Trigger`）。

- [ ] **Step 1: 失败测试**：`SyntheticTrigger`（发送 2 条固定 WakeSignal 后返回）挂到真实 scheduler，断言两次 run 被触发、dedup_key 原样。
- [ ] **Step 2: 实现测试触发源 + 契约注释**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub_agent trigger`
- [ ] **Step 4: Commit**

```bash
git commit -am "test(agent): E2 Trigger 接缝契约+合成触发替换证明"
```

---

## Phase E2b — 固定设备工具集

### Task 7: Tool.exposed + 旧名隐藏 + catalog 词表收敛

**Files:**
- Modify: `crates/agent/src/port/tool.rs`（`fn exposed(&self)->bool { true }`）
- Modify: `crates/agent/src/tools/catalog.rs`（`tool_label`/`tool_group` 表驱动 + 过滤 exposed）
- Modify: `crates/agent/src/adapters/rig/loop_.rs:234,257`（只把 exposed 工具给模型）
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`（旧 9 工具 `exposed()==false`——见 Task 10）
- Test: `catalog.rs` + `loop_.rs` 测试

**Interfaces:**
- `Tool::exposed()` 默认 true；`SeamTool` 透传内层。
- `catalog.rs` 新增：

```rust
pub struct ToolSpecEntry { pub name: &'static str, pub label: &'static str, pub group: &'static str, pub danger: bool, pub exposed: bool }
pub const BUILTIN_TOOL_SPECS: &[ToolSpecEntry];
```

`tool_label`/`tool_group` 从 `BUILTIN_TOOL_SPECS` 查找，fallback 到现状规则（MCP 平面名）。

- [ ] **Step 1: 失败测试**：rig preamble 的 `tool_specs` 与 dynamic tools 不含 `exposed()==false` 的工具；catalog 不含隐藏别名。
- [ ] **Step 2: 实现 + 接线**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub_agent tools catalog` + `cargo test -p tinyiothub-cloud catalog`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(agent): E2 工具暴露开关+catalog 词表单一事实源"
```

---

### Task 8: 固定设备读工具 list_devices / read_device

**Files:**
- Create: `apps/cloud/src/domains/agent/host/tools/thing/device_read.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`（注册 + `ThingToolContext` 接缝/快照依赖）
- Modify: `apps/cloud/src/domains/agent/host/tools/service.rs` 与 `autonomous_factory.rs`（新工具随 `create_thing_tools` 产出）
- Test: `device_read.rs`

**Interfaces:**
- `ListDevicesTool` name `list_devices`；`ReadDeviceTool` name `read_device`，args `{thing_id, properties?: [string]}`，输出：

```json
{"v":1,"thingId":"t1","name":"…","thingType":"device","health":"attention","zone":"园区A/3楼","online":true,
 "state":{"温度":"31.5"},"position":null,"text":"3 楼空调处于 Attention（离线边缘）…"}
```

- read_device 数据源：`ThingService::get_thing`（存在性/元数据）+ `LiveSummarySource::summary`（health/zone/values）；live 缺失时回退属性当前值；`properties` 给定则只回这些键（缺失键标 `null`）。

- [ ] **Step 1: 失败测试**：read_device 返回 state+health+zone+text；`properties` 过滤；物不存在返回工具错误；live 缺失回退缓存不炸。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 注册并验证**：`cargo test -p tinyiothub-cloud device_read`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(agent-tools): E2 list_devices/read_device 固定读工具"
```

---

### Task 9: set_device + stop（含 stop 闸门）

**Files:**
- Modify: `crates/agent/src/runtime/thing_agent/arbitration.rs`（stop 闸门 + force release）
- Create: `apps/cloud/src/domains/agent/host/tools/thing/device_write.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`
- Test: `arbitration.rs` + `device_write.rs`

**Interfaces:**
- `ArbitrationRegistry` 新增：

```rust
pub fn set_stop(&self, thing_id: &str, reason: String, ttl: Duration);
pub fn stop_reason(&self, thing_id: &str) -> Option<String>;  // 过期返回 None
pub fn force_release(&self, thing_id: &str);
```

- `SetDeviceTool` args `{thing_id, properties: {k:v}}`：物存在 → 每属性校验存在/非只读/非 derived → `Db::update_device_property_value` → 返回 `{v:1, thingId, updated:[k], status:"ok"}`。stop 闸门命中 → `stopped: <reason>`。
- `StopTool` args `{thing_id, reason?, ttl_secs?}`：`set_stop`（默认 60s）+ `force_release` + hub_ledger `job`（payload `{v:1,kind:"stop",thingId,reason}`）→ `{v:1,status:"stopped",until}`。
- `call_device`（原 invoke_action）执行前也检查 `stop_reason`。

- [ ] **Step 1: 失败测试**：stop 闸门 TTL 边界；set_device 成功后属性值变化；只读/派生属性拒绝；call_device 在 stop 期间拒绝、TTL 过后恢复。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub_agent arbitration` + `cargo test -p tinyiothub-cloud device_write`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(agent-tools): E2 set_device+stop（stop 闸门/仲裁释放）"
```

---

### Task 10: call_device / watch / unwatch + 旧名隐藏别名

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/invoke_action.rs`（name → `call_device`，保留旧结构）
- Create: `apps/cloud/src/domains/agent/host/tools/thing/watch_tools.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`（注册顺序 + 旧名 `exposed=false`）
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`（autonomous 变体改名 call_device）
- Test: `watch_tools.rs` + `autonomous_factory.rs`

**Interfaces:**
- `CallDeviceTool` name `call_device`，args `{thing_id, action, params?}`（内部字段与 invoke_action 相同，schema 用 `action`）。
- `WatchTool` args 二选一：`{thing_id, property, op, value, timeout_secs?}` / `{thing_id, level}` / `{thing_id, online}` → 构造 `WatchCondition` → 校验（引用物/上限 20）→ `WatchRuleStore::insert`（persistent=false）→ manager sink `WatchCommand::Upsert` → `{v:1,ruleId,expiresInSecs}`。
- `UnwatchTool` args `{rule_id}` → store delete + `WatchCommand::Remove` → `{v:1,status:"removed"}`。
- 旧 9 工具（`list_things/get_thing/get_thing_profile/read_property/invoke_action`）保持注册，`exposed()==false`；`create_thing_tools` 文档注释标明"隐藏别名，E3 删除"。

- [ ] **Step 1: 失败测试**：watch 工具写入真实 watch_rules 表并投递 Upsert；超上限拒绝；unwatch 删除行；模型工具列表无旧名、但 `load_all_tools` 仍能找到 `invoke_action`。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 批准通道回归**：`approve_and_execute` 用旧名 `invoke_action` 的存量提案仍可执行（既有测试改造）。
- [ ] **Step 4: 验证**：`cargo test -p tinyiothub-cloud thing watch`
- [ ] **Step 5: Commit**

```bash
git commit -am "feat(agent-tools): E2 call_device/watch/unwatch+旧名隐藏别名"
```

---

## Phase E2c — 快照注入

### Task 11: LiveState 工作区快照 + WatchCommand::Snapshot

**Files:**
- Create: `crates/runtime/src/watch/snapshot.rs`（`WorkspaceSnapshot`/`DeviceSnapshot` + `LiveState::workspace_snapshot` 辅助）
- Modify: `crates/runtime/src/watch/live_state.rs`（`pub fn summaries(&self) -> Vec<(String, ThingLiveSummary)>`）
- Modify: `crates/runtime/src/watch/mod.rs`
- Modify: `crates/agent/src/runtime/thing_agent/watch_loop.rs`（`WatchCommand::Snapshot` 分支 + `WatchEngine` 转发）
- Modify: `crates/runtime/src/watch/engine.rs`（`pub fn workspace_snapshot(&self)->WorkspaceSnapshot`）
- Test: `live_state.rs` + `watch_loop.rs`

**Interfaces:**
- `WorkspaceSnapshot { pub v: u8 /*1*/, pub devices: Vec<DeviceSnapshot>, pub total: u32 }`；`DeviceSnapshot { thing_id, name, health: Health, online, zone, values: Vec<(String,String)> }`。
- `WatchCommand::Snapshot { reply: oneshot::Sender<Option<WorkspaceSnapshot>> }`；engine loop 分支 `reply.send(engine.workspace_snapshot())`；engine 未运行/无实时态 → `Some(空快照)`（有 total）或 None（调用方都按空处理）。
- 排序：`Health` worst-first（Bad>Attention>Unknown>Ok），同级 zone、name。名称取 cache `get_display_name`。

- [ ] **Step 1: 失败测试**：三台不同 health 的物 → 快照顺序 bad→attention→ok；zone 正确；name 用显示名。
- [ ] **Step 2: 实现**
- [ ] **Step 3: watch_loop 命令往返测试**：sink 发 Snapshot → 收到 Some。
- [ ] **Step 4: 验证**：`cargo test -p tinyiothub_runtime watch` + `cargo test -p tinyiothub_agent watch_loop`
- [ ] **Step 5: Commit**

```bash
git commit -am "feat(runtime): E2 LiveState 工作区快照读面（WatchCommand::Snapshot）"
```

---

### Task 12: 快照渲染 + 6KB 预算 + 中文基线（T5）

**Files:**
- Create: `crates/agent/src/prompt/snapshot.rs`
- Modify: `crates/agent/src/prompt/mod.rs`（`pub mod snapshot;`）
- Modify: `crates/core/src/agent_runs.rs`（`SnapshotMetrics`）
- Test: `snapshot.rs`

**Interfaces:**
- `pub const SNAPSHOT_MAX_BYTES: usize = 6144;`
- `pub fn render_snapshot(ws: &WorkspaceSnapshot) -> (String, SnapshotMetrics)`
  - 每设备一行：`- {name}（{thing_id}）[{health}] {zone} {在线|离线} 关键属性: k=v, ...`
  - values 最多 8 个、单值 ≤40 字符；zone ≤120 字节；逐行累加字节，超 6144 停止并追加 `- … 另有 N 台设备未列出`。
  - 空设备 → 返回 `("", metrics{0})`，调用方整段省略。
- `SnapshotMetrics { bytes: u32, chars: u32, devices: u32, omitted: u32, est_tokens: u32 }`（core，`Serialize/Deserialize/PartialEq/Eq`），`est_tokens = ceil(chars / 1.5)`。
- 中文基线测试：构造 50 台（中文园区/楼栋/设备名 + health/zone/8 属性），断言 `bytes <= 6144`、`devices + omitted == 50`、省略行存在；把实测 bytes/chars/est_tokens 写入测试注释作为 T5 基线。

- [ ] **Step 1: 失败测试**（含基线）
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub_agent snapshot`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(agent): E2 快照渲染+6KB 预算+中文实测基线（T5）"
```

---

### Task 13: run_pipeline 快照装配注入 + RunReport 指标 + prompt 段

**Files:**
- Modify: `crates/agent/src/prompt/runtime.rs`（`build_prompt_with_snapshot`；`build_prompt` 委托 None）
- Modify: `crates/agent/src/runtime/thing_agent/manager.rs`（PipelineDeps.snapshot + run_pipeline 装配 + heartbeat prompt 追加）
- Modify: `crates/agent/src/runtime/thing_agent/runner.rs`（RunContextInner.snapshot → report）
- Modify: `crates/core/src/agent_runs.rs`（`RunReport.snapshot: Option<SnapshotMetrics>` `#[serde(default)]`）
- Modify: 所有 `RunReport { .. }` 构造点补 `snapshot: None`
- Test: `prompt/runtime.rs` + `manager.rs` + `runner.rs`

**Interfaces:**
- `PipelineDeps` 新增 `snapshot: Option<Arc<dyn WorkspaceSnapshotSource>>`：

```rust
#[async_trait::async_trait]
pub trait WorkspaceSnapshotSource: Send + Sync {
    async fn snapshot(&self, workspace_id: &str) -> Option<WorkspaceSnapshot>;
}
```

manager 内部实现 `ManagerSnapshotSource`（走 `watch_cmds` sink + 120ms `tokio::time::timeout`）；未接线 → None。
- `run_pipeline`：装配快照（fail-soft → None）→ `render_snapshot` → 记录 metrics 到 `ctx.inner.snapshot` → `build_prompt_with_snapshot(..., snapshot.as_deref())`；heartbeat 路径在专用 prompt 后追加同一段（非空时）。
- prompt 段：

```
\n\n工作区实时快照（v1，共 {total} 台，列出 {devices} 台）：\n<workspace_snapshot>\n{lines}\n</workspace_snapshot>
```

空字符串 → 不追加（既有精确快照测试全绿）。
- 指标：`Runner::assemble_report` 从 `RunContextInner.snapshot` 写入 `RunReport.snapshot`。

- [ ] **Step 1: 失败测试**
  - `build_prompt_with_snapshot(None)` == 原 `build_prompt` 输出（逐字节）；
  - Some 时快照段位于原则段后、触发段前且带围栏；
  - manager 测试：source 返回 Some → report.snapshot 有值；source 超时/None → report.snapshot None 且 run 照常。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 旧 JSON 兼容测试**：无 `snapshot` 字段的旧 report 反序列化 `None`。
- [ ] **Step 4: 验证**：`cargo test -p tinyiothub_agent prompt runner manager` + `cargo test -p tinyiothub_core agent_runs`
- [ ] **Step 5: Commit**

```bash
git commit -am "feat(agent): E2 turn 开头快照注入+RunReport 快照指标"
```

---

### Task 14: evals 快照基线用例

**Files:**
- Modify: `evals/scenarios.json`
- Modify: `evals/README.md`（如需要说明基线）

- [ ] **Step 1: 加快照注入场景**（3 条）：
  - `snapshot-01`：50 台设备快照 + "查 3 楼温度异常" → 期望不调用 `get_thing_profile`，直接 `read_device` 或作答；
  - `snapshot-02`：快照含 bad 设备 → 期望优先处理 bad；
  - `snapshot-03`：空快照 → 期望回退到逐物查询。
- [ ] **Step 2: 运行 eval 生成基线，记录对比结果**
- [ ] **Step 3: Commit**

```bash
git commit -am "test(evals): E2 快照注入基线场景"
```

---

## Phase E2d — 审批六分类落地

### Task 15: 审批端点/DB/前端六分类

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/handler/workspace_heartbeat.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/pending_action.rs`（过期/缺失区分）
- Modify: `crates/db/src/agent.rs`（提案状态词表注释/查询兼容）
- Modify: `web/src/api/memory.ts` + `web/src/ui/views/ai-ops.ts`（reason 展示）
- Test: `workspace_heartbeat.rs` + web vitest

**Interfaces:**
- 提案 content 增加 `decisionReason`（`auto_approved|human_approved|human_rejected|unavailable|expired|cancelled`）。
- DB status 词表扩展：`pending | approved | rejected | unavailable | expired | cancelled`；`flip_agent_proposal_status` 调用点按语义选值：
  - 工具不可执行/无 admin/审批通道不可用 → `unavailable`（**禁止写 rejected**）；
  - 审批窗口过期 → `expired`；
  - run 取消 → `cancelled`。
- approve/reject 响应新增 `reason`；`take_pending_action` 返回 `Result<PendingAction, TakeError{Expired, NotFound, WorkspaceMismatch}>`（WorkspaceMismatch 映射 Unavailable 且不执行）。
- 前端：`ai-ops.ts` 待审批卡片对 unavailable/expired/cancelled 显示"未执行"；`memory.ts` 状态联合类型扩展。

- [ ] **Step 1: 失败测试**：六分类各一例端点测试；Unavailable 后 DB status != rejected；过期 token 返回 Expired；前端渲染测试（unavailable 显示"未执行"）。
- [ ] **Step 2: 实现 DB/端点**
- [ ] **Step 3: 实现前端 + `npm test`（Node 24 PATH 见 memory）**
- [ ] **Step 4: 验证**：`cargo test -p tinyiothub-cloud workspace_heartbeat`；`cd web && npx vitest run src/ui/views`
- [ ] **Step 5: Commit**

```bash
git commit -am "feat(approval): E2 审批原因六分类（unavailable≠rejected）"
```

---

## Phase E2e — composite

### Task 16: 声明存储 + API + 类型校验

**Files:**
- Create: `crates/db/migrations/20261021000001_agent_composites.sql`
- Create: `crates/db/src/composite.rs`
- Modify: `crates/db/src/lib.rs`
- Create: `crates/agent/src/runtime/thing_agent/seams/composite.rs`（纯类型 + 校验 + `CompositeStore` trait）
- Create: `apps/cloud/src/domains/agent/host/handler/composite.rs`（API）
- Modify: `apps/cloud/src/domains/agent/host/handler/mod.rs`（nest 路由）
- Test: `db/src/composite.rs` + `seams/composite.rs` + API 测试

**Interfaces:**
- 表：

```sql
CREATE TABLE agent_composites (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    definition TEXT NOT NULL,   -- JSON（CompositeDef，v=1）
    created_at TEXT NOT NULL
);
CREATE INDEX idx_agent_composites_ws ON agent_composites(workspace_id);
```

- `CompositeDef { id, workspace_id, name, description, v: u8, steps: Vec<CompositeStep> }`（1..=10）；
- `CompositeStepOp::{Call{thing_id,action,params}, Set{thing_id,properties}}`；`CompositeStep { op, rollback: Option<Box<CompositeStepOp>> }`；
- `validate_composite(def) -> Result<(), String>`：非空/上限/名字非空/op 字段非空；
- `CompositeStore` trait：`list(ws) / get(ws,id) / insert(def) / delete(ws,id)`；cloud 适配器落 db。
- API：`GET/POST /workspaces/{id}/composites`、`DELETE /workspaces/{id}/composites/{cid}`（admin 校验 + workspace 隔离）。

- [ ] **Step 1: 失败测试**：迁移生效；round-trip；11 步拒绝；跨区 get 返回 None；API 400 原因。
- [ ] **Step 2: 实现**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub_db composite` + `cargo test -p tinyiothub_agent composite` + `cargo test -p tinyiothub-cloud composite`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(composite): E2 声明存储+校验+API"
```

---

### Task 17: run_composite / list_composites + 执行器（顺序/回滚/账本/审批）

**Files:**
- Create: `apps/cloud/src/domains/agent/host/composite.rs`（CompositeExecutor）
- Create: `apps/cloud/src/domains/agent/host/tools/thing/composite_tools.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`（注册）
- Modify: `apps/cloud/src/domains/agent/host/handler/workspace_heartbeat.rs`（批准通道特判 run_composite 整链执行）
- Test: `composite.rs` + `composite_tools.rs` + `workspace_heartbeat.rs`

**Interfaces:**
- `ListCompositesTool`：`{v:1, composites:[{id,name,description,stepCount}]}`。
- `RunCompositeTool` args `{composite_id}`：
  - 加载 + validate + 引用物存在性（本区）；
  - ApprovalSeam（`tool_name="run_composite"`, `thing_id=None`, args 含链摘要）→ Pending 时 mint token 返回 `confirmation_required`；AutoApproved 直接执行；
  - 执行见下。
- `CompositeExecutor::execute(def, workspace, run_id)`：
  ```
  for (i, step) in steps:
      acquire(thing_id, holder=format!("composite:{}:{}", def.name, run_id), TTL=120s)
      ledger job accepted → ToolExec(call_device/set_device 同路径) → ledger result/rejected
      失败：break
  回滚：已完成步骤逆序、有 rollback 的执行，ledger kind=job status="rolled_back"
  返回 {v:1, compositeId, status: completed|failed|partial|rolled_back, steps:[{index,status,output?,error?}], message}
  ```
  - 执行走真实工具（`CallDeviceTool`/`SetDeviceTool`），因此 ApprovalSeam/PolicySeam/仲裁语义一致；步骤内动作不再单独人工确认（整链已批）。
  - 跨区引用在 validate 后再次以 `find_thing_by_id(Some(ws))` 校验。
- 批准通道：`execute_approved_tool` 识别工具返回 `confirmation_required` 且 `compositeId` 存在 → 消费 token → `CompositeExecutor` 执行整链（不再二次 mint）；执行结果写 outcome，proposal status=approved。

- [ ] **Step 1: 失败测试**
  - 成功链：2 步全成功 → status=completed，ledger 有 accepted/result ×2；
  - 第 2 步失败 + 第 1 步有 rollback → status=rolled_back，rollback 执行且落账；
  - 无 rollback → status=partial；
  - busy 第一步 → status=failed，busy 文案带持有者；
  - 跨区 thing → 工具错误；
  - 批准通道：proposal(toolName=run_composite) → approve → 整链执行一次、proposal approved。
- [ ] **Step 2: 实现执行器与工具**
- [ ] **Step 3: 验证**：`cargo test -p tinyiothub-cloud composite`
- [ ] **Step 4: Commit**

```bash
git commit -am "feat(composite): E2 run_composite 顺序执行/回滚/逐步落账/整链审批"
```

---

## Phase Verify

### Task 18: 端到端 + 全量门禁 + 文档回写

**Files:**
- Test: `apps/cloud/src/tests/e2_brain_seams_e2e_tests.rs`（新建）
- Modify: `apps/cloud/src/tests/mod.rs`
- Modify: `docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md`（Implementation Tasks 勾选 T2/T5）
- Modify: `TODOS.md`（E2 结清项/局限：stop 在途不中断、旧别名 E3 删除）

- [ ] **Step 1: 端到端测试**
  - 模拟驱动上报 → watch 命中 → run 启动 → prompt 含快照段 → `call_device` 经五接缝（替换实现计数）→ 账本可回放（state_change/watch_hit/job）；
  - `storm_guard` composite：提案 → approve → 逐步 job 账本 → 第 2 步失败回滚。
- [ ] **Step 2: 全量门禁**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
（若 clippy 有既有基线问题，与 main 基线对比，只修本次新增）

- [ ] **Step 3: 前端**

```bash
cd web && export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && npm test -- --run
```

- [ ] **Step 4: 文档回写**：架构 spec Implementation Tasks 勾选；E2 退出标准逐条对照记录证据（commit/测试名）。

- [ ] **Step 5: Commit**

```bash
git commit -am "test(e2): 端到端+全量门禁+架构 spec 回写"
```

---

## 依赖关系

- Task 1 → 2/3/4/5（骨架先行）
- Task 5 依赖 Task 2（单物预算）
- Task 7 依赖 Task 5（SeamTool 透传 exposed）
- Task 8/9 依赖 Task 4/5（工具经接缝）
- Task 10 依赖 Task 7/8/9
- Task 12 依赖 Task 11；Task 13 依赖 12
- Task 15 依赖 Task 4
- Task 17 依赖 Task 16 + 8/9/10 + 4
- Task 18 依赖全部

## 风险与回滚

- 每个子阶段独立开关（mode 门控）：`fixed_tools_enabled` / `snapshot_enabled` / `composite_enabled`；接缝默认实现无行为变更，回滚=恢复直连调用（git revert 粒度）
- 旧工具别名保留一个版本，批准通道不因改名断链
- 快照装配 fail-soft；composite 失败显式 partial/rolled_back，不静默成功

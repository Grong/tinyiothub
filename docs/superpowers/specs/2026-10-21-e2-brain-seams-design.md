# E2 Brain 接缝层子 spec（五接缝 + 固定设备工具 + 快照注入 + 审批六分类 + composite）

- 日期：2026-10-21
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md`（P4/P5、E2 行、R1-3-1 工具映射、X3 composite、D5 快照预算）
- 前序：`docs/superpowers/specs/2026-10-20-e1-watch-hub-design.md`（E1a+b 已落地：LiveState/Watch 引擎/仲裁/账本/重织）
- 范围：完整 E2 阶段。内部子阶段 **E2a** 五接缝封闭契约与替换实现、**E2b** 固定设备工具集与 9→4 映射、**E2c** turn 开头快照注入（含中文预算基线）、**E2d** 审批原因六分类、**E2e** composite 编排原语框架。
- 状态：设计决策已与作者确认（2026-10-21：旧名隐藏别名 / 单工具 run_composite / 6KB UTF-8 字节上限）。

## 0. 承接收尾

| 来源 | 内容 | 本 spec 落点 |
|---|---|---|
| P4 | 预算/审批/门控/调度/触发是封闭契约的可替换段；替换器拿 `next()`，委托须原样返回 | §2 五接缝 |
| E2 行 | 五接缝 + watch/stop 工具 + 快照注入 + 审批六分类 + composite 框架 + 9→4 映射 | §2-§6 |
| R1-3-1 | 设备类 5 工具折叠为 list/read/call/set 形态；非设备类保留独立类别 | §3 |
| X3 | composite 框架；失败/部分成功/回滚语义以 E1 真实协同场景为设计输入 | §6 |
| D5 | 快照 token 计数留在 E2（中文预算实测） | §4 |
| E1 终审结转 | watch 注册事件与 job 生命周期落账延至 E2 | §4/§6 账本 |
| E1 终审结转 | E1d 成本上限挂 runner 硬预算旁，E2 接缝化后迁入 Budget 接缝 | §2.2 |
| F3 | runtime 持久化只走 ports；agent crate 零 sqlx | §2/§6 |

## 1. 组件与边界

```
crates/agent/src/runtime/thing_agent/seams/
  mod.rs        SeamRegistry + 链式替换规则 + 一致性测试辅助
  budget.rs     BudgetSeam（run 级预算决策）
  approval.rs   ApprovalSeam + ApprovalReason 六分类
  policy.rs     PolicySeam（包 autonomy gate_check）
  tool_exec.rs  ToolExecSeam + SeamTool 包装器
  trigger.rs    Trigger 接缝契约（复用既有 Trigger trait，补给契约文档/一致性测试）

crates/agent/src/prompt/
  runtime.rs    build_prompt 增加快照段（空快照整段省略）
  snapshot.rs   新增：WorkspaceSnapshot/DeviceSnapshot 渲染 + 6KB 字节预算

crates/agent/src/runtime/thing_agent/watch_loop.rs
  WatchCommand::Snapshot 变体（engine actor 内读 LiveState，单写者不破）

apps/cloud/src/domains/agent/host/tools/thing/
  device.rs / fixed.rs   固定设备工具 list_devices/read_device/call_device/set_device/stop/watch/unwatch
  legacy 旧 9 工具保留为隐藏别名（exposed=false）
  composite.rs           run_composite/list_composites 工具（组合层执行器）

apps/cloud/src/domains/agent/host/composite.rs
  CompositeExecutor：逐步执行/仲裁/回滚/账本

crates/db/migrations/
  agent_composites 表（声明式链的持久源）

apps/cloud/src/domains/agent/host/handler/
  watch.rs / workspace_heartbeat.rs / composite.rs  审批六分类落点
```

边界规则：

1. **agent crate 纯逻辑**：接缝契约、决策类型、快照渲染、composite 声明类型都在 `crates/agent`，零 sqlx；数据访问经 port/句柄注入。
2. **接缝替换是链式中间件**：每个接缝的替换实现持有 `next: Arc<dyn Seam>`（内置实现）；只能**收窄**决策，不能放宽（预算只能更低、策略只能更多拒绝、审批只能更多人工、ToolExec 只能包装且必须恰好调用一次内层、Trigger 不得伪造 workspace_id/priority 语义）。
3. **默认零行为变更**：E2a 把现有硬编码决策点搬进 Default 实现，常量与顺序逐项保持不变；回归测试锁住（Runner 25 次/300s、gate_check 六步顺序、确认门 fail-closed）。
4. **工具列表稳定**：模型可见设备工具恒为 7 个固定名；设备自有 action 是 `call_device` 的参数。
5. **跨层契约版本化（P5）**：工具输出与快照段带 `v=1`；未知字段忽略。内部接缝 trait 不做版本字段（编译期契约）。
6. **多租户**：全部接缝输入带 workspace_id；composite/watch/stop 只能引用本区物。

## 2. 五接缝封闭契约（E2a）

### 2.0 通用替换规则

```rust
pub trait Seam: Send + Sync {
    /// 内置实现（链尾）返回 None；替换实现返回被包装的内建。
    fn next(&self) -> Option<&dyn Seam> { None }
}
```

规则（spec 级约束，每接缝一致性测试强制）：

- **R-1 委托原样**：替换实现选择委托时，返回值必须等于 `next()` 的返回值，不得改写字段/原因码。
- **R-2 只收窄**：替换实现只能把 `Allow` 变 `Deny`/更低预算/更多人工审批；反向（`Deny→Allow`、放宽预算、跳人工）不允许。
- **R-3 单次调用**：ToolExec 替换器必须恰好调用一次内层执行。
- **R-4 无状态泄漏**：接缝实现不得跨 workspace/run 保留决策状态（指标除外）。

`SeamRegistry` 持有五个默认实现，组合根可整体替换；未替换时行为与 E2 前一致。
每接缝提供 `RecordingSeam` 与 `NarrowingSeam` 两个测试替换实现，作为**真实替换证明**（走真实 Runner/Tool 调用路径，不是 mock 掉核心）。

### 2.1 Trigger 接缝

现状：`crates/agent/src/runtime/thing_agent/trigger/mod.rs:20` 的 `Trigger` trait 已是扩展点，四个真实实现（Heartbeat/Timer/ThingEvent/Watch）。

契约固化（不新增 trait）：

- 输出只允许 `WakeSignal`；`workspace_id` 必须来自构造参数；`Priority` 只影响调度语义，不得用于绕过限速/预算（Critical 的既有豁免保留）。
- `dedup_key` 命名空间：`thing:{id}:event:{name}` / `timer:{ws}` / `heartbeat:{ws}[:{thing}:{alarm}]` / `watch:{rule_id}`；新触发源必须使用自己的前缀。
- 新增触发源 = 新增一个 `Trigger` 实现并在 `PipelineDeps` 挂载，不改 scheduler/runner。
- 替换证明：测试里新增 `SyntheticTrigger`（固定序列信号）挂到真实 scheduler，断言 run 被唤醒。

### 2.2 Budget 接缝

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

pub enum BudgetDecision {
    Continue,
    Truncate(BudgetReason),   // ToolCalls | Duration | Tokens | PerThingActions
}

pub trait BudgetSeam: Seam {
    fn check(&self, input: &BudgetInput<'_>) -> BudgetDecision;
}
```

- `DefaultBudget`：25 次工具调用 / 300s（`runner.rs:27-29` 常量原样）；token 上限 `None`（E1d 迁入后设置）；单物动作上限 3 次。
- 迁移：`Runner` 持 `Arc<dyn BudgetSeam>`；`Runner::with_budget(n, d)` 构造一个参数化 `DefaultBudget`，测试 API 不变。runner 的 `consume_events` 在 ToolCall/时间边界调用接缝，映射到既有 `TruncationReason`（ToolCallBudget/DurationBudget）——既有报告字段不变。
- 单物动作上限由 runner 的 `action_counts` 喂给接缝；执行前检查放在 ToolExec 包装器（§2.5）里，`Truncate(PerThingActions)` 以工具错误 `budget: per-thing action cap` 返回。
- 替换证明：`HalfBudget` 测试实现（上限减半）在真实 Runner 上触发截断。

### 2.3 Approval 接缝

```rust
pub enum ApprovalReason {
    AutoApproved,   // 策略/信任自动通过（无人工）
    HumanApproved,  // 人工批准
    HumanRejected,  // 人工明确否决
    Unavailable,    // 无人可问 / 审批通道不可用——不是否决，且不执行
    Expired,        // 审批窗口过期——不是否决，且不执行
    Cancelled,      // run/会话取消或已被替代——不是否决，且不执行
}

impl ApprovalReason {
    pub fn executes(&self) -> bool;   // 仅前两者 true
    pub fn is_denial(&self) -> bool;  // 仅 HumanRejected true
}

pub enum ApprovalDecision {
    Pending { token: String, message: String },  // 状态：等待人工
    Decided(ApprovalReason),
}

pub struct ApprovalInput<'a> {
    pub workspace_id: &'a str,
    pub tool_name: &'a str,
    pub args: &'a serde_json::Value,
    pub thing_id: Option<&'a str>,
    pub trigger: &'a str,
}

pub trait ApprovalSeam: Seam {
    fn decide(&self, input: &ApprovalInput<'_>) -> ApprovalDecision;
}
```

- `DefaultApproval`：`require_action_confirm=true` 且工具分类为 Destructive/Write → `Pending`（mint token，行为不变）；只读 → `AutoApproved`；`require_action_confirm=false` → `AutoApproved`。
- **Unavailable ≠ Rejected**：批准端点无 admin/无审批通道时返回 `Unavailable`（提案保持 `pending` 或转 `unavailable`，但绝不写 `rejected`）；前端/审计必须显示"未执行（无人可问）"。
- 状态落点：提案内容新增 `decisionReason`；DB status 词表扩展 `approved | rejected | unavailable | expired | cancelled | pending`；`flip_agent_proposal_status` 的条件翻转仍以 `pending` 为源。
- 替换证明：`StrictApproval`（把所有 Write 变 `Pending`，即使 confirm 关闭）走真实 invoke_action 工具。

### 2.4 Policy 接缝

```rust
pub struct PolicyInput<'a> {
    pub workspace_id: &'a str,
    pub action_name: &'a str,
    pub actions_this_run: u32,
    pub actions_last_hour: u32,
    pub policy: Option<&'a AutonomyPolicy>,
}

pub enum PolicyDecision {
    Allow,
    Deny { code: String, message: String },  // code = gate_check 既有原因串
}

pub trait PolicySeam: Seam {
    fn evaluate(&self, input: &PolicyInput<'_>) -> PolicyDecision;
}
```

- `DefaultPolicy` = `tinyiothub_policy::autonomy::gate_check` 六步顺序原样（no policy/mode≠Act → denied_actions → allowlist → run cap → hourly fuse → Allow）。
- 调用点：`autonomous_invoke.rs:342` 直连 gate_check 改为经接缝；chat invoke_action 不经 Policy（信任门在 TrustAwareTool，保持现状）。
- 替换证明：`DenyExtraAction`（额外拒绝 `open_valve`）在真工具路径生效。

### 2.5 ToolExec 接缝

```rust
pub struct ToolInvocation {
    pub workspace_id: String,
    pub tool: Arc<dyn Tool>,
    pub args: serde_json::Value,
}

#[async_trait]
pub trait ToolExecSeam: Seam {
    async fn execute(&self, inv: ToolInvocation) -> anyhow::Result<ToolResult>;
}
```

- `DefaultToolExec`：调用 `inv.tool.execute(inv.args)`（既有工具语义原样）。
- `SeamTool`：包装每个解析出的工具（chat 的 `ToolRegistry::resolve_tools_for_agent`、自治的 `build_autonomous_tools`），把 `Tool::execute` 路由到 `ToolExecSeam`。仲裁/确认/策略仍在其内层工具里，接缝是它们的统一外壳（顺序：SeamTool → 内层工具 → policy/confirm → arbitration → dispatch）。
- 单物动作上限的执行前检查也在此层（读 `RunContextInner.action_counts`）：超限直接返回工具错误，不进入 dispatch。
- 替换证明：`AuditToolExec`（记录后原样委托）在真实 agent loop 工具路径上被调用，且 `RecordingSeam` 断言内层只被调用一次。

## 3. 固定设备工具集与 9→4 映射（E2b）

### 3.1 模型可见工具（固定）

| 工具 | 参数 | 行为 |
|---|---|---|
| `list_devices` | `{parent_id?, thing_type?, status?, limit?, offset?}` | 本区物分页列表（复用 list.rs 语义） |
| `read_device` | `{thing_id, properties?}` | 一次返回 state（属性值）+ health + zone + online + 一句 `text` 摘要；`properties` 缺省=全部已上报值。live 摘要来自 LiveState，缺实时态时回退缓存 |
| `call_device` | `{thing_id, action, params?}` | 执行设备 action（原 invoke_action 语义 + 确认门 + 仲裁） |
| `set_device` | `{thing_id, properties: {k:v}}` | 写属性目标值（复用属性写入服务；只读属性拒绝） |
| `stop` | `{thing_id, reason?}` | 置该设备 stop 闸门（默认 TTL 60s，期间 call/set 拒绝 `stopped: <reason>`）；释放该设备仲裁租约；落账 `job`（kind=stop）。**在途单次调用不可中断**（记为局限，E4 声明制再细化） |
| `watch` | `{thing_id, property, op, value, timeout_secs?}` 或 `{thing_id, level}` / `{thing_id, online}` | 注册**会话临时** one-shot watch（`persistent=false`，重启失效——R2-1-2）；返回 `rule_id`；超时默认 600s、上限 3600s |
| `unwatch` | `{rule_id}` | 删除会话 watch（DB + 注册表） |

非设备工具保留原类别（不进设备工具集）：`query_events`、`search_knowledge`、`read_document`、`get_thing_tree`。

### 3.2 旧名隐藏别名

- 旧工具 `list_things / get_thing / get_thing_profile / read_property / invoke_action` **继续注册**（批准通道可解析存量提案），但 `Tool::exposed()` 返回 `false`。
- `Tool` trait 增加默认方法 `fn exposed(&self) -> bool { true }`；`rig_loop_factory` 只把 `exposed()` 为真的工具交给模型；catalog 构建同样过滤。
- 新工具是独立实现（新名、新 schema、固定形态），旧实现不改语义——避免"改名顺带改行为"。
- 一个版本后（E3 起点）删除旧别名，并在发布说明标注。

### 3.3 catalog 单一事实源

`crates/agent/src/tools/catalog.rs` 改为表驱动：`ToolSpecEntry { name, label, group, danger, exposed }` 单一数组，`build_catalog`、`tool_group`、`tool_label` 与动态/静态兜底全部从它派生；新增测试断言"注册的工具名 ⊆ catalog 词表"与"catalog 不含未注册名"。

## 4. Turn 开头快照注入（E2c + T5）

### 4.1 装配

```
run_pipeline（每个 run 一次）
  → WorkspaceSnapshotSource（120ms 超时，fail-soft）
  → WatchCommand::Snapshot（engine actor 内读 LiveState + cache；单写者不破）
  → WorkspaceSnapshot { devices: [...], total, omitted }
  → render_snapshot（UTF-8 字节预算 6144）
  → build_prompt(..., snapshot: Option<&str>)
```

`WorkspaceSnapshot`/`DeviceSnapshot` 类型住 `crates/agent/src/prompt/snapshot.rs`（纯类型 + 渲染），数据由 cloud 侧 port 注入（照 `LiveSummarySource`/`LiveSummarySlot` 的迟绑定先例）。

### 4.2 内容与排序

每设备一行：`- {name}（{thing_id}）[{health}] {zone} 在线/离线 关键属性: k=v, ...`
- 排序：health worst-first（bad → attention → unknown → ok），同级按 zone、name。
- 只列最近有实时态的物（LiveState 投影）；`omitted = total_in_workspace - rendered`。
- 段格式（围栏 + 版本）：`\n\n工作区实时快照（v1，共 N 台，列出 M 台）：\n<workspace_snapshot>\n...\n</workspace_snapshot>`

### 4.3 预算（D5 / T5）

- 硬上限 **6144 UTF-8 字节**（AGH ≤6KB 参照），按行组装，超限即停并追加 `- … 另有 N 台设备未列出`。
- 单设备行内 values 截断：最多 8 个属性、单值 ≤40 字符；zone ≤120 字节。
- 空快照（无实时态/装配失败）→ **整段省略**（既有 prompt 精确快照测试不受影响）。
- **实测基线**：单测构造中文工作区（园区/楼栋/设备中文名，50 台，含 health/zone/属性），断言 ≤6144 字节并落基线用例（记录 bytes/chars/设备数）。token 估算 `est_tokens = ceil(chars / 1.5)`（中文 UTF-8 3 字节/字，按保守比例；报告只作观测，不作硬闸门）。
- 指标/溯源：`RunReport.snapshot: Option<SnapshotMetrics { bytes, chars, devices, omitted, est_tokens }>`（`#[serde(default)]` 向后兼容）；装配耗时 >120ms 或失败 → `None` + warn 日志，run 照常（hook never throws 同款）。

### 4.4 Prompt 注入点

快照段插在角色/原则段之后、触发段之前（"先看世界，再看发生了什么"）。`build_prompt` 新增参数 `snapshot: Option<&str>`；调用方（`run_pipeline`）传渲染结果。heartbeat 专用 prompt 也追加同一段（同一 run_pipeline 装配）。

## 5. 审批原因六分类（E2d）

见 §2.3 类型定义。落地点：

| 场景 | 现状 | E2 |
|---|---|---|
| chat `invoke_action` 确认门 | `require_action_confirm` → return `confirmation_required` + token | 经 ApprovalSeam；结果同前，但带 `reason` |
| token 确认端点 | 消费 token 执行/未找到报错 | 批准→HumanApproved；过期→Expired；workspace 不匹配→Unavailable（不是 rejected） |
| 心跳提案 approve | 条件翻转 `pending→approved`，工具不可执行时自动 `rejected` | 工具不可执行→`unavailable`（未执行，不是否决）；无 admin→403 + `unavailable` 审计 |
| 心跳提案 reject | `pending→rejected` + reason | HumanRejected |
| run 取消/被新 run 替代 | 无显式处理 | Cancelled（审批队列中的提案在取消时落此原因） |
| 自动批准路径（heartbeat 批准即授权） | 直接执行 | HumanApproved（人工批准是授权来源） |

- API 响应统一新增 `reason` 字段（`approve`/`reject`/`take_pending_action`）。
- 提案内容新增 `decisionReason`；DB status 扩展词表；前端 `#/ai` 待审批卡片按 reason 显示（unavailable/expired/cancelled 显示"未执行"而非"已拒绝"）。
- 测试矩阵：六类各一例（工具层 + 端点层），并断言 `Unavailable` 不写 `rejected`。

## 6. composite 编排原语框架（E2e）

### 6.1 声明

```rust
pub struct CompositeDef {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub description: String,
    pub v: u8,                      // 契约版本 = 1
    pub steps: Vec<CompositeStep>,  // 1..=10
}

pub enum CompositeStepOp {
    Call { thing_id: String, action: String, params: Value },
    Set  { thing_id: String, properties: Value },
}

pub struct CompositeStep {
    pub op: CompositeStepOp,
    /// 回滚动作（可选）；已完成步骤在链失败时逆序执行。
    pub rollback: Option<Box<CompositeStepOp>>,
}
```

持久源：新表 `agent_composites(id, workspace_id, name, description, definition, created_at)`；`CompositeStore` port（runtime 侧定义、db 适配）。最小 API：`GET/POST /workspaces/{id}/composites`、`DELETE /workspaces/{id}/composites/{cid}`（E3 场景包可直接导入）。

### 6.2 工具形态（固定工具列表）

- `list_composites {}` → 本区声明列表（id/name/description/step 数）。
- `run_composite {composite_id, params?}` → 整链一次审批、逐步执行。
- 两条工具恒存在，不随声明数量变化。

### 6.3 执行语义

```
run_composite
  → 加载声明 + 校验（本区、1..=10 步、引用物存在、op 词表）
  → ApprovalSeam.decide（整链；thing_id=None；args 含链摘要）
      Pending → token（批准后由批准通道执行整链）
      HumanApproved/AutoApproved → 执行
  → 顺序执行每一步：
       ToolExecSeam（call_device/set_device 同一包装）
       仲裁：每步 acquire(holder="composite:{id}:{run_id}")，步骤结束释放
       账本：kind=job，payload {compositeId, stepIndex, status: accepted|progress|result|rejected, v=1}
  → 失败：停止后续步骤；对已完成且有 rollback 的步骤逆序回滚（每步同样落账 rolled_back）
  → 结果：
      completed     全部成功
      failed        首步即失败（无部分成功）
      partial       部分成功且回滚未声明/回滚失败
      rolled_back   部分成功且回滚全部成功
```

- **partial 必须显式**：返回 `{status, steps:[{index, status, output?, error?}], message}`，绝不把部分成功报成 success。
- **审批**：整链一个决策点；批准通道执行整链（`APPROVAL_TOOL_DENYLIST` 不含 run_composite）；自动批准仅当链内全部 op 的分类 ≤ Write 且工作区 confirm 关闭。
- **溯源**：run trace 含 `run_composite` 调用；hub_ledger 逐步 job 事件链可回放（E3 Flow 的数据源）。
- 设计输入（真实协同场景）：`storm_guard` = 关闭天窗（call）→ 收太阳能板（call）→ 停 rover（call），回滚逆序。E2 用它做端到端验收（测试环境模拟设备）。

### 6.4 非目标（本阶段）

- 条件步/并行步/循环（只支持顺序链）
- 跨工作区引用（一律拒绝）
- 链内 watch/等待（watch 是独立工具；"等 A 完成再 B"留 E3 模板）
- 回滚的 exactly-once（尽力而为，失败如实上报 partial/failed）

## 7. 数据流（一个被打断的 turn，全景）

```
watch 命中 → WakeSignal{Watch} → scheduler → run_pipeline
  → WorkspaceSnapshot（LiveState 快照 ≤6KB）→ prompt（快照段 + 触发段 + 纪律段 + 边界段）
  → LLM → 工具调用
  → SeamTool → ToolExecSeam（审计/预算/包装）
      → call_device：ApprovalSeam → PolicySeam → 仲裁 → DataServer 下发 → 账本
      → watch：WatchRuleStore INSERT + WatchCommand::Upsert（persistent=false）
      → stop：stop 闸门 + 释放租约 + 账本
  → BudgetSeam 在每次 ToolCall/时间边界裁决；超限 → cancel
  → RunReport（含 snapshot 指标 + trace）→ agent_runs / 事件
```

## 8. 持久化与错误处理

| 失败 | 处理 | 用户可见 |
|---|---|---|
| 接缝替换实现 panic | 不能炸 run：ToolExec/Budget/Approval/Policy 调用包 `catch_unwind`（tokio spawn_blocking 不适用）或按 fail-closed 映射（Policy/Approval → Deny/Pending，Budget → Continue，ToolExec → 工具错误） | 工具错误/待审批 |
| 快照装配超时/失败 | 空快照（整段省略）+ warn；run 照常 | 否 |
| 快照超预算 | 按行截断 + omitted 计数 | 模型侧可见省略行 |
| composite 声明非法 | 工具错误 + 原因（fail-loud） | 是 |
| composite 步骤失败 | 停止 + 逆序回滚 + partial/rolled_back 明细 | 是 |
| composite 仲裁 busy | 该步拒绝（busy 带持有者），按失败路径回滚 | 是 |
| 旧名提案批准 | 隐藏别名仍可解析执行 | 是（无感） |
| 审批无人可问 | Unavailable（不执行，不记否决） | 是（"未执行"） |
| stop 面对在途调用 | 不中断在途单次调用，只阻断后续 | 文档声明 |

## 9. 测试策略

- **接缝一致性**（每接缝）：`RecordingSeam` 委托输出逐字段相等；`NarrowingSeam` 只能收窄；ToolExec 内层恰好调用一次；默认实现回归锁（25/300s、gate_check 顺序、confirm fail-closed）。
- **工具集**：固定 7 设备工具 schema 快照；`exposed()` 过滤（模型工具列表无旧名、批准通道仍可解析旧名）；read_device 一次返回 state+health+zone；set_device 只读属性拒绝；watch/unwatch 走真实 store+sink；stop 闸门 TTL 与释放租约。
- **快照**：空→整段省略；50 台中文基线 ≤6144 字节；worst-first 排序；装配失败 run 照常；metrics 落 report。
- **审批**：六分类矩阵（工具层 + 端点层）；Unavailable 不写 rejected；条件翻转不双执行。
- **composite**：成功链逐步落账；第 2 步失败→回滚第 1 步→rolled_back；无 rollback→partial；busy→失败+回滚；批准通道整链执行；跨区引用拒绝。
- **eval**：`evals/scenarios.json` 加快照注入基线用例（对比现四段式 prompt 场景）与 composite 提案用例。
- **端到端**：模拟驱动 + 真 manager：watch 命中 → 快照注入的 run → call_device 经五接缝 → 账本可回放；composite `storm_guard` 经审批执行并逐步落账。

## 10. 退出标准

1. 五个接缝各有一个真实替换实现，在真实 Runner/工具路径上生效（非 mock 核心）；默认实现回归测试全绿。
2. 快照注入上线：turn 开头自动携带本区 health/zone 摘要（≤6144 字节，实测基线在案），AI 不再需要逐物调 `get_thing_profile`（eval 场景证明工具调用序列改变）。
3. 一条 composite 链（`storm_guard`）端到端：提案可审批、批准后逐步执行、失败可回滚、hub_ledger 可回放每一步、Flow 数据源完整。
4. 旧 9 工具提案在隐藏别名下仍可批准执行；模型可见工具列表稳定为 7 设备工具 + 4 非设备工具 + run_composite/list_composites。
5. 审批六分类全矩阵有测试；`unavailable` 绝不写 `rejected`。

## 11. 非目标（YAGNI）

- E3 呈现层（Flow 时间机器/World 投影/规则模板库）
- E4 声明制 resources（stop/仲裁仍为每设备单一互斥资源）
- 精确 tokenizer 依赖（只记 bytes/chars/估算）
- stop 中断在途单次调用（E4 双通道再细化）
- composite 条件/并行/跨区
- 通用接缝 DSL / 配置化接缝（只有代码级替换实现）
- 版本协商框架（只在 payload 带 `v` 字段，未知字段忽略）

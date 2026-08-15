# Agent 共性能力抽取设计 — crates/agent

- 日期：2026-08-15（经 /plan-eng-review 修订，D2–D13 全部闭环）
- 状态：已评审（设计 + 工程评审双批准）
- 分支：`refactor/crates-reorg`

## 1. 背景与动机

`apps/cloud/src/domains/agent/` 当前约 21k 行，内部分三层：`loop_/`（纯运行时，已有 zero-axum CI 约束）、`host/`（AgentPool、tools、session、prompt 等通用机制 + axum handler）、`chat/`（OpenClaw 代理 handler）。

本次重构把**共性 agent 能力**抽取为独立 crate `crates/agent`（lib `tinyiothub_agent`），核心动机三个：

1. **边缘侧复用** — `apps/edge` 未来要跑 agent 能力（当前无任何 agent 代码，绿地消费方），必须从 cloud 解耦
2. **架构边界固化** — 用 crate 边界物理固化已有的 loop_/host 分层（mod 级约束太弱），CI 强制
3. **对外发布/SDK** — agent 能力未来作为独立库发布，需要干净的所有权和 API 面

**非目标**：HTTP handler 不搬；llm/policy/skills 不改动其内部；DB schema 不变；crates/memory 的引擎部分（`service.rs`）不动。

## 2. 已确认的关键决策

| # | 决策点 | 结论 |
|---|---|---|
| — | 抽取范围 | `loop_/` 全部 + `host/` 中不含 axum 的通用机制；HTTP handler 留在 cloud |
| — | crate 粒度 | 单一 `crates/agent`（lib `tinyiothub_agent`），内部模块分层 |
| — | 存储归属 | 存储是业务，不进 crate。crate 零 sqlx 源码依赖 |
| — | 端口风格 | **buzz 式内存 + 事件**（参考 buzz-agent）：状态全在内存，`AgentEvent` 广播出口，**不用** Repository trait |
| — | 重启恢复 | **启动时重建**：cloud 构造 `RestoreSnapshot` → `AgentRuntime::restore()` |
| — | zeroclaw 依赖 | 随 AgentPool 迁入 crates/agent |
| — | 稳定性定级 | Beta（与 llm/memory/policy/skills 同级） |
| D2 | 工具归属 | **工具框架**（注册/分发/策略门/信任评估）进 crate；**数据实现**（thing 工具、chat/history、config/service）留 cloud，经现有 `ToolRegistry` 注册进 crate。事件化改造面收敛为 loop_ 7 文件 36 处 SQL |
| D3 | 运行时入站 | crate 暴露**命令式入站 API**（`update_heartbeat_config()` / `upsert_thing_agent()` / `remove_thing_agent()` 等），与事件出口对称（命令进、事件出） |
| D4 | 事件可靠性 | subscriber 收到 `RecvError::Lagged` → 调 `dump_state()` 全量 resync；broadcast 容量调大（4096） |
| D5 | 导入纪律 | **无 re-export shim**（AGENTS.md 硬规则）；3 个外部消费方（`driver/plugin/registry.rs`、`mcp/tool_registry.rs`、`mcp/agent_bridge.rs`）直改 import 指向 `tinyiothub_agent` |
| D6 | 接缝测试 | 新增全链路 E2E：fake LLM 跑一轮 thing_agent loop → 断言事件序列 → 断言 DB 投影 → 断言 HTTP 读 API 一致 |
| D7 | 内存驱逐 | RunRegistry 保留活跃 run + 每 thing_agent 最近 N=50 条已完成 run；DB 是全量存档 |
| D8 | 战略校准 | 维持 buzz 式事件方案（用户明确选择，知悉外部声音的"直接依赖 db"替代论证） |
| D9 | 迁移顺序 | **先在 cloud 原地事件化，后搬迁**——剥离存储依赖后移动是真正的 `git mv`，CI guard 从 crate 创建第一天生效，无豁免中间态 |
| D10 | 验收口径 | 见下方"memory 纯逻辑并入"——并入后依赖树可达真正干净，验收维持**依赖树级**（源码 grep + `cargo tree` 双重检查） |
| D10' | memory 处置 | **memory 纯逻辑模块（knowledge/reflect/types/metrics/workspace_memory/provider/reference）本期并入 crates/agent**（用户指示）。唯一外部消费方就是 agent 域，无第三方破坏面；`service.rs` 引擎留 crates/memory，cloud 继续消费。**计划阶段扩展**：policy/skills→db 的边同样只是类型耦合（TrustConfig/Proposal/AutonomyPolicy 等），实现计划 Task 1 把这些领域值类型归位 `crates/core`（沿用 core::memory 先例），policy/skills 就此脱掉 db 依赖，依赖树纯度完整达成 |
| D11 | 事件硬化包 | ①订阅先于 restore；②事件携带单调 version，upsert 带 fencing；③启动 reconcile：DB 僵尸 running 行标记 interrupted；④周期性全量对账（不仅 Lagged 触发）；⑤命令与 DB 写序：先写 DB 再发命令，命令失败告警，重启以 DB 为准；⑥单实例假设写进 spec |
| D12 | 总线关系 | **双通道职责分离**：既有 `AiEventPublisher`（mpsc 有界 + DropNotifier，满即丢+告警）继续服务 runtime 系统事件；新 `AgentEvent`（broadcast + resync + fencing）专服务持久化投影。spec 明确分工，不合并 |
| D13 | 心跳新鲜度 | 心跳实时字段（last_tick、指标）读 API 改走 crate 内存查询；"读路径不变"修正为"**历史/归档读 DB，实时状态读 crate**" |

## 3. 总体架构

依赖方向（单向，CI 强制）：

```
apps/cloud (组合根)                apps/edge (未来)
   │  ├─ HTTP handlers（留在 cloud）     │
   │  ├─ 工具数据实现（thing 工具等）     │
   │  ├─ 持久化 subscriber（事件→DB）    │
   │  └─ 启动 rehydration（DB→snapshot） │
   ▼                                    ▼
crates/agent (tinyiothub_agent)  ◄──────┘
   │  零 axum / 零 sqlx / 零 tinyiothub_storage（源码 + 依赖树双重约束）
   ▼
crates/llm · crates/policy · crates/skills · crates/core · zeroclaw
（crates/memory 纯逻辑已并入本 crate；crates/runtime 仅 EventBus 契约）
```

## 4. crate 内部布局

```
crates/agent/
  Cargo.toml            # [lib] name = "tinyiothub_agent"，forbid(unsafe_code)
  src/
    lib.rs              # facade：pub use 各层公共 API + crate 级文档
    runtime/            # ← loop_/ 迁入（事件化改造完成后才是真正的纯移动，见 D9）
      event/            #   AiEvent bus 类型 + DLQ 类型（DLQ 落库实现留 cloud）
      thing_agent/      #   manager/scheduler/runner/pushback/trigger/prompt/traits
      heartbeat/        #   loop_/runner/report/metrics/types（repo.rs 的 storage re-export 废除）
      orchestrator/     #   Orchestrator + callbacks
      agent/            #   agent pool contract（现 loop_::agent）
    pool/               # ← host/agent/（AgentPool + zeroclaw 构建；先剥离 SqlitePool/MemoryStore 字段）
    tools/              # ← 仅工具框架：注册/分发/策略门/信任评估/catalog
                        #   （thing 工具实现、canvas、autonomous_invoke 等数据实现留 cloud，见 D2）
    session/            # ← host/session.rs（SessionKey 解析）；chat 通用逻辑极薄（D2 后 chat/history、
                        #   chat/service 的 DB 部分留 cloud），本模块以 SessionKey 为主
    prompt/             # ← host/shared/ 的 prompt 组装 + scaffold 逻辑
    memory/             # ← crates/memory 纯逻辑并入（D10'）：knowledge/reflect/types/metrics/
                        #   workspace_memory/provider/reference
    events.rs           # AgentEvent 枚举 + broadcast 出口（持久化投影专用通道，见 D12）
    commands.rs         # 命令式入站 API（D3）：update_heartbeat_config / upsert_thing_agent / ...
    snapshot.rs         # RestoreSnapshot 契约 + dump_state()（D4 全量 resync 出口）
    error.rs            # AgentError（thiserror，无 HTTP 语义）
```

**留在 cloud 的部分**（`apps/cloud/src/domains/agent/` 重构后只剩）：

- `host/handler/*`、`chat/handler/*` — 全部 axum handler
- 工具数据实现：thing 工具、canvas、autonomous_invoke、dispatch_task、get_skill 等（注册进 crate 的 ToolRegistry）
- `dlq_repo.rs`、`chat/history.rs`、`config/service.rs`、memory handler 的 SQL
- 持久化 subscriber + 启动 rehydration + 僵尸行 reconcile（新写）
- `AgentState` 组合切片；workspace/tenant 访问校验（`verify_workspace` 留在 handler 层）

## 5. 关键边界决策

1. **crate 对租户零感知** — `workspace_id: String` 只是不透明分区键，访问校验永远发生在 cloud handler
2. **读模型分工（D13 修正）** — 历史/归档数据（runs 列表、跃迁历史）读 DB；实时状态（活跃 run、心跳 last_tick）读 crate 内存 API
3. **`AgentState` 字段换类型不换结构** — 组合根改 import 指向 `tinyiothub_agent::*`
4. **单实例假设（D11-⑥）** — cloud 恰好一个进程持有内存真相源 + 单 subscriber 投影；水平扩展是本架构的明确非目标。写进 spec 即声明：多副本部署会出现双份自治 loop 互相覆盖投影
5. **双事件通道分工（D12）** — `AiEventPublisher`：runtime 系统事件，满即丢+告警；`AgentEvent`：持久化投影，不丢+Lagged resync+fencing。两者共存，语义不混

## 6. 核心契约（公共 API 面）

```rust
// events.rs — 持久化投影唯一出口（D12：与 AiEvent 系统事件通道分离）
pub enum AgentEvent {
    RunStarted { version: u64, run_id, thing_id, workspace_id, trigger, .. },
    RunFinished { version: u64, run_id, outcome, usage, .. },
    RunFailed  { version: u64, run_id, error, .. },
    HeartbeatStateChanged { version: u64, workspace_id, trust_level, .. },  // 只发状态跃迁
    DirectiveReceived { version: u64, .. },
    DlqEntryAdded { version: u64, entry },
    // ...
}
// D11-②：每个事件携带单调 version；subscriber upsert 带 fencing
// （WHERE version > stored_version），旧事件无法覆盖新状态。

// commands.rs — 运行时入站唯一通道（D3）
impl AgentRuntime {
    pub fn update_heartbeat_config(&self, ws: &str, cfg: HeartbeatConfig) -> Result<()>;
    pub fn upsert_thing_agent(&self, cfg: ThingAgentConfig) -> Result<()>;
    pub fn remove_thing_agent(&self, thing_id: &str) -> Result<()>;
    // D11-⑤ 写序：cloud handler 先写 DB，成功后调命令；命令失败 → 告警，
    // 进程内状态以重启时 DB 重建为准。
}

// snapshot.rs — 启动重建入口 + 全量 resync 出口
pub struct RestoreSnapshot {
    pub thing_agents: Vec<ThingAgentConfig>,
    pub heartbeat_states: Vec<HeartbeatState>,
    pub pending_directives: Vec<Directive>,
    pub recent_runs: Vec<RunSummary>,      // pushback 预热窗口，每 agent 最近 N=50
}
impl AgentRuntime {
    pub fn restore(snapshot: RestoreSnapshot) -> Self;
    pub fn events(&self) -> broadcast::Receiver<AgentEvent>;
    pub fn dump_state(&self) -> RestoreSnapshot;   // D4/D11-④：Lagged resync + 周期对账
}
```

（字段为示意，实现时以现有类型为准；`AgentEvent` 各变体携带完整状态而非增量 diff。）

## 7. 数据流

1. **启动**（顺序敏感，D11-①③）：
   a. cloud 从 DB 读活跃配置构造 `RestoreSnapshot`
   b. **先订阅** `events()`（或 restore 返回缓冲事件）——broadcast 只投递给已存在的 receiver，先 restore 后订阅会丢启动期事件
   c. `AgentRuntime::restore()`
   d. **僵尸行 reconcile**：DB 中 status=running 但内存 RunRegistry 无主的 run 标记 `interrupted`
   e. 启动持久化 subscriber → 构建 axum router
2. **运行时（自治 loop）**：thing_agent loop 全在内存执行；状态变化点发 `AgentEvent`（带 version）；subscriber 幂等 upsert（fencing：`WHERE version > stored`）
3. **读路径（D13）**：历史/归档 → DB；实时状态（活跃 run、心跳 last_tick/指标）→ crate 内存 API
4. **配置变更（D3/D11-⑤）**：handler 校验 → 写 DB → 调 crate 命令 API → 失败则告警（重启后 DB 为准）

**频率控制**：heartbeat tick 等高频路径不发事件，只有状态跃迁才发，避免 subscriber 写放大（D13：实时性由内存 API 承担，不靠 tick 落库）。

**可靠性（D4/D11）**：
- broadcast 容量 4096；subscriber `Lagged` → `dump_state()` 全量 resync
- 周期性全量对账（定时器触发 dump_state 投影），不只依赖 Lagged
- 崩溃丢失窗口：事件已发但未落库时进程死亡 → 该状态变化丢失，重启后从 DB 重建（**已知取舍，buzz 式内存的固有代价**）

## 8. 存储解耦改造点（实测校准后）

**事件化改造（在 cloud 原地进行，D9）—— loop_ 7 文件 36 处 SQL：**

| 文件（现址 `domains/agent/`） | 实测 SQL 点数 | 改法 |
|---|---|---|
| `loop_/thing_agent/manager.rs` | 5 | 内存 `RunRegistry`（活跃+N=50 窗口，D7）+ 事件 |
| `loop_/thing_agent/scheduler.rs` | 5 | 内存态 + 事件 |
| `loop_/heartbeat/runner.rs` | 9（含测试夹具） | 内存态 + 跃迁事件；`heartbeat/repo.rs` 的 storage re-export 废除 |
| `loop_/heartbeat/loop_.rs` | 5 | 内存态 + 事件 |
| `loop_/orchestrator/callbacks.rs` | 6 | 内存态 + 事件 |
| `loop_/orchestrator/mod.rs` | 2 | 内存态 + 事件 |
| `loop_/event/bus.rs` | 4 | DLQ 写 → `DlqEntryAdded` 事件 |

注：`pushback.rs` 无直接 SQL（经 manager 间接读），随 RunRegistry 窗口自然解决。

**AgentPool 剥离（P0 级隐藏工作量，外部声音发现 3）**：`host/agent/pool.rs:8` 持 `sqlx::SqlitePool`、`:91` 持 `Arc<MemoryStore>`——chat prompt 构造所需的 memory 数据改由调用方（cloud handler/service 层）注入，pool 只管 agent 生命周期与 zeroclaw 构建。

**留 cloud 不事件化**（D2）：thing 工具实现（read_property/query_events/invoke_action/search_knowledge/read_document）、canvas、autonomous_invoke、dispatch_task、chat/history.rs、config/service.rs、skill.rs、policy_engine.rs、thing_agent_host.rs、heartbeat.rs（host 侧）、dlq_repo.rs —— 这些是业务数据访问，继续直接用 `tinyiothub_storage` 具体类型（符合 8/3 db 层模式）。

## 9. 错误处理

- crate 内统一 `AgentError`（thiserror）：`Llm` / `Tool` / `Policy` / `Session` / `Internal` 等变体，不含 HTTP 状态码
- cloud 侧 `domains/agent/error.rs` 做 `AgentError → ApiResponse` 映射
- **事件写库失败**：subscriber 重试 3 次 + 指数退避 → 仍失败写 cloud 侧 DLQ 表 + `tracing::error!`；周期对账（D11-④）兜底检测分叉
- **命令失败**（D11-⑤）：DB 已写而命令失败 → `tracing::warn!` + 返回成功但提示"部分生效，重启后完全一致"
- LLM/工具调用失败沿用现有重试与 pushback 语义，仅状态记录改走事件

## 10. 测试策略

- `crates/agent`：纯单元测试 + 内存 fake（`test_utils.rs` / `directive_sink.rs` stub 随代码迁入），不起 DB、不起 HTTP
- `apps/cloud`：现有集成测试（`agent_tasks_api_tests` 等）保持通过——验证读模型回归
- **新增测试清单**（评审 D6/D11 直接产出）：
  - subscriber 事件→DB 投影测试（每类事件一个用例，含 fencing：乱序/旧 version 不覆盖）
  - rehydration 往返测试（snapshot → restore → 事件 → DB 一致）
  - **全链路 E2E**（D6）：fake LLM 一轮 thing_agent loop → 事件序列 → DB 投影 → HTTP 读 API 一致
  - 事件发射断言：RunStarted/Finished/Failed 各跃迁；**反向断言**：heartbeat tick 不发射事件
  - Lagged → dump_state resync 测试
  - 僵尸行 reconcile 测试（DB 插 running 行 → 启动 → 断言标记 interrupted）
  - 入站命令测试（运行中 update_heartbeat_config 即生效）
  - 工具框架链路：cloud 注册 thing 工具 → crate 分发 → 策略门
- **CI guard**：crate 创建即生效——`crates/agent` 源码禁 `axum` / `sqlx` / `tinyiothub_storage` / `crate::domains`；`cargo tree` 校验依赖树无 sqlx/storage（D10：memory 并入后可达）。**同步重写 `ci.yml:129-147` 两条既有守卫**（G9 loop 纯度 + thing/tenant 边守卫）的路径——它们 grep 的 `apps/cloud/src/domains/agent/loop_` 搬迁后失效

## 11. 迁移步骤（D9 修订：先事件化，后搬迁；每步独立可编译）

1. **cloud 原地事件化**（行为改变步骤，单独 PR）——`events.rs`/`snapshot.rs`/`commands.rs` 先作为 cloud 内部模块建立；loop_ 7 文件 36 处 SQL → 内存+事件；AgentPool 剥离 SqlitePool/MemoryStore；cloud subscriber + rehydration + 僵尸 reconcile + 周期对账；D6 E2E 与 D11 各测试落地
2. **建 crate 骨架 + CI guard**——`crates/agent` + workspace 注册 + guard（含 ci.yml 两条既有守卫路径重写）
3. **搬 memory 纯逻辑**（D10'）——`crates/memory` 的 knowledge/reflect/types/metrics/workspace_memory/provider/reference → `crates/agent/src/memory/`；`crates/memory` 只剩 `service.rs` 引擎
4. **搬 runtime/**——`loop_/` 整体平移（此时已是真正的 `git mv` 级纯移动）；`domains/agent/types` 兼容 re-export 删除，3 个外部消费方直改 import（D5）
5. **搬 pool/tools 框架/session/prompt**——host 非 HTTP 非数据部分迁入；zeroclaw 依赖移至 crate
6. **清理**——删除 cloud 侧被搬空的模块、`AgentState` 换类型、AGENTS.md/Stability Tiers 表更新

风险控制：步骤 1 是唯一的行为改变步骤（原地改，现有测试全部可用作安全网）；步骤 2-5 是纯移动。

## 12. 验收标准

- [ ] `cargo build -p tinyiothub-agent` 独立编译通过
- [ ] `crates/agent` 源码 grep 无 `axum`/`sqlx`/`tinyiothub_storage`；`cargo tree -p tinyiothub-agent` 无 sqlx/tinyiothub-storage（D10：memory 并入后可达；core 的 sqlx 为 optional feature 不启用）
- [ ] CI guard 对 `crates/agent` 生效；ci.yml 两条既有守卫路径已重写
- [ ] `cargo test -p tinyiothub-agent` 全绿（无 DB 无 HTTP）
- [ ] cloud 现有 agent 集成测试全部通过（读模型回归）
- [ ] D6 全链路 E2E + D11 硬化测试（fencing/reconcile/resync/命令）全部落地
- [ ] AGENTS.md 的 Repository Map 与 Stability Tiers 表更新

## 13. NOT in scope（评审中考虑过、明确推迟）

| 项 | 理由 |
|---|---|
| HTTP handler 搬迁 | axum 处理层是 cloud 组合职责，动机不含它 |
| crates/memory 引擎（service.rs）处置 | 本期只移纯逻辑；引擎去留等 cloud 侧 memory 消费形态稳定后再定 |
| SDK 发布管线 / zeroclaw git 依赖治理 | 用户评审时明确跳过（TODO-2 跳过）；发布前必须解决，届时立项 |
| 多副本/水平扩展 | 单实例假设已声明（§5.4）；多副本需要分布式协调，是另一个架构时代的事 |
| edge 侧实际接入 | 绿地消费方，本期的交付物是"可接入的 crate"，接入本身另立项 |
| AiEvent 系统事件总线改造 | 双通道分工已明确（D12），旧总线消费者不动 |
| unwrap/expect 治理（TODOS P2） | 与本次解耦，已独立跟踪 |

## 14. What already exists（复用清单）

| 已有 | 复用方式 |
|---|---|
| `tinyiothub_skills::registry::ToolRegistry` | D2 的框架核心，cloud 工具实现注册进来，不另造注册中心 |
| `tinyiothub_policy`（策略门/信任评估） | 工具分发链路沿用，crate 依赖组合 |
| `tinyiothub_llm`（provider/prompt/session 契约） | 直接使用 |
| `loop_/event/bus.rs` AiEventPublisher + `service_manager.rs:151,235` 接线 | 系统事件通道原样保留（D12），搬迁时接线点随迁 |
| `heartbeat/repo.rs` HeartbeatTaskRepository | 存储实现留 cloud；crate 侧由事件+snapshot 替代其运行时读取 |
| `test_utils.rs` / `directive_sink.rs` | 测试 stub 随代码迁入 crate |
| ci.yml G9 guard 模式 | guard 重写时沿用同款 grep 模式 |
| buzz-agent（外部参考） | 内存+事件形态、wire 进出对称的范本 |

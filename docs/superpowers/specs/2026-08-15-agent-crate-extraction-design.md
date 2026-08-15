# Agent 共性能力抽取设计 — crates/agent

- 日期：2026-08-15
- 状态：已评审（用户已批准设计）
- 分支：`refactor/crates-reorg`

## 1. 背景与动机

`apps/cloud/src/domains/agent/` 当前约 21k 行，内部分三层：`loop_/`（纯运行时，已有 zero-axum CI 约束）、`host/`（AgentPool、tools、session、prompt 等通用机制 + axum handler）、`chat/`（OpenClaw 代理 handler）。

本次重构把**共性 agent 能力**抽取为独立 crate `crates/agent`（lib `tinyiothub_agent`），核心动机三个：

1. **边缘侧复用** — `apps/edge` 未来要跑 agent 能力（当前无任何 agent 代码，绿地消费方），必须从 cloud 解耦
2. **架构边界固化** — 用 crate 边界物理固化已有的 loop_/host 分层（mod 级约束太弱），CI 强制
3. **对外发布/SDK** — agent 能力未来作为独立库发布，需要干净的所有权和 API 面

**非目标**：HTTP handler 不搬；现有能力 crate（llm/memory/policy/skills）不合并、不改动其内部；DB schema 不变。

## 2. 已确认的关键决策

| 决策点 | 结论 |
|---|---|
| 抽取范围 | `loop_/` 全部 + `host/` 中不含 axum 的通用机制（AgentPool、session、tools、prompt 组装）；HTTP handler 留在 cloud |
| crate 粒度 | 单一 `crates/agent`（lib `tinyiothub_agent`），内部模块分层 |
| 存储归属 | 存储是业务，不进 crate。参考 buzz-agent（`/Users/chenguorong/code/github/buzz/crates/buzz-agent`）：crate 零 sqlx 零 DB 依赖 |
| 端口风格 | **buzz 式内存 + 事件**：crate 状态全在内存，通过 `AgentEvent` 广播通知外部；cloud 订阅落库。**不用**窄端口 Repository trait |
| 重启恢复 | **启动时重建**：cloud 从 DB 读配置构造 `RestoreSnapshot`，调 `AgentRuntime::restore()` |
| 与现有能力 crate | **依赖组合**：crates/agent 依赖 llm/memory/policy/skills，它们保持独立演进 |
| zeroclaw 依赖 | 随 AgentPool 一起迁入 crates/agent（AgentPool 负责构建 zeroclaw agent） |
| 稳定性定级 | Beta（与 llm/memory/policy/skills 同级） |

## 3. 总体架构

依赖方向（单向，CI 强制）：

```
apps/cloud (组合根)                apps/edge (未来)
   │  ├─ HTTP handlers（留在 cloud）     │
   │  ├─ 持久化 subscriber（事件→DB）    │
   │  └─ 启动 rehydration（DB→snapshot） │
   ▼                                    ▼
crates/agent (tinyiothub_agent)  ◄──────┘
   │  零 axum / 零 sqlx / 零 tinyiothub_storage / 零 crate::domains
   ▼
crates/llm · crates/memory · crates/policy · crates/skills · zeroclaw
```

## 4. crate 内部布局

```
crates/agent/
  Cargo.toml            # [lib] name = "tinyiothub_agent"，forbid(unsafe_code)
  src/
    lib.rs              # facade：pub use 各层公共 API + crate 级文档
    runtime/            # ← loop_/ 整体迁入（纯运行时）
      event/            #   AiEvent bus + DLQ 类型（DLQ 落库实现留 cloud）
      thing_agent/      #   manager/scheduler/runner/pushback/trigger/prompt/traits
      heartbeat/        #   loop_/runner/report/metrics/types
      orchestrator/     #   Orchestrator + callbacks
      agent/            #   agent pool contract（现 loop_::agent）
    pool/               # ← host/agent/（AgentPool + zeroclaw 构建 + config + chat）
    tools/              # ← host/tools/（thing 工具、canvas、dispatch_task、autonomous_invoke、get_skill、service）
    session/            # ← host/session.rs（SessionKey 解析）+ host/chat 非 HTTP 部分
    prompt/             # ← host/shared/ 的 prompt 组装 + scaffold 逻辑
    events.rs           # AgentEvent 枚举 + broadcast 出口（持久化唯一通道）
    snapshot.rs         # RestoreSnapshot 契约（启动重建唯一入口）
    error.rs            # AgentError（thiserror，无 HTTP 语义）
```

**留在 cloud 的部分**（`apps/cloud/src/domains/agent/` 重构后只剩）：

- `host/handler/*`、`chat/handler/*` — 全部 axum handler
- `dlq_repo.rs`、memory handler 的 SQL、持久化 subscriber（新写）
- `AgentState` 组合切片、rehydration 启动逻辑（新写）
- workspace/tenant 访问校验（`verify_workspace` 留在 handler 层）

## 5. 关键边界决策

1. **crate 对租户零感知** — `workspace_id: String` 只是不透明分区键，访问校验永远发生在 cloud handler
2. **DB 仍是读模型** — cloud handler 现在从 DB 查 runs/heartbeat 状态的 API 全部不变；事件 subscriber 负责把 crate 状态变化写回 DB
3. **`AgentState` 字段换类型不换结构** — `agent_pool` 等字段类型从 cloud 内部类型换成 `tinyiothub_agent::*`，组合根改 import

## 6. 核心契约（公共 API 面）

```rust
// events.rs — 持久化唯一出口
pub enum AgentEvent {
    RunStarted { run_id, thing_id, workspace_id, trigger, .. },
    RunFinished { run_id, outcome, usage, .. },
    RunFailed  { run_id, error, .. },
    HeartbeatStateChanged { workspace_id, trust_level, .. },  // 只发状态跃迁，不发每 tick
    DirectiveReceived { .. },
    DlqEntryAdded { entry },   // cloud subscriber 写 dlq 表
    // ...
}
pub struct EventBus { /* tokio::broadcast::Sender<AgentEvent> */ }

// snapshot.rs — 启动重建唯一入口
pub struct RestoreSnapshot {
    pub thing_agents: Vec<ThingAgentConfig>,   // 活跃 agent 配置
    pub heartbeat_states: Vec<HeartbeatState>, // 各 workspace 信任级/配置
    pub pending_directives: Vec<Directive>,    // 未消费指令
}
impl AgentRuntime {
    pub fn restore(snapshot: RestoreSnapshot) -> Self;
    pub fn events(&self) -> broadcast::Receiver<AgentEvent>;
}
```

（字段为示意，实现时以现有类型为准；`AgentEvent` 各变体携带完整状态而非增量 diff。）

## 7. 数据流

1. **启动**：cloud bootstrap 从 `tinyiothub_storage` 读活跃配置 → 构造 `RestoreSnapshot` → `AgentRuntime::restore()` → 订阅 `events()` 启动持久化 subscriber → 构建 axum router（handler 拿 `Arc<AgentRuntime>` / `Arc<AgentPool>`）
2. **运行时（自治 loop）**：thing_agent loop 全在内存执行；状态变化点发 `AgentEvent`；cloud subscriber 异步落库。**约束：事件携带完整状态，subscriber 幂等 upsert**——重启后重放不依赖事件顺序
3. **HTTP 读路径**：handler 查 DB（现状不变）；需要实时内存状态的少数场景（如"当前正在跑的 run"）由 crate 提供只读查询 API（`runtime.active_runs()`）

**频率控制**：heartbeat tick 等高频路径不发事件，只有**状态跃迁**（trust level 变化、配置变更）才发，避免 subscriber 成为写入瓶颈。

## 8. thing_agent 直读 SQL 的改造点（核心工作量，~6 个文件）

| 文件（现址 `domains/agent/`） | 现状 | 改法 |
|---|---|---|
| `loop_/thing_agent/manager.rs` | run 记录创建/更新直写 SQL | 内存 `RunRegistry` + 事件 |
| `loop_/thing_agent/scheduler.rs` | 调度读写 SQL | 内存态 + 事件 |
| `loop_/thing_agent/pushback.rs` | 读历史 run 做 pushback 判断 | 内存滑动窗口，启动时从 snapshot 预热（最近 N=50 条 run 摘要） |
| `loop_/orchestrator/callbacks.rs` | memory profile 编译读写 | 内存态 + 事件 |
| `host/dlq_repo.rs` | DLQ 落库 | 留在 cloud；crate 发 `DlqEntryAdded` 事件 |
| `host/memory/handler.rs` | memory SQL | 留在 cloud（HTTP handler 一部分） |

**已确认的取舍**：

- pushback 历史窗口改内存，重启后从 snapshot 预热，预热深度 N=50 条 run 摘要
- "正在跑的 run"类实时查询走 crate 内存 API 而非 DB；重启期间该 API 短暂返回空，可接受

## 9. 错误处理

- crate 内统一 `AgentError`（thiserror）：`Llm` / `Tool` / `Policy` / `Session` / `Internal` 等变体，不含 HTTP 状态码
- cloud 侧 `domains/agent/error.rs` 做 `AgentError → ApiResponse` 映射（沿用现有 web 基础设施）
- **事件写库失败**：subscriber 重试 3 次 + 指数退避 → 仍失败写 cloud 侧 DLQ 表 + `tracing::error!`。**不反向影响 crate 运行**（内存态是 source of truth，DB 只是读模型投影，允许短暂落后）
- LLM/工具调用失败沿用现有重试与 pushback 语义，仅状态记录改走事件

## 10. 测试策略

- `crates/agent`：纯单元测试 + 内存 fake（现有 `test_utils.rs` / `directive_sink.rs` stub 随代码迁入），不起 DB、不起 HTTP
- `apps/cloud`：现有集成测试（`agent_tasks_api_tests` 等）保持通过为验收标准——它们走 HTTP→DB 路径，正好验证"读模型不变"
- 新增：subscriber 事件→DB 投影测试（每类事件一个用例）；rehydration 往返测试（snapshot → restore → 事件 → DB 与原始 DB 一致）
- **CI guard 扩展**：现有 `loop_` zero-axum 检查改为检查 `crates/agent`，禁依赖 `axum` / `sqlx` / `tinyiothub_storage` / `crate::domains`

## 11. 迁移步骤（每步独立可编译、对应独立 commit）

1. **建 crate 骨架** — `crates/agent` + workspace 注册 + CI guard + 空模块
2. **搬 runtime/** — `loop_/` 整体平移（纯移动不改逻辑）；cloud 改 import；`domains/agent/types` 兼容 re-export 指向新 crate
3. **搬 pool/tools/session/prompt** — host 非 HTTP 部分迁入；handler 改 import；zeroclaw 依赖移至 crate
4. **事件化改造** — `events.rs` / `snapshot.rs` + 第 8 节的存储直读点改造 + cloud subscriber + rehydration 启动逻辑。**唯一的行为改变步骤，单独成 PR**
5. **清理** — 删除 cloud 侧被搬空的模块、`AgentState` 换类型、更新 AGENTS.md / 文档

风险控制：步骤 2-3 是 `git mv` 级纯移动（review 快）；步骤 4 配 subscriber 投影测试兜底。

注：本 spec 的文件归属以模块为单位描述（如 "host/chat 非 HTTP 部分"）；逐文件的搬迁/留库清单在实现计划（writing-plans）阶段产出，以"crate 内零 axum/零 sqlx"为判定标准。

## 12. 验收标准

- [ ] `cargo build -p tinyiothub-agent` 独立编译通过，依赖树中无 axum/sqlx/tinyiothub_storage
- [ ] CI guard 脚本对 `crates/agent` 生效，违规依赖直接 fail
- [ ] `cargo test -p tinyiothub-agent` 全绿（无 DB 无 HTTP）
- [ ] cloud 现有 agent 集成测试全部通过（读模型不变）
- [ ] 重启恢复路径有往返测试覆盖
- [ ] AGENTS.md 的 Repository Map 与 Stability Tiers 表更新

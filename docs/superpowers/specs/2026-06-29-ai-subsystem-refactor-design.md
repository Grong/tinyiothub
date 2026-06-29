# AI 子系统重构设计

## 目标

将当前分散、耦合的 AI 相关代码重构为清晰的领域模块架构，解决 AgentPool 上帝对象、类型混杂、依赖注入不统一等问题。

## 最终文件结构

```
crates/tinyiothub-ai/
├── Cargo.toml
└── src/
    ├── lib.rs                    # pub mod 声明 + AiSystem 入口
    │
    ├── agent/
    │   ├── mod.rs
    │   ├── types.rs              # AgentRuntimeConfig, AgentError, AgentInfo
    │   ├── repo.rs               # AgentRepository trait + SQLite impl
    │   ├── service.rs            # AgentService: CRUD
    │   ├── pool.rs               # AgentPool: get_or_create, invalidate, cleanup
    │   ├── builder.rs            # AgentBuilder: 组装 zeroclaw Agent
    │   └── handler.rs            # HTTP 路由处理
    │
    ├── session/
    │   ├── mod.rs
    │   ├── types.rs              # SessionKey, Session, ChatEvent, ChatError
    │   ├── repo.rs               # SessionRepository trait + SQLite impl
    │   ├── service.rs            # SessionService: 生命周期管理
    │   └── chat.rs               # ChatService: send_message, history, abort
    │
    ├── patrol/
    │   ├── mod.rs
    │   ├── types.rs              # HeartbeatTask, HealingReport, WakeSignal, TrustConfig
    │   ├── repo.rs               # AgentActionRepository trait + SQLite impl
    │   ├── manager.rs            # PatrolManager: 循环生命周期管理
    │   ├── loop.rs               # patrol_loop(): 实际巡检逻辑
    │   └── report.rs             # 解析 HealingReport、记录 Action
    │
    ├── alarm/
    │   ├── mod.rs
    │   ├── types.rs              # Alarm, AlarmRule, AlarmCondition, AlarmTrigger 等
    │   ├── repo.rs               # AlarmRepository + AlarmRuleRepository trait + impl
    │   ├── service.rs            # AlarmService: 告警 CRUD、统计
    │   ├── engine.rs             # RuleEngine: 条件评估、去抖、节流、恢复
    │   └── handler.rs            # AlarmEventHandler
    │
    ├── tool/
    │   ├── mod.rs
    │   ├── types.rs              # ToolMetadata, PermissionLevel
    │   ├── registry.rs           # ToolRegistry: MCP 工具加载、分类
    │   ├── trust.rs              # TrustEngine + TrustAwareTool wrapper
    │   ├── catalog.rs            # build_catalog(), 分组、标签
    │   └── adapters/
    │       ├── mod.rs
    │       ├── canvas.rs
    │       ├── knowledge.rs
    │       └── workspace.rs
    │
    ├── memory/
    │   ├── mod.rs
    │   ├── types.rs              # ChatTurnMessage, MemoryFact
    │   ├── service.rs            # MemoryService
    │   ├── reflect.rs            # reflect_conversation_turn()
    │   ├── profile.rs            # compile_profile()
    │   └── digest.rs             # generate_weekly_digest()
    │
    ├── event/
    │   ├── mod.rs
    │   ├── types.rs              # AiEvent 枚举
    │   └── bus.rs                # EventBus: 事件发布/订阅
    │
    └── orchestrator/
        ├── mod.rs
        └── callbacks.rs          # 跨领域回调注册
```

## 领域职责

| 领域 | 职责 | 当前状态 |
|------|------|----------|
| `agent/` | Agent 生命周期（CRUD、池管理、构建） | AgentPool 796 行上帝对象，需拆分 |
| `session/` | 会话管理 + Chat 对话执行 | Chat 在子模块，Session 分散，需合并 |
| `patrol/` | 自主巡检引擎（原 heartbeat） | heartbeat.rs + heartbeat_manager.rs 共 1310 行 |
| `alarm/` | 告警规则引擎 + 生命周期 | 已相对完整，需瘦身 + 接口规范化 |
| `tool/` | 工具注册、Trust 控制、Catalog | 554 行，结构良好，需微调 |
| `memory/` | 对话记忆提取、画像、周报 | 392 行 + templates/，独立即可 |
| `event/` | AI 系统事件定义 + 发布/订阅总线 | 新建 |
| `orchestrator/` | 跨领域回调编排 | 新建，解决当前直接耦合 |

## 共享类型

跨领域使用的类型定义在 `src/types.rs`（crate 根），各领域按需 re-export：

```rust
// src/types.rs — 跨领域共享类型
pub use patrol::types::{TrustConfig, TrustLevel, WakeSignal, WakePriority};
pub use event::types::AiEvent;
```

- 每个领域通过 `pub use crate::types::XXX` 引入
- 不允许跨领域直接 import（如 `use crate::alarm::types::XXX` from `patrol/`）
- 新类型加入 `src/types.rs` 前需在 PR 中说明为什么不放在领域内部

## 依赖规则

```
orchestrator 可以依赖所有领域（它是协调者）
其他领域之间不直接依赖，通过 orchestrator 的事件回调通信
每个领域内部保持 Handler → Service → Repo 三层

例外：tool/adapters/ 可以依赖 workspace/knowledge 领域，但必须通过
ToolDependencyProvider trait，不得直接持有 WorkspaceService/KnowledgeService
```

`ToolDependencyProvider` trait 定义在 `tool/types.rs`：

```rust
#[async_trait]
pub trait ToolDependencyProvider: Send + Sync {
    async fn resolve_knowledge(&self, workspace_id: &str) -> Option<Arc<dyn KnowledgeProvider>>;
    async fn resolve_workspace(&self, workspace_id: &str) -> Option<Arc<dyn WorkspaceProvider>>;
}
```

## 每个领域模块的标准文件结构

```
<domain>/
├── mod.rs          # pub mod, re-exports
├── types.rs        # 只放本领域的 DTO/Enum/Error（≤ 200 行）
├── repo.rs         # Repository trait + SQLite 实现
└── service.rs      # 核心逻辑，依赖 Arc<dyn RepoTrait>
```

- 所有 SQL 都在 Repo 实现中，Service 不碰 SQL
- 错误类型用 `thiserror` 枚举，非 `anyhow`
- 构造函数注入 `Arc<dyn Trait>`，启动时一次性组装

## EventBus 设计

**复用现有 `tinyiothub_runtime::EventBus`**，而非新建独立总线。

现有 EventBus 特性（`crates/tinyiothub-runtime/src/event_bus.rs`）：
- `tokio::sync::broadcast` channel，capacity 1000
- `EventHandler` trait + `ArcSwap` 优先级分发
- 已支持 SSE broadcast 和 `AlarmEventHandler`
- 所有现有 subscribers 无需修改

新增内容：
- 将 `AiEvent` 变体加入 `tinyiothub_runtime::Event` 枚举
- AI subscribers 实现 `EventHandler` trait，通过 `event_bus.register_handler()` 注册
- **Lagging receiver 策略:** 当 `send` 返回 error 时，记录 `warn!(event_type, lag_count)` 并递增 `events_dropped` 计数器
- **Metrics 计数器:** `events_published`, `events_dropped` per event variant，通过 `/health/metrics` 暴露
- **Critical alert:** `events_dropped > 0` 在 5 分钟内触发告警

## Orchestrator 设计

`AiEvent` 加入现有 `tinyiothub_runtime::Event` 枚举：

```rust
pub enum AiEvent {
    AlarmCreated(Alarm),
    AlarmResolved { alarm_id: String, device_id: String, rule_id: Option<String> },
    PatrolCompleted { workspace_id: String, report: PatrolReport },
    ChatCompleted { workspace_id: String, agent_id: String, session_key: String, model: String, messages: Vec<ChatTurnMessage> },
}
```

`PatrolReport` 类型定义在 `patrol/types.rs`：

```rust
pub struct PatrolReport {
    pub workspace_id: String,
    pub status: PatrolStatus,  // Complete / Partial / Error
    pub summary: String,
    pub executed_actions: Vec<AutoExecutedAction>,
    pub pending_proposals: Vec<PendingProposal>,
    pub error: Option<String>,
}
```

跨领域回调在 Orchestrator::start() 中注册：

| 源事件 | 回调动作 |
|--------|----------|
| `AlarmCreated` (level ≥ Error) | → PatrolManager.wake() |
| `ChatCompleted` | → MemoryService.reflect_conversation_turn()（需 model 字段） |
| `PatrolCompleted` | → AgentActionRepository.insert() |
| `WorkspaceCreated` | → PatrolManager.start(workspace_id) |
| `WorkspaceDeleted` | → PatrolManager.stop(workspace_id) |

**注意：** WorkspaceService 不再直接持有 HeartbeatManager。Workspace CRUD 发布事件，Orchestrator 回调管理 patrol 生命周期。

## 依赖注入与应用组装

`cloud/src/server.rs` 中 `build_ai_system()` 一次性构建：

1. 基础层：Repository 实例（SQLite 实现）
2. 服务层：每个 Service 只接收自己需要的 Repo
3. Agent 引擎：AgentPool（只做池管理 + Agent 构建）
4. Chat：ChatService（只做对话）
5. Patrol：PatrolManager（只做巡检循环管理）
6. Orchestrator：最后组装，注册所有跨领域回调

### 改进

| 当前做法 | 新做法 |
|----------|--------|
| `OnceLock::new()` + `set_xxx()` 延迟注入 | 构造函数注入，编译期保证依赖完整 |
| `RwLock<Option<Arc<...>>>` 可选依赖 | 用 `Option` 直接传参 |
| 启动顺序靠「谁先 set_xxx」隐式决定 | `build_ai_system()` 里顺序一目了然 |
| 服务之间互相持有引用（耦合） | 服务只依赖自己的 Repo，不持有其他 Service |
| `alarm_service` 持有 `OnceLock<Arc<HeartbeatManager>>` | `AlarmService` 完全不感知 `PatrolManager` |
| `chat_service` 内部 spawn 调 reflect | `ChatService` 只发 `ChatCompleted` 事件 |

## 行数预估

| 模块 | 当前行数 | 重构后预估 |
|------|----------|-----------|
| agent/ | ~800 分散 | ~500 |
| session/ | ~400 分散 | ~400 |
| patrol/ | ~1310 | ~900 |
| alarm/ | ~1790 | ~1200 |
| tool/ | ~554 | ~500 |
| memory/ | ~392 | ~350 |
| event/ | 新 | ~200 |
| orchestrator/ | 新 | ~200 |
| **总计** | **~12000** | **~4250** |

## TrustConfig 唯一数据源

当前 TrustConfig 存在三处副本（HeartbeatManager DashMap、AgentPool DashMap、`workspaces.heartbeat_trust_config` DB 列），
重构后统一：

- **DB（`workspaces.heartbeat_trust_config` 列）为唯一数据源**
- `PatrolManager::start(ws_id)` 时从 DB 加载并缓存到内存
- `AgentPool::build_agent()` 时从 PatrolManager 获取（不再维护自己的 DashMap）
- `update_trust_config()` 写 DB 后刷新 PatrolManager 内存缓存

## 不再内置 MCP 模块

当前 `cloud/src/modules/mcp/` 作为设备操作工具的实现层保持不变，由 `ai/tool/registry.rs` 通过 trait 接口引用。`tinyiothub-ai` crate 不重新实现 MCP 协议。

## Shutdown 顺序

`Orchestrator::shutdown()` 保证事件不丢失：

1. 设置 `shutting_down` 标志，拒绝新的 patrol loop 启动
2. 向所有活跃 patrol loop 发送 cancel token，等待最多 30 秒
3. 所有 loop 排空后，drop broadcast sender
4. Repo 最后释放（Arc 引用计数，顺序由 Rust 自动管理）

## Dead-Letter Queue（事件持久化失败兜底）

`PatrolCompleted → ActionRepo::insert()` 失败时的处理链：

1. 重试 3 次，指数退避（100ms → 1s → 10s）
2. 3 次均失败 → 写入 `lost_events` 表（event_type, payload_json, error, created_at）
3. Cron 任务每小时重试 `lost_events` 表中 24 小时内的记录
4. 超过 24 小时的记录标记为 `abandoned`，保留 90 天后清理

## Partial HealingReport（LLM 调用中途失败）

patrol_loop 中 LLM 调用失败时保存部分结果：

- LLM 调用包裹在 `tokio::time::timeout` + `catch_unwind` 中
- 失败时构造 `HealingReport { status: "partial", error, executed_actions }` 
- 已执行的动作不丢失，未完成的提案保留为 pending
- 作为 `PatrolCompleted` 事件发布，ActionRepo 记录为 `EventType::Error`

## Security

### TrustConfig 工作空间隔离

- `PatrolManager::start(ws_id)` 时从 `workspace_settings` 表加载该工作空间的 TrustConfig
- Tool 执行上下文中校验 `workspace_id` 匹配
- 测试用例：workspace_A 的 TrustConfig 不得影响 workspace_B 的 patrol loop

### Prompt Injection 防御（memory/）

- Reflection 输入截断到 32K chars
- 过滤匹配注入模式的行（以 `You are`、`System:`、`Instructions:` 等开头的行）
- Profile 编译时对 device_id、workspace_name 做 sanitize
- Digest 使用结构化 JSON 输入，不做自由文本拼接

## Test Strategy

| 层 | 策略 | 工具 |
|---|------|------|
| Repo 测试 | 内存 SQLite，测试所有 CRUD 操作 | `sqlx::SqlitePool` + `#[cfg(test)]` |
| Service 测试 | Mock Repo trait，测试业务逻辑 | `mockall` 或手写 mock struct |
| Integration 测试 | 真实 EventBus channel，验证事件链路 | `tokio::test` |
| 迁移测试 | `alarm_handler_tests` 改为发布 `AlarmCreated` 事件而非直接调用 `wake()` | 现有测试框架 |
| TrustConfig 隔离 | 验证跨 workspace 不泄漏 | 新测试 |

覆盖目标：新 crate 80%+ line coverage。

## Observability

- **EventBus:** 每个事件带有 `trace_id`，span 覆盖 publish → receive → process 全链路
- **Metrics 计数器:** `events_published`, `events_dropped`, `events_processed`, `events_errored` per variant
- **Patrol loop:** LLM 调用延迟 histogram，成功/失败率
- **Critical alert:** `events_dropped > 0` 在 5 分钟窗口 → 触发告警

## Deployment

1. **Pre-deploy:** 确保以下 migration 已应用：
   - `20260625000002_drop_healing_executions.sql`
   - `20260625000003_create_lost_events.sql`（新建：event_type, payload_json, error, created_at）
2. **Build:** `cargo build -p tinyiothub-ai && cargo build -p cloud`
3. **Smoke test:** 创建告警 → 验证 `PatrolCompleted` 事件触发 → 验证 ActionRepo 记录写入
4. **Rollback:** `git revert`（纯代码搬迁，无数据迁移，回滚安全）

## 迁移策略

1. 创建空 `crates/tinyiothub-ai/` crate，建立目录结构
2. 将 `AiEvent` 变体加入 `tinyiothub_runtime::Event` 枚举（扩展复用现有 EventBus）
3. 逐个领域迁移（按依赖顺序：event → types → patrol → alarm → tool → agent → session → memory → orchestrator）
4. 迁移 patrol_loop 时：删除直接 `action_repo.insert()` 调用，改为发布 `PatrolCompleted` 事件
5. 创建 Orchestrator，注册所有跨领域回调（替换 `OnceLock` setter 模式）
6. 更新 `cloud/` 的路由引用
7. 删除旧 `modules/agent/` 等代码

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy (refactor) | 1 | CLEAN | 12 findings, all resolved — event bus sizing, shutdown ordering, dead-letter queue, TrustConfig isolation, prompt injection, shared types, test strategy, observability, deployment, partial HealingReport, model field fix, TrustConfig single source of truth |
| Outside Voice | auto (Claude subagent) | Independent 2nd opinion | 1 | CLEAN | 2 CRITICAL (duplicate EventBus, missing model field), 6 MAJOR — all resolved; switched to extending existing EventBus |

**VERDICT:** CEO + OUTSIDE VOICE CLEARED. Spec is complete with architecture, error handling, security, testing, observability, deployment, and migration strategy. Ready for `/writing-plans`.

NO UNRESOLVED DECISIONS

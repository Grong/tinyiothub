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

## AiEventType 定义（加入 EventType 枚举）

`tinyiothub_core::models::event::EventType` 新增第三变体：

```rust
pub enum EventType {
    System(SystemEventType),
    Device(DeviceEventType),
    Ai(AiEventType),  // 新增
}

pub enum AiEventType {
    AlarmCreated,
    AlarmResolved,
    PatrolCompleted,
    ChatCompleted,
    WorkspaceCreated,
    WorkspaceDeleted,
}
```

### EventType 迁移检查清单

添加 `Ai` 变体后，以下 ~23 个 match 站点需增加 `Ai(_)` arm（编译器会强制检查，此清单确保不遗漏行为决策）：

| 文件 | 行 | 需增加的 arm 行为 |
|------|-----|------|
| `event_type.rs` | 65-66 | `type_string()` → `"ai"` |
| `event_type.rs` | 73,79 | `subtype_string()` → AiEventType 映射 |
| `event_type.rs` | 99,107,115,123 | `is_property_event/is_command_event/is_alarm/is_normal` → `false` |
| `event_type.rs` | 130-155 | `from_strings()` → 新增 `"ai"` 分支 + AiEventType 解析 |
| `event.rs` | 119-127 | `should_update_real_time_status()` → `false`（Ai 事件不触发实时状态更新） |
| `event.rs` | 133-156 | `validate()` → Ai 事件无额外校验（pass-through） |
| `event.rs` | 159-169 | source validation → Ai 事件 source 不强制 device_id |
| `event_repository_impl.rs` | 40-41,237-238 | DB type_string → `"ai"` |
| `access_control.rs` | 94,120,147,173 | 权限门控 → Ai 事件仅管理员可见 |
| `event/service.rs` | 187,200,214-230,404,420-421 | 分类/过滤/验证 → Ai 事件归类为 `EVENT_CATEGORY_AI` |
| `event/handler/real_time.rs` | 286-291,332-337 | 序列化 → Ai EventType 映射 |
| `event/handler/query.rs` | 189-190 | 查询展示 → AiEventType display_name |
| `device/handler/profile.rs` | 215-216 | display_name → `"AI"` |
| `alarm/service.rs` | 331,1052 | matches! 检查 → Ai 事件不匹配 Device |
| `persistence_handler.rs` | 80 | handler 分发 → Ai 事件跳过持久化 handler |
| `data_server.rs` | 425,477 | 设备事件处理 → Ai 事件跳过 DataServer |

## HEARTBEAT.md → DB 迁移

修复当前 `heartbeat.rs:65-68` 文档化的 TOCTOU 竞态条件：

```sql
CREATE TABLE IF NOT EXISTS heartbeat_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'low',
    text TEXT NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, id)
);
```

- `version` 列实现乐观锁：更新时 `WHERE version = ?` 并 `SET version = version + 1`
- `PatrolManager::start()` 时从 DB 加载任务，不再依赖文件系统 HEARTBEAT.md
- 写入/更新通过 Repo trait，Service 层不碰 SQL

## 领域错误类型

每个领域定义 `thiserror` 枚举（内部 helper 可保留 `anyhow::Result`）：

```rust
// patrol/types.rs
#[derive(Debug, thiserror::Error)]
pub enum PatrolError {
    #[error("Agent pool error: {0}")]
    Agent(String),
    #[error("LLM call failed after {retries} retries: {source}")]
    LlmFailure { retries: u32, source: String },
    #[error("Action persistence failed: {0}")]
    ActionPersistence(String),
    #[error("TrustConfig load failed for workspace {workspace_id}: {source}")]
    TrustConfig { workspace_id: String, source: String },
}

// alarm/types.rs, agent/types.rs, session/types.rs … 同理，各 4-8 个变体
```

EventBus 错误传播规则：handler 失败只记录 `error!` 日志 + 递增 `events_errored` 计数器，**绝不**向事件发布者反向传播错误（fire-and-forget 语义）。

## Shutdown 顺序

`Orchestrator::shutdown()` 保证事件不丢失：

1. 设置 `shutting_down` 标志，拒绝新的 patrol loop 启动
2. 向所有活跃 patrol loop 发送 cancel token，等待最多 30 秒
3. 所有 loop 排空后，`Arc<EventBus>` 随 Orchestrator drop 自然释放（不手动 drop broadcast sender — EventBus 被 SseEventHandler/AlarmEventHandler/DataServer 共享）
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

## Performance

- **SQLite WAL 模式:** sqlx 默认启用 WAL，允许并发读 + 单写者。多个 patrol loop 同时写入 `agent_actions` 时竞争写锁——每个 patrol tick 使用单个写事务批量插入（`INSERT INTO agent_actions (...) VALUES (...), (...), ...`），将锁获取次数从 N 次降为 1 次。
- **broadcast channel capacity:** 1000，与现有 EventBus 共享。AiEvent 发布失败时记录 `warn!` + 递增 `events_dropped` 计数器。5 分钟窗口内 `events_dropped > 0` 触发 Critical alert。
- **DashMap 分片:** HeartbeatManager/AgentPool 的 per-workspace 查找使用 DashMap（sharded lock-free reads），workspace 数量 < 1000 时无明显性能瓶颈。

## NOT in scope

- **Cron retry worker for lost_events:** 表结构和 3x 内联重试逻辑在 scope 内；定时 cron 任务（每小时扫描 `lost_events` 表）推迟到 follow-up PR。
- **HEARTBEAT.md → DB 管理 UI:** DB 表迁移在 scope 内；管理后台 UI 继续使用文件系统 HEARTBEAT.md 编辑，推迟到后续 PR。
- **A2UI bidirectional binding:** 前次 CEO Review 中接受的扩展，不在本次重构范围内。tool/catalog.rs 基础设施预留支持。

## What already exists

| 组件 | 现有位置 | 重构后如何复用 |
|------|----------|---------------|
| EventBus | `tinyiothub_runtime::event_bus.rs` (170 行) — broadcast channel + ArcSwap handler 分发 | 扩展 AiEvent 变体，不替换 |
| Event 模型 | `tinyiothub_core::models::event/` — Event struct + EventType enum | AiEventType 作为第三变体加入 |
| AgentPool | `cloud/src/modules/agent/agent.rs` (796 行) | 拆分为 agent/pool.rs + builder.rs + service.rs |
| HeartbeatManager | `cloud/src/modules/agent/heartbeat_manager.rs` (383 行) | 移至 patrol/manager.rs，增加 DB 加载 TrustConfig |
| heartbeat_loop | `cloud/src/modules/agent/heartbeat.rs` (447 行) | 移至 patrol/loop.rs，从直接 insert 改为发布 PatrolCompleted 事件 |
| Tool service | `cloud/src/modules/agent/tools/service.rs` (554 行) | 移至 tool/registry.rs + trust.rs，增加 ToolDependencyProvider trait |
| Memory/reflect | `cloud/src/modules/agent/reflect.rs` (392 行) | 移至 memory/reflect.rs，由 ChatCompleted 事件触发而非 inline spawn |
| Alarm service | `cloud/src/modules/alarm/service.rs` | 保留在 alarm/，移除 OnceLock<HeartbeatManager>，发布 AlarmCreated 事件 |
| Workspace service | `cloud/src/modules/workspace/` | 移除 OnceLock<HeartbeatManager>，发布 WorkspaceCreated/Deleted 事件 |
| lost_events 表 | `cloud/migrations/20260625000003_create_lost_events.sql` | 已创建——原样复用 |

## Failure modes

| 故障 | 测试? | 错误处理? | 用户可见? |
|------|-------|-----------|----------|
| PatrolCompleted → ActionRepo insert 失败（DB 锁定） | GAP | Dead-letter queue: 3x 重试 → lost_events 表 | 静默（cron 1h 内恢复） |
| LLM 调用超时（mid-patrol） | GAP | 保存部分 HealingReport，错误记录为 EventType::Error | 错误计数在 action history 中可见 |
| EventBus channel 满（capacity 1000） | GAP | `warn!` 日志 + `events_dropped` 计数器递增 | 5 分钟窗口内 >0 时触发 Critical alert |
| PatrolManager::start 时 TrustConfig DB 加载失败 | GAP | 回退到 ApprovalRequired 默认值（所有工具安全默认） | 无自动执行——安全默认 |
| ChatCompleted 事件丢失（channel 满） | GAP | 该轮 Memory reflection 跳过 | 静默——记忆稍不完整 |
| 同一 workspace 重复 patrol loop（意外重复启动） | PARTIAL | HeartbeatManager::start() 先调用 stop()（幂等） | 无重复 loop |

## Worktree parallelization

重构按依赖顺序执行，但 3 个 lane 可在 foundation 完成后并行：

| Lane | 步骤 | 涉及模块 |
|------|------|---------|
| A | event/types foundation → patrol → alarm | tinyiothub-core (EventType), tinyiothub-ai (patrol/, alarm/) |
| B | tool → agent | tinyiothub-ai (tool/, agent/) |
| C | session → memory → orchestrator | tinyiothub-ai (session/, memory/, orchestrator/) |

执行顺序: Lane A 先完成（foundation）。然后 B + C 并行。最后集成修改 cloud/ 路由挂载。

Lane B 和 C 无共享模块——可安全并行 worktree。Lane A 必须在 B 或 C 启动前完成。

## Integration test scenarios

1. **Alarm → Patrol wake 完整链路:** 创建告警 → EventBus 发布 AlarmCreated → Orchestrator 回调唤醒 patrol → patrol loop tick 执行 → PatrolCompleted 事件发布 → ActionRepo 写入。验证真实 broadcast channel。
2. **Dead-letter queue 重试 + 回退:** Mock 一个始终失败的 ActionRepo → 验证 3x 指数退避重试 → 验证写入 `lost_events` 表（status='pending'）。
3. **ChatCompleted → Memory reflection:** 发送 chat 消息 → 验证 ChatCompleted 事件发布（含 model 字段）→ 验证 MemoryService.reflect_conversation_turn() 被调用。

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
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAN | 12 findings, all resolved — event bus sizing, shutdown ordering, dead-letter queue, TrustConfig isolation, prompt injection, shared types, test strategy, observability, deployment, partial HealingReport, model field fix, TrustConfig single source of truth |
| Outside Voice | auto (Claude subagent) | Independent 2nd opinion | 1 | CLEAN | 2 CRITICAL (duplicate EventBus, missing model field), 6 MAJOR — all resolved; switched to extending existing EventBus |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAN | 7 findings, all resolved — AiEvent → EventType::Ai with ~23-site migration checklist, HEARTBEAT.md TOCTOU → DB migration, shutdown sender drop fixed, error type sketches per domain, integration test scenarios (3), SQLite WAL + batch insert note, EventType match-site checklist |
| Outside Voice | auto (Claude subagent) | Independent 2nd opinion (eng) | 1 | ISSUES_FOUND | 3 CRITICAL, 2 MAJOR — all resolved; kept separate crate per user decision, added EventType migration checklist per outside voice recommendation, verified circle dependency is broken by EventBus in proposed architecture |

**CROSS-MODEL:** Outside voice flagged EventType blast radius (~50+ sites claimed, ~23 actual) and crate circular dependency (based on current code, not proposed EventBus-broken architecture). Both resolved — checklist added to spec, crate extraction kept with user confirmation.

**VERDICT:** CEO + ENG + OUTSIDE VOICE CLEARED. Spec hardened with architecture fixes, error type definitions, integration test scenarios, EventType migration checklist, HEARTBEAT.md DB migration, performance notes, NOT in scope, failure modes, and worktree parallelization. Ready for `/writing-plans`.

NO UNRESOLVED DECISIONS

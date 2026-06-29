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

## 依赖规则

```
orchestrator 可以依赖所有领域（它是协调者）
其他领域之间不直接依赖，通过 orchestrator 的事件回调通信
每个领域内部保持 Handler → Service → Repo 三层
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

## Orchestrator 设计

通过 `tokio::sync::broadcast` 实现 AI 系统内部事件总线：

```rust
pub enum AiEvent {
    AlarmCreated(Alarm),
    AlarmResolved { alarm_id: String, device_id: String, rule_id: Option<String> },
    PatrolCompleted { workspace_id: String, report: PatrolReport },
    ChatCompleted { workspace_id: String, agent_id: String, session_key: String, messages: Vec<ChatTurnMessage> },
}
```

跨领域回调在 Orchestrator::start() 中注册：

| 源事件 | 回调动作 |
|--------|----------|
| `AlarmCreated` (level ≥ Error) | → PatrolManager.wake() |
| `ChatCompleted` | → MemoryService.reflect_conversation_turn() |
| `PatrolCompleted` | → ActionRepository.insert() |

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

## 不再内置 MCP 模块

当前 `cloud/src/modules/mcp/` 作为设备操作工具的实现层保持不变，由 `ai/tool/registry.rs` 通过 trait 接口引用。`tinyiothub-ai` crate 不重新实现 MCP 协议。

## 迁移策略

1. 创建空 `crates/tinyiothub-ai/` crate，建立目录结构
2. 逐个领域迁移：types → repo → service，每完成一个编译通过
3. 创建 Orchestrator，逐步替换跨模块直接调用
4. 更新 `cloud/` 的路由引用
5. 删除旧 `modules/agent/` 等代码

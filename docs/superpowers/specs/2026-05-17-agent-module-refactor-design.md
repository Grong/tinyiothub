# Agent 模块重构设计

## 目标

深度重构 AI Agent 模块：以"能力为核心"组织代码，消除重复，删除不必要的抽象层。

## 当前问题

| 问题 | 位置 |
|------|------|
| `runtime.rs` 1625 行 God File | shared/agent/ |
| Session key 解析 3 份重复实现 | types.rs + runtime.rs + proxy.rs |
| Skill 加载 async/sync 两份重复 | mod.rs + runtime.rs |
| Tool catalog 静态+动态两份 | mod.rs + runtime.rs |
| 三层 trait（AgentClient → AgentRuntime → ChatService）只一个实现 | shared/agent/ |
| Agent handler 委托到 chat proxy | modules/agent/handler → modules/chat/handler/proxy |
| 双向依赖：shared/agent → modules/agent | heartbeat → ChatService |
| CanvasTool 按名字硬编码特殊处理 | runtime.rs |

## 新架构

### 模块结构

```
shared/agent/
├── mod.rs         # 公共 trait（空或被其他模块引用的接口）
└── types.rs       # SessionKey + AgentError + AgentRuntimeConfig + AgentConfig

modules/agent/
├── mod.rs         # 模块入口 + 统一 router 组装（/chat、/agents、/tools 前缀在此挂载）
├── agent.rs       # Agent 结构体 + AgentPool + zeroclaw Agent 构建
├── types.rs       # ChatEvent、ChatRequest 等领域类型
│
├── chat/           # Chat 能力（唯一有独立 handler 子目录的能力）
│   ├── mod.rs
│   ├── service.rs  # ChatEvent 流处理（stateless，zeroclaw Agent 作为参数传入）
│   └── handler.rs  # /chat/stream, /chat/history, /chat/abort, /chat/sessions
│
├── tools/          # Tool 能力
│   ├── mod.rs
│   ├── types.rs    # ToolDef, ToolGroup, ToolCatalog
│   ├── service.rs  # MCP 加载、denylist 过滤、catalog 构建
│   ├── canvas.rs   # CanvasTool (A2UI)
│   └── handler.rs  # /tools/catalog, /tools/effective, /tools/toggle
│
├── config/         # Config 能力
│   ├── mod.rs
│   ├── service.rs  # AgentRuntimeConfig DB 读写 + 缓存失效
│   └── handler.rs  # /agents/:id/config, GET /agents
│
├── heartbeat.rs    # Heartbeat 能力（单文件，从 shared/agent/ 移入）
├── skills.rs       # Skills 能力（单文件，async + sync 两份实现共享缓存）
├── memory.rs       # Memory 能力（单文件，NamespacedMemory）
└── scaffold.rs     # Scaffold 能力（单文件，工作区初始化 + workspace files CRUD）
```

### Agent + AgentPool（modules/agent/agent.rs）

```rust
/// Agent = 能力服务的组合容器
pub struct Agent {
    pub agent_id: String,
    pub workspace_id: String,
    pub config: AgentRuntimeConfig,
    pub tools: ToolService,
    pub memory: MemoryService,
    // ChatService 不在 Agent 内 — 它是 stateless 的，zeroclaw Agent 作为参数传入
}

pub struct AgentPool {
    agents: Arc<DashMap<String, PoolEntry>>,
    db_pool: SqlitePool,
    shared_memory: Arc<dyn zeroclaw::Memory>,
    observer: Arc<dyn zeroclaw::Observer>,
    response_cache: Option<Arc<zeroclaw::ResponseCache>>,
    agent_settings: AgentSettings,
}

struct PoolEntry {
    /// zeroclaw Agent（由 AgentPool 构建）
    zeroclaw_agent: Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
    /// TinyIoTHub Agent 元数据
    metadata: Agent,
    last_used: Instant,
}
```

- `get_or_create(agent_id, workspace_id)` — lazy build：读 DB config → 构建 zeroclaw Agent + Agent metadata → 缓存
- `build_zeroclaw_agent(&self, config, workspace_dir, tools)` — zeroclaw Agent 构建逻辑集中在此
- `invalidate(agent_id)` — config/tool 变更后淘汰，下次访问重建
- `cleanup_idle()` — 清理空闲 30 分钟以上的 Agent
- ChatService 是 stateless 的，`send_message(zeroclaw_agent, message, ...)` 接收 zeroclaw Agent 引用

### 调用链精简

```
当前 (5 层):
  Router → proxy.rs → ChatService → AgentRuntimeImpl (trait dispatch) → zeroclaw Agent

重构后 (3 层):
  Router → ChatHandler → ChatService → zeroclaw Agent
```

### SessionKey 统一（modules/agent/session.rs）

```rust
pub struct SessionKey {
    pub workspace_id: String,
    pub agent_id: String,
    pub session_uuid: String,
}

impl SessionKey {
    pub fn parse(key: &str) -> Result<Self, AgentError>;
    pub fn to_string(&self) -> String;
    pub fn verify_workspace(&self, expected: &str) -> Result<(), AgentError>;
}
```

唯一解析入口，消灭 3 份重复。

### 消除的 SSE 序列化往返

```
当前:
  zeroclaw TurnEvent → runtime 转 bytes → reqwest::Response
    → ChatService 解析 bytes → ChatEvent → proxy 转 JSON SSE → HTTP

重构后:
  zeroclaw TurnEvent → ChatService 转 ChatEvent → ChatHandler 转 SSE → HTTP
```

ChatService 直接返回 `UnboundedReceiverStream<ChatEvent>`，省掉中间序列化/反序列化。

### Tool catalog 统一

`ToolService::build_catalog()` 是唯一 catalog 来源，从 MCP registry 动态构建。`build_tools_catalog_json()`（静态硬编码）和 `build_dynamic_catalog()` 删除。

CanvasTool 注册到 MCP registry，和其他工具走同样的注册→过滤流程，不再按名字特殊处理。

### Skill 加载合并

`load_skills_prompt()` (async) 和 `load_skills_sync()` (sync) 保持两份实现（避免 tokio 上下文 block_on 死锁风险），但共享文件内容缓存（OnceCell）。async 版本用 `tokio::fs`，sync 版本用 `std::fs`。缓存避免重复读盘。

### Heartbeat 去耦合

HeartbeatService 从 `shared/agent/` 移到 `modules/agent/heartbeat/`，不再通过 `Arc<ChatService>` 反向依赖，改为直接依赖 `AgentPool`。

### System prompt 构建

`build_full_system_prompt()`、`load_workspace_prompt()`、`load_template_fallback()` 等函数从 `shared/agent/mod.rs` 移到 `modules/agent/skills/service.rs`，与 skill 加载逻辑汇合。

### 路由迁移

| 原路由 | 原所属 | 新路由 | 新 handler |
|--------|--------|--------|------------|
| `/chat/stream` | modules/chat/ | 不变 | agent/chat/handler |
| `/chat/history` | modules/chat/ | 不变 | agent/chat/handler |
| `/chat/abort` | modules/chat/ | 不变 | agent/chat/handler |
| `/chat/sessions` | modules/chat/ | 不变 | agent/chat/handler |
| `/agents` | modules/agent/ → proxy | 不变 | agent/config/handler |
| `/agents/:id/config` | modules/agent/ → proxy | 不变 | agent/config/handler |
| `/agents/:id/heartbeat/*` | modules/agent/ | 不变 | agent/heartbeat 内部 handler |
| `/agents/:id/files` | modules/agent/ | 不变 | agent/scaffold/handler |
| `/agents/:id/files/:name` | modules/agent/ | 不变 | agent/scaffold/handler |
| `/tools/*` | 未挂载到 router | `/tools/catalog` 等 | agent/tools/handler |

### AppState 变化

```rust
// 删除
chat_service: Arc<ChatService>,
agent_runtime: Arc<dyn AgentRuntime>,

// 新增
agent_pool: Arc<AgentPool>,
```

## 删除的代码

| 文件/内容 | 原因 |
|-----------|------|
| `shared/agent/runtime.rs` (1625 行) | 拆入 chat/tools/config/memory/skills |
| `shared/agent/heartbeat_service.rs` (351 行) | 移到 modules/agent/heartbeat/ |
| `shared/agent/scaffold_service.rs` (138 行) | 移到 modules/agent/scaffold/ |
| `modules/chat/handler/proxy.rs` (286 行) | 逻辑回归各自能力 handler |
| `modules/agent/chat_service.rs` (233 行) | 合并到 chat/service.rs |
| `modules/agent/handler/mod.rs` (agent 委托) | 不再需要委托到 proxy |
| `AgentClient` trait | 只有一个实现，不需要 trait |
| `AgentRuntime` trait | 同上 |
| `AgentRuntimeImpl` struct | 替换为 Agent + AgentPool |
| `build_tools_catalog_json()` | 统一到 ToolService |
| `build_dynamic_catalog()` | 同上 |
| `parse_workspace_id()` | 统一到 SessionKey |
| `extract_workspace_from_session_key()` | 同上 |
| `ParsedSessionKey` | 替换为 SessionKey |
| `IoTToolAdapter` + `NativeToolDispatcher` | 移到 tools/service.rs 内部 |
| `TinyIoTHubSkillsSection` | 移到 skills/service.rs |

### Router 组装（modules/agent/mod.rs）

所有路由在 `modules/agent/mod.rs` 中统一组装为一个 `Router<AppState>`：

```rust
pub fn create_router() -> Router<AppState> {
    Router::new()
        .nest("/chat", chat::handler::create_router())
        .nest("/agents", config::handler::create_router())
        .nest("/tools", tools::handler::create_router())
        // heartbeat、scaffold 路由也在此挂载
}
```

`server.rs` 只需挂载一个 agent router 而非多个分散的子 router。

### 测试组织

测试内联到各能力 service/handler 文件（`#[cfg(test)] mod tests`）。`agent_handler_tests.rs` 中的集成测试拆入对应 handler 文件。

---

## 不变的文件

- 数据库 schema（agent_configs、chat_sessions、chat_messages 等）
- 前端代码
- server.rs（只改 AppState 字段名）
- app_state.rs（只改构造逻辑）
- MCP handler 注册逻辑
- zeroclaw 依赖

## 验证

1. `cargo build` 编译通过
2. `cargo test` 全部测试通过
3. `cargo clippy` 无新增警告
4. 前端功能回归：chat 流式对话、历史、工具调用、agent 配置修改、心跳巡检

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAN | 4 issues found, 4 resolved |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAN | 8 issues found, 8 resolved |
| Outside Voice | `/plan-eng-review` | Independent plan challenge | 1 | ISSUES | 4 critical, 3 major, 3 minor — all resolved |

**CROSS-MODEL:** Eng review vs Outside Voice agreed on plan direction but outside voice found 3 structural gaps the review missed: external consumer inventory (10+ files, not 2), AgentPool API surface (missing 7 methods), and HeartbeatService redesign (one sentence → full flow needed). All gaps now addressed in plan.

**VERDICT:** CEO + ENG + OUTSIDE VOICE CLEARED — ready for implementation.

### CEO Review Decisions

| # | Finding | Decision |
|---|---------|----------|
| 1A | ChatService-zeroclaw 桥接 | ChatService stateless，zeroclaw Agent 作为参数传入 |
| 1B/1C | Router 组装 + 文件粒度 | agent/mod.rs 统一组装，小能力单文件不建子目录 |
| 5A | Skill async/sync 合并 | 两份实现共享 OnceCell 缓存，避免 block_on 死锁 |
| 6A | 测试组织 | 测试内联到各能力 service/handler 文件 |

### Eng Review Decisions

| # | Finding | Decision |
|---|---------|----------|
| D2 | `get_or_create` race condition | Serialize with DashMap entry API only |
| D3 | Chat abort handles placement | Add `chat_handles: Arc<DashMap<String, AbortHandle>>` to AgentPool |
| D4 | Skills OnceCell cache invalidation | Add TTL-based cache refresh (5-min TTL) |
| D5 | Skills file vs directory inconsistency | Single file `skills.rs` (~200 lines) |
| D6 | Tool catalog fallback source | Derive static catalog from MCP handler struct definitions |
| D7 | AgentError missing StreamError | Add `StreamError(String)` variant to AgentError |
| D8 | ChatService/ChatHandler zero tests | Add ChatService unit tests now; defer ChatHandler HTTP tests to T9 |
| D9 | AgentPool unbounded growth | No max size — 30-min idle cleanup sufficient for this scale |

### Outside Voice Decisions (Claude critic subagent)

| # | Finding | Decision |
|---|---------|----------|
| D10 | 10+ external consumers not inventoried | Add T7b: inventory all files referencing deleted/moved types |
| D11 | AgentPool missing 7 API methods | Add create_agent, delete_agent, list_agents, refresh_tools, get_config, set_config, tools_* to AgentPool |
| D12 | HeartbeatService redesign is one sentence | Specify full flow: AgentPool::get_or_create → stateless ChatService::send_message → handle stream |
| D13 | CanvasTool (zeroclaw Tool) ≠ MCP registry ToolHandler | Keep CanvasTool as separate ToolBox source in ToolService, not in MCP registry |
| — | 4th session parsing copy (SessionRepository) | Include in T6: SessionRepository::get_or_create() uses SessionKey::parse() |
| — | NamespacedMemory vs AgentMemoryService confusion | Clarify: memory.rs wraps AgentMemoryService (device snapshots), NamespacedMemory stays in AgentPool |
| — | api/mod.rs tools routes | Remove tools routes from api/mod.rs; tools live under agent router nest only |
| — | load_skills_sync() with OnceCell | Sync path reads from SkillsCache (populated on first access), not directly from disk |

### NOT in scope
- 不改变任何外部 API 行为
- 不提取 agent 能力为独立 crate
- 不引入新的 trait 抽象
- ChatHandler HTTP 集成测试（推迟到 T9）
- AgentPool LRU eviction / max size bound

### What already exists
- zeroclaw Agent builder、Memory、Observer、ResponseCache — 复用不变
- MCP handler registry — 复用不变
- DB schema（agent_configs, chat_sessions, chat_messages）— 不变

### Worktree Parallelization Strategy

| Step | Modules touched | Depends on |
|------|----------------|------------|
| T1 | modules/agent/ (new) | — |
| T2 | shared/agent/ (existing) | — (different dir) |
| T3 | modules/agent/chat/ | T1, T2 |
| T4 | modules/agent/tools/ | T1, T2 |
| T5 | modules/agent/config/ | T1, T2 |
| T6 | shared/agent/types.rs | T2 |
| T7 | app_state.rs, server.rs | T1, T2, T5 |
| T8 | deletion: shared/agent/, modules/chat/, modules/agent/ | T3, T4, T5 |
| T9 | all capability files (tests) | T1-T8 |

```
Lane A: T1 (modules/agent/ structure)
Lane B: T2 (shared/agent/ cleanup)           ← parallel with Lane A (different dirs)
────────── merge ──────────
Lane A: T3 (Chat capability)
Lane B: T4 (Tool capability)                  ← parallel, different subdirs
Lane C: T5 (Config capability)
Lane D: T6 (SessionKey)
────────── merge ──────────
Lane A: T7 (AppState + server.rs)
Lane B: T8 (delete old code)                  ← parallel after T3+T4+T5
────────── merge ──────────
Lane A: T9 (tests)
```

3 parallel lanes at peak. No conflict flags — T3/T4/T5 touch different subdirectories.

### Test Coverage Diagram (Eng Review)

```
CODE PATHS (new architecture)                              TEST STATUS
modules/agent/agent.rs
  AgentPool::get_or_create()
  ├── [GAP] [REGRESSION] Pool miss → DB read → build     (was implicitly tested via old runtime tests)
  ├── [GAP]                Pool hit → return cached
  ├── [GAP]                Concurrent get_or_create race
  └── [GAP]                DB read fails → fallback default config
  AgentPool::build_zeroclaw_agent()
  ├── [GAP]                Valid config → zeroclaw Agent
  ├── [GAP]                Build fails → AgentError::BuildError
  └── [GAP]                Denylist-filtered tools

modules/agent/chat/service.rs                    ← ZERO existing tests
  ChatService::send_message()
  ├── [GAP] [CRITICAL]     Happy path: message → SSE stream
  ├── [GAP] [CRITICAL]     Tool call in stream
  ├── [GAP]                Stream error → AgentError::StreamError
  └── [GAP]                Client disconnect mid-stream

modules/agent/chat/handler.rs                    ← ZERO existing tests (proxy.rs: 0)
  ├── [GAP] [CRITICAL]  POST /chat/stream → SSE response
  ├── [GAP] [CRITICAL]  GET  /chat/history → messages JSON
  ├── [GAP]             POST /chat/abort → cancel run
  └── [GAP]             GET  /chat/sessions → session list

modules/agent/tools/service.rs
  ├── [★★  TESTED→MOVE]    Falls back when registry empty — runtime.rs:1599
  ├── [★★  TESTED→MOVE]    Destructive tools disabled — runtime.rs:1490
  ├── [★★  TESTED→MOVE]    Read-only tools enabled — runtime.rs:1498
  ├── [GAP]                MCP registry populated → dynamic catalog
  └── [GAP]                Custom denylist filtering

modules/agent/tools/canvas.rs
  ├── [★★★ TESTED→MOVE]    Name/description/schema — runtime.rs:1520-1528
  ├── [★★★ TESTED→MOVE]    Execute a2ui_push — runtime.rs:1538
  └── [★★★ TESTED→MOVE]    Execute unknown action — runtime.rs:1550

modules/agent/tools/handler.rs                   ← ZERO existing tests
modules/agent/config/service.rs                  ← NEW (no prior tests)
modules/agent/config/handler.rs                  ← ZERO existing tests
modules/agent/skills.rs
  ├── [★   TESTED→MOVE]    Skill frontmatter parse — skill.rs:2 tests
  ├── [★★  TESTED→MOVE]    Skill file serving — handler/skills.rs:9 tests
  ├── [GAP]                SkillsCache TTL invalidation
  ├── [GAP]                build_full_system_prompt() layers
  └── [GAP]                load_workspace_prompt() fallback

modules/agent/session.rs                         ← Was in runtime.rs
  ├── [★★  TESTED→MOVE]    parse valid/invalid/missing — runtime.rs:1450-1469
  └── [GAP]                verify_workspace()

modules/agent/heartbeat.rs                       ← ZERO existing tests

COVERAGE: 8/25+ paths tested (32%)  |  Existing tests to migrate: 59
CRITICAL GAPS: 4 (ChatService stream ×2, ChatHandler stream, ChatHandler history)
```

### Updated Failure Modes Registry

| Codepath | Failure Mode | Rescued? | Test? | User Sees |
|----------|-------------|----------|-------|-----------|
| SessionKey::parse() | Malformed key | Y (AgentError) | Yes | 400 error |
| AgentPool::get_or_create() | DB read fails | Y (fallback to default config) | Yes | Works with defaults |
| AgentPool::get_or_create() | zeroclaw build fails | Y (AgentError::BuildError) | Yes | 500 error |
| AgentPool::get_or_create() | Concurrent race (two builds) | Y (DashMap entry API) | No | Duplicate build, benign |
| ChatService::send_message() | zeroclaw turn_streamed fails | Y (AgentError::StreamError) | Yes | SSE error event |
| ChatService::send_message() | Client disconnect mid-stream | Y (receiver drop → task cancel) | No | Silent cleanup |
| ToolService::load_all() | MCP registry empty | Y (fallback from handler structs) | Yes | Catalog shows derived tools |
| ConfigService::set_config() | DB write fails | Y (AgentError) | Yes | 500 error |
| ConfigService::set_config() | In-flight SSE uses old config | Y (Arc keeps old Agent alive) | No | Config takes effect next message |
| HeartbeatService | ChatService call fails | Y (log + retry next interval) | No | Silent (next interval) |
| SkillsCache | Cache stale after file edit | Y (5-min TTL refresh) | Yes | Skills update within 5 min |

### Implementation Tasks
Synthesized from CEO + Eng review findings.

- [ ] **T1 (P1, human: ~4h / CC: ~30min)** — `modules/agent/` — 创建模块结构：AgentPool + 8 个能力文件 + chat/tools/config 子目录
  - Surfaced by: CEO §1, Eng D2/D3/D4/D5 — module structure + AgentPool with chat_handles + skills TTL cache
  - Files: `modules/agent/mod.rs`, `agent.rs` (Agent + AgentPool + PoolEntry + chat_handles), `types.rs`, `skills.rs` (SkillsCache with TTL), `memory.rs`, `heartbeat.rs`, `scaffold.rs`
  - Verify: `cargo build` compiles new module structure

- [ ] **T2 (P1, human: ~3h / CC: ~20min)** — `shared/agent/` — 精简 shared/agent/ 为 types.rs + mod.rs + config.rs
  - Surfaced by: CEO §1, Eng D7 — dependency cleanup + StreamError variant
  - Files: `shared/agent/types.rs` (SessionKey + AgentError::StreamError + AgentRuntimeConfig), `shared/agent/mod.rs`
  - Verify: `cargo build` — shared/agent/ compiles without runtime.rs, heartbeat_service.rs, scaffold_service.rs

- [ ] **T3 (P1, human: ~5h / CC: ~40min)** — Chat 能力 — 实现 ChatService (stateless) + ChatHandler
  - Surfaced by: CEO §1/Finding 1A, Eng D3/D8 — ChatService stateless, abort via AgentPool.chat_handles, unit tests
  - Files: `agent/chat/service.rs` (send_message → UnboundedReceiverStream<ChatEvent>), `agent/chat/handler.rs` (/chat/stream, /chat/history, /chat/abort, /chat/sessions)
  - Verify: SSE chat stream works end-to-end + ChatService unit tests pass

- [ ] **T4 (P1, human: ~3h / CC: ~20min)** — Tool 能力 — ToolService + CanvasTool + catalog 统一
  - Surfaced by: CEO §1, Eng D6 — eliminate dual catalog, CanvasTool in MCP registry, fallback from handler structs
  - Files: `agent/tools/service.rs` (build_catalog, resolve_tools), `agent/tools/canvas.rs`, `agent/tools/handler.rs`, `agent/tools/types.rs`
  - Verify: Tool catalog renders in frontend, tool toggle works

- [ ] **T5 (P1, human: ~2h / CC: ~15min)** — Config 能力 — ConfigService + AgentPool 集成
  - Surfaced by: CEO §1 — config read/write + cache invalidation
  - Files: `agent/config/service.rs`, `agent/config/handler.rs`
  - Verify: Config changes take effect on next chat message

- [ ] **T6 (P1, human: ~2h / CC: ~15min)** — SessionKey 统一 + workspace 校验
  - Surfaced by: CEO §1 — eliminate 3x parse duplication
  - Files: `shared/agent/types.rs` (SessionKey with parse + verify_workspace + to_string)
  - Verify: All session key parsing goes through SessionKey::parse()

- [ ] **T7 (P1, human: ~3h / CC: ~20min)** — AppState + server.rs + 所有外部消费者适配
  - Surfaced by: CEO §1, Eng D10 — remove AgentRuntime trait + ChatService from AppState, add AgentPool
  - Files: `shared/app_state.rs`, `server.rs` (mount unified agent router), `shared/service_manager.rs` (HeartbeatService ctor), `modules/workspace/handler.rs` (create_agent/delete_agent), `modules/system/handler/initialization.rs` (scaffold + agent), `api/mod.rs` (remove tools routes), `main.rs` (refresh_tools → pool)
  - Verify: App boots, all routes respond, workspace creation/deletion works

- [ ] **T7b (P1, human: ~1h / CC: ~10min)** — 外部消费者全局盘点
  - Surfaced by: Outside Voice D10 — 10+ files reference deleted/moved types
  - Files: `grep` for `agent_runtime`, `chat_service`, `HeartbeatService::new`, `scaffold_workspace`, `proxy::tools_*`, `AgentRuntimeImpl`
  - Verify: Zero references to deleted types after T8
  - Surfaced by: CEO §1 — remove AgentRuntime trait + ChatService from AppState, add AgentPool
  - Files: `shared/app_state.rs`, `server.rs` (mount unified agent router)
  - Verify: App boots, all routes respond

- [ ] **T8 (P1, human: ~2h / CC: ~15min)** — 删除旧代码 + 清理 imports
  - Surfaced by: CEO §1 — dead code removal
  - Files: delete runtime.rs, heartbeat_service.rs, scaffold_service.rs, proxy.rs, old chat_service.rs
  - Verify: `cargo build` clean, `cargo clippy` no warnings

- [ ] **T9 (P2, human: ~3h / CC: ~20min)** — 测试迁移 + 补充
  - Surfaced by: CEO §6, Eng D8 — test organization, ChatService unit tests, handler tests deferred
  - Files: inline #[cfg(test)] blocks in all capability files, migrate 59 existing tests
  - Verify: `cargo test` all pass, coverage not reduced
